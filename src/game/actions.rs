use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::{Chest, Collider, Destructible, Faction, InInventory, Inventory, Key, LockedDoorData, MovementMode, Name, Position, Item},
    game::machines::{Machine, MachineBumpMessage},
    constants::BASE_ACTION_COST,
    game::{
        combat::{AttackIntentMessage, DamageType, DamageTypeTag, DamageSource},
        enchantment::{display_item_name, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified},
        factions::FactionMatrix,
        effects::UseItemMessage,
        items::{DropItemMessage, EquipItemMessage, ItemStack, UnequipItemMessage},
        // CastSpellMessage removed (spell system replaced by monster abilities)
        spawner::spawn_item,
        turns::MyTurn,
    },
    map::{Map, tile::{can_entity_enter_tile, TerrainType, TileMutationMessage, TileMarker}},
    map::dungeon::Floor,
    player::Player,
    assets::{
        ItemManifest, ItemManifestHandle, ItemSpawnTable, ItemSpawnTableHandle, ItemSpriteAssets,
    },
    ui::game_log::GameLogMessage,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Wait,
    Move { dir: Direction },
    #[allow(dead_code)]
    MeleeAttack { target: Entity },
    PickUp,
    EquipItem   { item: Entity },
    UnequipItem { item: Entity },
    DropItem    { item: Entity },
    UseItem     { item: Entity },
    /// Fire a ranged weapon at a pre-selected target entity.
    RangedAttack { target: Entity },
    /// Zap a staff at a target.
    ZapStaff { staff_entity: Entity, target: Entity, target_pos: Option<(i32, i32)> },
}

// --- Events ---

#[derive(Message)]
pub struct MovementIntent {
    pub entity: Entity,
    pub dir: Direction,
}

#[derive(Message)]
pub struct MeleeIntent {
    pub attacker: Entity,
    pub target: Entity,
}

#[derive(Message)]
pub struct WaitIntent {
    pub entity: Entity,
}

#[derive(Message)]
pub struct PickUpIntent {
    pub entity: Entity,
}

#[derive(Message)]
pub struct OpenDoorIntent {
    pub entity: Entity,
    pub door_pos: Point,
}

#[derive(Message)]
pub struct OpenChestIntent {
    pub entity: Entity,
    pub chest_entity: Entity,
}

#[derive(Message)]
pub struct UnlockDoorIntent {
    pub entity: Entity,
    pub door_pos: Point,
}

#[derive(Message)]
pub struct RangedAttackIntent {
    pub attacker: Entity,
    pub target: Entity,
}

/// Classifies an action for delay calculation. Movement uses `movement_delay`,
/// attacks use `attack_delay`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionKind {
    #[default]
    Movement,
    Attack,
}

#[derive(Component, Clone)]
pub struct SpeedStats {
    /// Base movement delay set at spawn time. Never overwritten at runtime.
    pub base_movement_delay: f32,
    /// Base attack delay set at spawn time. Never overwritten at runtime.
    pub base_attack_delay: f32,
    /// Effective movement delay = base_movement_delay * status-effect multiplier.
    /// Recomputed each frame by `apply_speed_effects_system`.
    pub movement_delay: f32,
    /// Effective attack delay = base_attack_delay * status-effect multiplier.
    /// Recomputed each frame by `apply_speed_effects_system`.
    pub attack_delay: f32,
}

impl SpeedStats {
    pub fn new(base_movement_delay: f32, base_attack_delay: f32) -> Self {
        Self {
            base_movement_delay,
            base_attack_delay,
            movement_delay: base_movement_delay,
            attack_delay: base_attack_delay,
        }
    }

    /// Return the effective delay for the given action kind.
    pub fn delay_for(&self, kind: ActionKind) -> f32 {
        match kind {
            ActionKind::Movement => self.movement_delay,
            ActionKind::Attack => self.attack_delay,
        }
    }
}

impl Default for SpeedStats {
    fn default() -> Self {
        Self::new(1.0, 1.0)
    }
}

/// Compute the effective action cost after applying a speed delay multiplier.
/// `base_cost` is the raw action cost (e.g. `BASE_ACTION_COST`), `delay` is
/// from `SpeedStats::delay_for()`.  Returns `round(base_cost * delay)`.
#[allow(dead_code)]
pub fn compute_action_cost(base_cost: u32, delay: f32) -> u32 {
    (base_cost as f32 * delay).round() as u32
}

/// Emitted by any action system when an action successfully resolves (or fails)
/// to signal the turn manager to move to the next entity.
#[derive(Message)]
pub struct ActionFinishedEvent {
    pub entity: Entity,
    pub base_cost: u32,
    pub action_kind: ActionKind,
}

/// Emitted when a player action is invalid (e.g. moving into a wall, firing with no bow).
/// Re-queues the entity at the same game time so no turn is consumed, then returns
/// immediately to `PlayerInput` state.  Must NOT be emitted for monsters (infinite loop).
#[derive(Message)]
pub struct FreeActionEvent {
    pub entity: Entity,
}

/// Holds the target position for a pending chasm fall confirmation.
#[derive(Resource, Default)]
pub struct PendingChasmFall(pub Option<crate::components::Position>);

/// Holds the player's queued action for the current turn.
/// Written by `handle_player_input` and consumed by `dispatch_player_action`.
/// Lives separately from `TurnManager` so the turn scheduler has no knowledge of action types.
#[derive(Resource, Default)]
pub struct PendingPlayerAction(pub Option<Action>);

/// Marker inserted on an entity when it begins processing an action.
/// Removed by `finish_turn()` / `free_turn()`. If still present at the end of
/// the Cleanup phase, the safety-net system emits a fallback `ActionFinishedEvent`
/// and logs a warning.
#[derive(Component)]
pub struct ActionGuard;

/// Emit `ActionFinishedEvent` and clear the `ActionGuard`. Every action handler
/// should call this (or `free_turn`) instead of writing `ActionFinishedEvent` directly.
pub fn finish_turn(
    commands: &mut Commands,
    finish_writer: &mut MessageWriter<ActionFinishedEvent>,
    entity: Entity,
    base_cost: u32,
    action_kind: ActionKind,
) {
    finish_writer.write(ActionFinishedEvent { entity, base_cost, action_kind });
    commands.entity(entity).remove::<ActionGuard>();
}

/// Emit `FreeActionEvent` (player only — no turn consumed) and clear the `ActionGuard`.
pub fn free_turn(
    commands: &mut Commands,
    free_writer: &mut MessageWriter<FreeActionEvent>,
    entity: Entity,
) {
    free_writer.write(FreeActionEvent { entity });
    commands.entity(entity).remove::<ActionGuard>();
}

// --- Systems ---

/// Converts the pending player action into the appropriate intent message.
/// Runs at the start of the Processing chain before any execution systems.
pub fn dispatch_player_action(
    mut commands: Commands,
    mut pending: ResMut<PendingPlayerAction>,
    mut move_events: MessageWriter<MovementIntent>,
    mut melee_events: MessageWriter<MeleeIntent>,
    mut wait_events: MessageWriter<WaitIntent>,
    mut pickup_events: MessageWriter<PickUpIntent>,
    mut equip_events: MessageWriter<EquipItemMessage>,
    mut unequip_events: MessageWriter<UnequipItemMessage>,
    mut drop_events: MessageWriter<DropItemMessage>,
    mut use_item_events: MessageWriter<UseItemMessage>,
    mut ranged_events: MessageWriter<RangedAttackIntent>,
    mut zap_staff_events: MessageWriter<crate::game::staves::ZapStaffMessage>,
    query: Query<Entity, (With<Player>, With<MyTurn>)>,
) {
    let Ok(player_entity) = query.single() else {
        return;
    };

    commands.entity(player_entity).insert(ActionGuard);

    if let Some(action) = pending.0.take() {
        match action {
            Action::Wait => { wait_events.write(WaitIntent { entity: player_entity }); }
            Action::Move { dir } => { move_events.write(MovementIntent { entity: player_entity, dir }); }
            Action::MeleeAttack { target } => { melee_events.write(MeleeIntent { attacker: player_entity, target }); }
            Action::PickUp => { pickup_events.write(PickUpIntent { entity: player_entity }); }
            Action::EquipItem { item } => { equip_events.write(EquipItemMessage { item_entity: item }); }
            Action::UnequipItem { item } => { unequip_events.write(UnequipItemMessage { item_entity: item }); }
            Action::DropItem { item } => { drop_events.write(DropItemMessage { item_entity: item }); }
            Action::UseItem { item } => { use_item_events.write(UseItemMessage { item_entity: item }); }
            Action::RangedAttack { target } => {
                ranged_events.write(RangedAttackIntent { attacker: player_entity, target });
            }
            Action::ZapStaff { staff_entity, target, target_pos } => {
                zap_staff_events.write(crate::game::staves::ZapStaffMessage {
                    zapper: player_entity,
                    staff_entity,
                    target,
                    target_pos,
                });
            }
        }
    } else {
        // No action pending — implicit wait to prevent turn stall.
        wait_events.write(WaitIntent { entity: player_entity });
    }
    commands.entity(player_entity).remove::<MyTurn>();
}

/// Merges a stackable floor item into existing inventory stacks, spilling remainder
/// into a new slot if space allows. Returns the total number of items transferred.
/// Handles despawn/update of the floor entity.
fn try_stack_pickup(
    commands: &mut Commands,
    floor_entity: Entity,
    item_name: &str,
    floor_stack: Option<&ItemStack>,
    inv: &mut Inventory,
    inv_stacks_query: &Query<(&Name, &ItemStack), With<InInventory>>,
) -> u32 {
    let floor_count = floor_stack.map(|s| s.count).unwrap_or(1);
    let max_stack = floor_stack.map(|s| s.max_stack).unwrap_or(1);
    let mut remaining = floor_count;

    // Fill every existing inventory stack that has room.
    let merge_targets: Vec<(Entity, u32, u32)> = inv.items.iter()
        .filter_map(|&e| {
            inv_stacks_query.get(e).ok().and_then(|(inv_name, inv_stack)| {
                if inv_name.0 == item_name && inv_stack.count < inv_stack.max_stack {
                    Some((e, inv_stack.count, inv_stack.max_stack))
                } else {
                    None
                }
            })
        })
        .collect();

    let mut total_transferred = 0u32;
    for (target_entity, current_count, target_max) in merge_targets {
        if remaining == 0 { break; }
        let space = target_max - current_count;
        let transfer = remaining.min(space);
        remaining -= transfer;
        total_transferred += transfer;
        commands.entity(target_entity).insert(ItemStack {
            count: current_count + transfer,
            max_stack: target_max,
        });
    }

    // Spill remainder into a new inventory slot if space allows.
    let mut moved_to_inv = false;
    if remaining > 0 && inv.items.len() < inv.capacity {
        commands.entity(floor_entity).insert(ItemStack { count: remaining, max_stack });
        inv.items.push(floor_entity);
        commands.entity(floor_entity)
            .insert(InInventory)
            .insert(Visibility::Hidden)
            .remove::<crate::components::FloorEntityMarker>()
            .remove::<Position>();
        total_transferred += remaining;
        remaining = 0;
        moved_to_inv = true;
    }

    // Clean up: despawn or update the floor entity.
    if !moved_to_inv {
        if remaining == 0 {
            commands.entity(floor_entity).despawn();
        } else if total_transferred > 0 {
            commands.entity(floor_entity).insert(ItemStack { count: remaining, max_stack });
        }
    }

    total_transferred
}

pub fn handle_pickup(
    mut commands: Commands,
    mut intents: MessageReader<PickUpIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    actors_query: Query<(Entity, &Position, Has<Player>)>,
    items_query: Query<(Entity, &Position, &Name, Option<&ItemStack>, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>), (With<Item>, Without<InInventory>)>,
    mut inv_query: Query<&mut Inventory, With<Player>>,
    mut monster_inv_query: Query<&mut Inventory, Without<Player>>,
    inv_stacks_query: Query<(&Name, &ItemStack), With<InInventory>>,
) {
    for intent in intents.read() {
        let Ok((actor_entity, actor_pos, is_player)) = actors_query.get(intent.entity) else {
            continue;
        };

        let mut picked_up = false;
        for (item_entity, item_pos, item_name, item_stack, item_enchant, item_weapon_runic, item_armor_runic, item_runic_id) in items_query.iter() {
            if actor_pos != item_pos {
                continue;
            }

            if is_player {
                if let Ok(mut inv) = inv_query.single_mut() {
                    let is_stackable = item_stack.map(|s| s.max_stack > 1).unwrap_or(false);

                    if is_stackable {
                        let transferred = try_stack_pickup(
                            &mut commands,
                            item_entity,
                            &item_name.0,
                            item_stack,
                            &mut inv,
                            &inv_stacks_query,
                        );
                        if transferred > 0 {
                            let dname = display_item_name(&item_name.0, item_enchant, item_weapon_runic, item_armor_runic, item_runic_id);
                            log_writer.write(GameLogMessage(format!(
                                "You pick up the {} (x{}).", dname, transferred
                            )));
                            picked_up = true;
                        } else {
                            log_writer.write(GameLogMessage("Your inventory is full!".to_string()));
                            break;
                        }
                    } else {
                        // Non-stackable: add directly as a new inventory slot.
                        if inv.items.len() < inv.capacity {
                            inv.items.push(item_entity);
                            commands
                                .entity(item_entity)
                                .insert(InInventory)
                                .insert(Visibility::Hidden)
                                .remove::<crate::components::FloorEntityMarker>()
                                .remove::<Position>();
                            let dname = display_item_name(&item_name.0, item_enchant, item_weapon_runic, item_armor_runic, item_runic_id);
                            log_writer.write(GameLogMessage(format!("You pick up the {}.", dname)));
                            picked_up = true;
                        } else {
                            log_writer.write(GameLogMessage("Your inventory is full!".to_string()));
                            break;
                        }
                    }

                    if picked_up {
                        break;
                    }
                }
            } else if let Ok(mut inv) = monster_inv_query.get_mut(actor_entity) {
                // Monster with inventory: add item to their inventory.
                inv.items.push(item_entity);
                commands
                    .entity(item_entity)
                    .insert(InInventory)
                    .insert(Visibility::Hidden)
                    .remove::<crate::components::FloorEntityMarker>()
                    .remove::<Position>();
                break;
            } else {
                // Entity without inventory: just despawn the item.
                commands.entity(item_entity).despawn();
                break;
            }
        }

        // Always finish — even if nothing was picked up, the turn is consumed.
        finish_turn(&mut commands, &mut finish_writer, actor_entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}

/// Handles movement. If a collision with a hostile entity is detected,
/// it converts the movement into a MeleeIntent instead.
/// If the target tile is a closed door, it converts it into an OpenDoorIntent.
/// Result of resolving what occupies a target tile.
enum BumpResult {
    /// Tile is free — move into it.
    Empty,
    /// Target is outside map bounds.
    OutOfBounds,
    /// Tile is a wall or unwalkable terrain.
    Wall,
    /// Tile has a closed door.
    Door(Point),
    /// Tile has a locked door.
    LockedDoor(Point),
    /// Tile has a hostile entity — convert to melee.
    HostileEntity(Entity),
    /// Tile has a chest — open it.
    Chest(Entity),
    /// Tile has a machine entity — activate it.
    Machine(Entity),
    /// Tile has a non-hostile entity with a Collider.
    BlockedByCollider,
    /// Tile has a chasm — prompt the player for confirmation before falling.
    Chasm,
}

/// Determines what happens when an actor tries to move into `target_pt`.
fn resolve_bump(
    actor: Entity,
    actor_faction: Option<&Faction>,
    actor_is_player: bool,
    actor_movement_mode: MovementMode,
    target_pt: Point,
    map: &Map,
    faction_matrix: &FactionMatrix,
    actors_query: &Query<(
        Entity,
        &mut Position,
        Has<Player>,
        Option<&Faction>,
        Has<Collider>,
        Has<Chest>,
        Has<Machine>,
        Option<&MovementMode>,
        Has<Destructible>,
    ), (Without<TileMarker>, Without<Item>)>,
) -> BumpResult {
    // 1. Bounds check
    if !map.in_bounds(target_pt) {
        return BumpResult::OutOfBounds;
    }

    let target_tile = map.tiles[map.xy_idx(target_pt.x, target_pt.y)];

    // 2. Closed door
    if target_tile.terrain == TerrainType::Door {
        return BumpResult::Door(target_pt);
    }

    // 2b. Locked door
    if target_tile.terrain == TerrainType::LockedDoor {
        return BumpResult::LockedDoor(target_pt);
    }

    // 3. Occupant scan — prioritize hostile entities over props.
    let mut bump_target = None;
    for (e, other_pos, _other_is_player, other_faction, other_has_collider, other_is_chest, other_is_machine, _, other_is_destructible) in
        actors_query.iter()
    {
        if other_pos.to_point() == target_pt && e != actor {
            // Faction-bearing entities (player/monsters) take priority over props.
            if other_faction.is_some() {
                bump_target = Some((e, other_faction, other_has_collider, other_is_chest, other_is_machine, other_is_destructible));
                break;
            }
            if bump_target.is_none() {
                bump_target = Some((e, other_faction, other_has_collider, other_is_chest, other_is_machine, other_is_destructible));
            }
        }
    }

    if let Some((target_entity, target_faction, target_has_collider, target_is_chest, target_is_machine, target_is_destructible)) =
        bump_target
    {
        let is_hostile = match (actor_faction, target_faction) {
            (Some(a), Some(b)) => faction_matrix.is_hostile_to(&a.0.0, &b.0.0),
            _ => false,
        };

        if is_hostile {
            return BumpResult::HostileEntity(target_entity);
        } else if target_is_destructible && actor_is_player {
            // Destructible props (barricades) can be attacked by the player.
            return BumpResult::HostileEntity(target_entity);
        } else if target_is_machine && !target_is_chest && target_has_collider && actor_is_player {
            // Bump-activated machine (blocking prop like altar/lever, not invisible step triggers)
            return BumpResult::Machine(target_entity);
        } else if target_is_chest && actor_is_player {
            // Only the player opens chests via bump. GOAP entities (kobolds) emit
            // OpenChestIntent directly from their AI dispatch.
            return BumpResult::Chest(target_entity);
        } else if target_has_collider {
            return BumpResult::BlockedByCollider;
        }
        // Non-hostile, non-blocking occupant — fall through to walkability check.
    }

    // 4. Chasm check — player gets a confirmation dialog; others treat as wall.
    if target_tile.liquid == crate::map::tile::LiquidType::Chasm {
        if actor_is_player {
            return BumpResult::Chasm;
        }
        return BumpResult::Wall;
    }

    // 5. Wall/obstacle check (mode-aware)
    if !can_entity_enter_tile(target_tile, actor_movement_mode) {
        return BumpResult::Wall;
    }

    BumpResult::Empty
}

pub fn handle_movement(
    mut commands: Commands,
    mut intents: MessageReader<MovementIntent>,
    mut melee_writer: MessageWriter<MeleeIntent>,
    mut open_door_writer: MessageWriter<OpenDoorIntent>,
    mut unlock_door_writer: MessageWriter<UnlockDoorIntent>,
    mut open_chest_writer: MessageWriter<OpenChestIntent>,
    mut machine_bump_writer: MessageWriter<MachineBumpMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut free_writer: MessageWriter<FreeActionEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut tile_writers: crate::map::tile::TileMutationWriters,
    mut actors_query: Query<(
        Entity,
        &mut Position,
        Has<Player>,
        Option<&Faction>,
        Has<Collider>,
        Has<Chest>,
        Has<Machine>,
        Option<&MovementMode>,
        Has<Destructible>,
    ), (Without<TileMarker>, Without<Item>)>,
    mut status_query: Query<&mut crate::game::magic::StatusEffects>,
    map: Res<Map>,
    faction_matrix: Res<FactionMatrix>,
    mut next_ingame_state: ResMut<NextState<crate::game::InGameState>>,
) {
    for intent in intents.read() {
        let Ok((_, pos, is_player, actor_faction, _, _, _, movement_mode, _)) = actors_query.get(intent.entity) else {
            finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
            continue;
        };
        let actor_faction = actor_faction.cloned();
        let actor_movement_mode = movement_mode.copied().unwrap_or_default();

        let target_pt = pos.to_point() + intent.dir.offset();
        let result = resolve_bump(intent.entity, actor_faction.as_ref(), is_player, actor_movement_mode, target_pt, &map, &faction_matrix, &actors_query);

        match result {
            BumpResult::Empty => {
                if let Ok((_, mut pos, _, _, _, _, _, _, _)) = actors_query.get_mut(intent.entity) {
                    pos.x = target_pt.x;
                    pos.y = target_pt.y;
                }
                // Decoration movement cost modifier
                let idx = map.xy_idx(target_pt.x, target_pt.y);
                let dec_multiplier = if idx < map.tiles.len() {
                    map.tiles[idx].decoration.movement_cost()
                } else {
                    1.0
                };
                let mut move_cost = (BASE_ACTION_COST as f32 * dec_multiplier) as u32;
                if dec_multiplier > 1.0 && is_player {
                    let dec_name = map.tiles[idx].decoration.name();
                    log_writer.write(GameLogMessage(format!(
                        "The {} slows your movement!", dec_name.to_lowercase()
                    )));
                }
                // Deep water movement cost (Land entities only)
                if actor_movement_mode == MovementMode::Land
                    && idx < map.tiles.len()
                    && map.tiles[idx].liquid == crate::map::tile::LiquidType::Water
                {
                    move_cost *= 2;
                    if is_player {
                        log_writer.write(GameLogMessage("The deep water slows your movement.".to_string()));
                    }
                }

                // On-step decoration promotion (trample tall grass, fungus)
                if idx < map.tiles.len() {
                    use crate::map::tile::{PromotionTarget, DecorationMutationMessage};
                    let decoration = map.tiles[idx].decoration;

                    if let Some(target) = decoration.on_step_promotion() {
                        match target {
                            PromotionTarget::Decoration(new_dec) => {
                                tile_writers.decoration.write(DecorationMutationMessage {
                                    position: bracket_lib::prelude::Point::new(target_pt.x, target_pt.y),
                                    new_decoration: new_dec,
                                });
                            }
                            PromotionTarget::Terrain(new_terrain) => {
                                tile_writers.terrain.write(crate::map::tile::TileMutationMessage {
                                    position: bracket_lib::prelude::Point::new(target_pt.x, target_pt.y),
                                    new_terrain,
                                });
                            }
                        }
                    }

                    // Cobweb entangle: immobilize for 3 turns, web persists until break-free
                    if decoration.entangles() {
                        if let Ok(mut effects) = status_query.get_mut(intent.entity) {
                            if !effects.is_entangled() {
                                effects.add(crate::game::magic::StatusEffectKind::Entangled, 3);
                                let msg = if is_player {
                                    "You are caught in the cobwebs!".to_string()
                                } else {
                                    "Something is caught in the cobwebs!".to_string()
                                };
                                log_writer.write(GameLogMessage(msg));
                            }
                        }
                    }
                }

                // Gas poisoning on step
                if let Some(gas_data) = tile_writers.gas_tiles.0.get(&(target_pt.x, target_pt.y)) {
                    if let Some((effect_kind, duration)) = gas_data.gas_type.on_step_effect(gas_data.concentration) {
                        if let Ok(mut effects) = status_query.get_mut(intent.entity) {
                            if !gas_data.gas_type.is_immune(&effects) {
                                effects.add(effect_kind, duration);
                                if is_player {
                                    log_writer.write(GameLogMessage(format!(
                                        "You inhale {}!",
                                        gas_data.gas_type.name(gas_data.concentration)
                                    )));
                                }
                            }
                        }
                    }
                }

                finish_turn(&mut commands, &mut finish_writer, intent.entity, move_cost, ActionKind::Movement);
            }
            BumpResult::Door(door_pos) => {
                open_door_writer.write(OpenDoorIntent {
                    entity: intent.entity,
                    door_pos,
                });
                // ActionGuard cleared by handle_door_open via finish_turn.
            }
            BumpResult::LockedDoor(door_pos) => {
                unlock_door_writer.write(UnlockDoorIntent {
                    entity: intent.entity,
                    door_pos,
                });
                // ActionGuard cleared by handle_unlock_door via finish_turn/free_turn.
            }
            BumpResult::HostileEntity(target) => {
                melee_writer.write(MeleeIntent {
                    attacker: intent.entity,
                    target,
                });
                // ActionGuard cleared by handle_melee via finish_turn.
            }
            BumpResult::Chest(chest_entity) => {
                open_chest_writer.write(OpenChestIntent {
                    entity: intent.entity,
                    chest_entity,
                });
                // ActionGuard cleared by handle_open_chest via finish_turn.
            }
            BumpResult::Machine(machine_entity) => {
                machine_bump_writer.write(MachineBumpMessage {
                    activator: intent.entity,
                    machine_entity,
                });
                // ActionGuard cleared by handle_machine_bump via finish_turn.
            }
            BumpResult::Chasm => {
                // Store the target position and open the confirmation dialog.
                commands.insert_resource(PendingChasmFall(Some(crate::components::Position {
                    x: target_pt.x,
                    y: target_pt.y,
                })));
                next_ingame_state.set(crate::game::InGameState::ChasmConfirm);
                free_turn(&mut commands, &mut free_writer, intent.entity);
            }
            BumpResult::OutOfBounds | BumpResult::Wall | BumpResult::BlockedByCollider => {
                if is_player {
                    free_turn(&mut commands, &mut free_writer, intent.entity);
                } else {
                    finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
                }
            }
        }
    }
}

pub fn handle_melee(
    mut commands: Commands,
    mut intents: MessageReader<MeleeIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut attack_writer: MessageWriter<AttackIntentMessage>,
    damage_type_query: Query<Option<&DamageTypeTag>>,
    equipment_query: Query<&crate::game::items::Equipment>,
    weapon_props_query: Query<&crate::game::items::ItemProperties>,
) {
    for intent in intents.read() {
        let damage_type = damage_type_query
            .get(intent.attacker)
            .ok()
            .flatten()
            .map(|t| t.0)
            .unwrap_or(DamageType::Physical);
        attack_writer.write(AttackIntentMessage {
            attacker: intent.attacker,
            target: intent.target,
            damage_type,
            source: DamageSource::Melee,
        });

        // Use weapon attack speed to scale action cost
        let attack_cost = equipment_query
            .get(intent.attacker)
            .ok()
            .and_then(|eq| eq.weapon)
            .and_then(|w| weapon_props_query.get(w).ok())
            .map(|props| (BASE_ACTION_COST as f32 * props.attack_speed).round() as u32)
            .unwrap_or(BASE_ACTION_COST);

        finish_turn(&mut commands, &mut finish_writer, intent.attacker, attack_cost, ActionKind::Attack);
    }
}

pub fn handle_wait(
    mut commands: Commands,
    mut intents: MessageReader<WaitIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    for intent in intents.read() {
        finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}

pub fn handle_door_open(
    mut commands: Commands,
    mut intents: MessageReader<OpenDoorIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut tile_mutation_writer: MessageWriter<TileMutationMessage>,
) {
    for intent in intents.read() {
        tile_mutation_writer.write(TileMutationMessage {
            position: intent.door_pos,
            new_terrain: TerrainType::OpenDoor,
        });
        finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}

/// Handles bumping a LockedDoor tile. Searches the player's inventory for a matching
/// Key component. If found, consumes the key, converts the door to OpenDoor, and costs
/// one turn. If not found, logs a message and does not consume a turn.
pub fn handle_unlock_door(
    mut commands: Commands,
    mut intents: MessageReader<UnlockDoorIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut free_writer: MessageWriter<FreeActionEvent>,
    mut tile_mutation_writer: MessageWriter<TileMutationMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut inventory_query: Query<&mut Inventory>,
    key_query: Query<&Key>,
    locked_door_query: Query<(Entity, &Position, &LockedDoorData)>,
    player_query: Query<Entity, With<Player>>,
) {
    for intent in intents.read() {
        // Only the player can unlock doors via bump
        let Ok(player_entity) = player_query.single() else {
            finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
            continue;
        };
        if intent.entity != player_entity {
            // Non-player entities treat locked doors as walls
            finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
            continue;
        }

        // Find the LockedDoorData entity at this position to get the key name
        let door_key_name = locked_door_query.iter().find_map(|(_, pos, data)| {
            if pos.x == intent.door_pos.x && pos.y == intent.door_pos.y {
                Some(data.key_name.clone())
            } else {
                None
            }
        });

        let Some(required_key) = door_key_name else {
            // No LockedDoorData entity found — treat as a generic locked door
            log_writer.write(GameLogMessage("This door is locked. You need a key.".to_string()));
            free_turn(&mut commands, &mut free_writer, intent.entity);
            continue;
        };

        // Search player inventory for a matching key
        let Ok(inventory) = inventory_query.get(player_entity) else {
            log_writer.write(GameLogMessage("This door is locked. You need a key.".to_string()));
            free_turn(&mut commands, &mut free_writer, intent.entity);
            continue;
        };

        let matching_key = inventory.items.iter().find(|&&item_entity| {
            key_query.get(item_entity).is_ok_and(|k| k.key_name == required_key)
        }).copied();

        if let Some(key_entity) = matching_key {
            let key_display = required_key.clone();

            // Remove key from inventory and despawn it
            if let Ok(mut inv) = inventory_query.get_mut(player_entity) {
                inv.items.retain(|&e| e != key_entity);
            }
            commands.entity(key_entity).despawn();

            // Convert door to OpenDoor
            tile_mutation_writer.write(TileMutationMessage {
                position: intent.door_pos,
                new_terrain: TerrainType::OpenDoor,
            });

            // Despawn the LockedDoorData marker entity
            for (door_entity, pos, _) in locked_door_query.iter() {
                if pos.x == intent.door_pos.x && pos.y == intent.door_pos.y {
                    commands.entity(door_entity).despawn();
                    break;
                }
            }

            log_writer.write(GameLogMessage(format!("You unlock the door with the {}.", key_display)));
            finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
        } else {
            log_writer.write(GameLogMessage("This door is locked. You need a key.".to_string()));
            free_turn(&mut commands, &mut free_writer, intent.entity);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum Direction {
    NW,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NoDirection,
}

impl Direction {
    pub const ALL: [Self; 8] = [
        Self::N,
        Self::NE,
        Self::E,
        Self::SE,
        Self::S,
        Self::SW,
        Self::W,
        Self::NW,
    ];

    pub fn from_pos(current: &Position, target: &Position) -> Self {
        match target.x.cmp(&current.x) {
            std::cmp::Ordering::Less => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::SW,
                std::cmp::Ordering::Equal => Direction::W,
                std::cmp::Ordering::Greater => Direction::NW,
            },
            std::cmp::Ordering::Equal => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::S,
                std::cmp::Ordering::Equal => Direction::NoDirection,
                std::cmp::Ordering::Greater => Direction::N,
            },
            std::cmp::Ordering::Greater => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::SE,
                std::cmp::Ordering::Equal => Direction::E,
                std::cmp::Ordering::Greater => Direction::NE,
            },
        }
    }

    pub fn offset(&self) -> Point {
        match self {
            Direction::NW => Point { x: -1, y: 1 },
            Direction::N => Point { x: 0, y: 1 },
            Direction::NE => Point { x: 1, y: 1 },
            Direction::E => Point { x: 1, y: 0 },
            Direction::SE => Point { x: 1, y: -1 },
            Direction::S => Point { x: 0, y: -1 },
            Direction::SW => Point { x: -1, y: -1 },
            Direction::W => Point { x: -1, y: 0 },
            Direction::NoDirection => Point { x: 0, y: 0 },
        }
    }

    /// Returns the two cardinal directions perpendicular to this one (left, right).
    pub fn perpendiculars(&self) -> (Direction, Direction) {
        match self {
            Direction::N | Direction::S => (Direction::W, Direction::E),
            Direction::E | Direction::W => (Direction::N, Direction::S),
            _ => (Direction::NoDirection, Direction::NoDirection),
        }
    }

    pub fn opposite(&self) -> Self {
        match self {
            Direction::NW => Direction::SE,
            Direction::N => Direction::S,
            Direction::NE => Direction::SW,
            Direction::E => Direction::W,
            Direction::SE => Direction::NW,
            Direction::S => Direction::N,
            Direction::SW => Direction::NE,
            Direction::W => Direction::E,
            Direction::NoDirection => Direction::NoDirection,
        }
    }
}

/// Returns rarity weights `[Common, Uncommon, Rare, Legendary]` scaled by floor depth.
pub fn rarity_weights_for_floor(floor: i32) -> [u32; 4] {
    match floor {
        1..=3   => [70, 24,  5, 1],
        4..=6   => [55, 32, 11, 2],
        7..=9   => [40, 38, 18, 4],
        _       => [25, 40, 27, 8],
    }
}

/// When a player bumps a chest, despawn it and spawn 1-3 random items from
/// the floor's item spawn table at the chest's position, using floor-scaled
/// rarity weights.
pub fn handle_open_chest(
    mut commands: Commands,
    mut intents: MessageReader<OpenChestIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    chest_query: Query<&Position, With<Chest>>,
    floor: Res<Floor>,
    item_spawn_table_handle: Res<ItemSpawnTableHandle>,
    item_spawn_tables: Res<Assets<ItemSpawnTable>>,
    item_manifests: Res<Assets<ItemManifest>>,
    item_manifest_handle: Res<ItemManifestHandle>,
    item_sprite_assets: Res<ItemSpriteAssets>,
    ascii_font: Option<Res<crate::game::ascii_mode::AsciiFont>>,
    player_check: Query<&Name, With<Player>>,
) {
    use bracket_lib::prelude::RandomNumberGenerator;
    use crate::game::items::Rarity;

    for intent in intents.read() {
        let Ok(chest_pos) = chest_query.get(intent.chest_entity) else {
            finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
            continue;
        };
        let pos = *chest_pos;

        // Despawn the chest entity.
        commands.entity(intent.chest_entity).despawn();

        // Pick random items from the spawn table.
        let Some(spawn_table) = item_spawn_tables.get(&item_spawn_table_handle.0) else {
            finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
            continue;
        };

        let Some(item_manifest) = item_manifests.get(&item_manifest_handle.0) else {
            finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
            continue;
        };

        let depth = floor.0 as i32;
        let floor_candidates: Vec<_> = spawn_table.spawns.iter()
            .filter(|s| depth >= s.min_floor && depth <= s.max_floor)
            .collect();

        let is_player = player_check.get(intent.entity).is_ok();

        if floor_candidates.is_empty() {
            if is_player {
                log_writer.write(GameLogMessage("You open the chest but it's empty!".to_string()));
            }
            finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
            continue;
        }

        let rarity_weights = rarity_weights_for_floor(depth);
        let rarity_tiers = [Rarity::Common, Rarity::Uncommon, Rarity::Rare, Rarity::Legendary];
        let rarity_total: u32 = rarity_weights.iter().sum();

        let mut rng = RandomNumberGenerator::new();
        let item_count = rng.range(1, 4); // 1-3 items

        if is_player {
            log_writer.write(GameLogMessage("You open the chest!".to_string()));
        }

        for _ in 0..item_count {
            // Roll a rarity tier using floor-scaled weights.
            let rarity_roll = rng.range(0, rarity_total as i32) as u32;
            let mut acc = 0u32;
            let mut chosen_rarity = Rarity::Common;
            for (i, &w) in rarity_weights.iter().enumerate() {
                acc += w;
                if rarity_roll < acc {
                    chosen_rarity = rarity_tiers[i].clone();
                    break;
                }
            }

            // Filter candidates to the chosen rarity, falling back to lower tiers.
            let mut candidates: Vec<_> = floor_candidates.iter()
                .filter(|s| {
                    item_manifest.items.get(&s.item)
                        .map(|a| a.rarity == chosen_rarity)
                        .unwrap_or(false)
                })
                .collect();

            // Fallback: try each lower rarity tier until we find items.
            if candidates.is_empty() {
                let tier_idx = rarity_tiers.iter().position(|r| *r == chosen_rarity).unwrap_or(0);
                for fallback in (0..tier_idx).rev() {
                    candidates = floor_candidates.iter()
                        .filter(|s| {
                            item_manifest.items.get(&s.item)
                                .map(|a| a.rarity == rarity_tiers[fallback])
                                .unwrap_or(false)
                        })
                        .collect();
                    if !candidates.is_empty() {
                        break;
                    }
                }
            }

            // If still empty, use all floor candidates as final fallback.
            if candidates.is_empty() {
                candidates = floor_candidates.iter().collect();
            }

            let total_weight: i32 = candidates.iter().map(|s| s.weight).sum();
            if total_weight <= 0 {
                continue;
            }

            let roll = rng.range(0, total_weight);
            let mut item_acc = 0;
            let chosen = candidates.iter().find(|s| {
                item_acc += s.weight;
                roll < item_acc
            });

            if let Some(spawn_info) = chosen {
                let pt = Point::new(pos.x, pos.y);
                if let Some(_entity) = spawn_item(
                    &mut commands,
                    &spawn_info.item,
                    &pt,
                    &item_manifests,
                    &item_manifest_handle,
                    &item_sprite_assets,
                    ascii_font.as_deref(),
                    Some(floor.0),
                ) {
                    if is_player {
                        log_writer.write(GameLogMessage(format!("  Found: {}", spawn_info.item)));
                    }
                }
            }
        }

        finish_turn(&mut commands, &mut finish_writer, intent.entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_1_weights() {
        let w = rarity_weights_for_floor(1);
        assert_eq!(w, [70, 24, 5, 1]);
    }

    #[test]
    fn floor_3_weights() {
        let w = rarity_weights_for_floor(3);
        assert_eq!(w, [70, 24, 5, 1]);
    }

    #[test]
    fn floor_6_weights() {
        let w = rarity_weights_for_floor(6);
        assert_eq!(w, [55, 32, 11, 2]);
    }

    #[test]
    fn floor_9_weights() {
        let w = rarity_weights_for_floor(9);
        assert_eq!(w, [40, 38, 18, 4]);
    }

    #[test]
    fn floor_10_weights() {
        let w = rarity_weights_for_floor(10);
        assert_eq!(w, [25, 40, 27, 8]);
    }

    #[test]
    fn floor_beyond_10_uses_deepest_tier() {
        let w = rarity_weights_for_floor(15);
        assert_eq!(w, [25, 40, 27, 8]);
    }

    // --- SpeedStats tests ---

    #[test]
    fn delay_for_movement_returns_movement_delay() {
        let stats = SpeedStats::new(0.8, 1.2);
        assert_eq!(stats.delay_for(ActionKind::Movement), 0.8);
    }

    #[test]
    fn delay_for_attack_returns_attack_delay() {
        let stats = SpeedStats::new(0.8, 1.2);
        assert_eq!(stats.delay_for(ActionKind::Attack), 1.2);
    }

    #[test]
    fn default_speed_stats_has_unit_delays() {
        let stats = SpeedStats::default();
        assert_eq!(stats.movement_delay, 1.0);
        assert_eq!(stats.attack_delay, 1.0);
        assert_eq!(stats.base_movement_delay, 1.0);
        assert_eq!(stats.base_attack_delay, 1.0);
    }

    #[test]
    fn new_speed_stats_sets_effective_equal_to_base() {
        let stats = SpeedStats::new(0.5, 2.0);
        assert_eq!(stats.movement_delay, stats.base_movement_delay);
        assert_eq!(stats.attack_delay, stats.base_attack_delay);
    }

    // --- Action cost calculation tests ---

    #[test]
    fn default_delay_preserves_base_cost() {
        let stats = SpeedStats::default();
        let cost = compute_action_cost(BASE_ACTION_COST, stats.delay_for(ActionKind::Movement));
        assert_eq!(cost, BASE_ACTION_COST);
    }

    #[test]
    fn fast_entity_halves_cost() {
        let stats = SpeedStats::new(0.5, 0.5);
        let cost = compute_action_cost(BASE_ACTION_COST, stats.delay_for(ActionKind::Movement));
        assert_eq!(cost, BASE_ACTION_COST / 2);
    }

    #[test]
    fn slow_entity_doubles_cost() {
        let stats = SpeedStats::new(2.0, 2.0);
        let cost = compute_action_cost(BASE_ACTION_COST, stats.delay_for(ActionKind::Attack));
        assert_eq!(cost, BASE_ACTION_COST * 2);
    }

    #[test]
    fn action_cost_rounds_correctly() {
        // 100 * 1.5 = 150.0 -> 150 (exact)
        assert_eq!(compute_action_cost(100, 1.5), 150);
        // 100 * 0.75 = 75.0 -> 75 (exact)
        assert_eq!(compute_action_cost(100, 0.75), 75);
        // 100 * 0.33 = 33.0 -> 33 (exact)
        assert_eq!(compute_action_cost(100, 0.33), 33);
        // 7 * 0.3 = 2.1 -> rounds to 2
        assert_eq!(compute_action_cost(7, 0.3), 2);
        // 7 * 0.5 = 3.5 -> rounds to 4 (round half up)
        assert_eq!(compute_action_cost(7, 0.5), 4);
    }

    #[test]
    fn zero_base_cost_always_zero() {
        assert_eq!(compute_action_cost(0, 1.0), 0);
        assert_eq!(compute_action_cost(0, 2.5), 0);
    }

    #[test]
    fn action_kind_selects_correct_delay_field() {
        let stats = SpeedStats::new(0.6, 1.4);
        let move_cost = compute_action_cost(BASE_ACTION_COST, stats.delay_for(ActionKind::Movement));
        let attack_cost = compute_action_cost(BASE_ACTION_COST, stats.delay_for(ActionKind::Attack));
        assert_eq!(move_cost, 60);
        assert_eq!(attack_cost, 140);
    }

    // --- Direction tests ---

    #[test]
    fn direction_offset_all_eight_directions() {
        assert_eq!(Direction::N.offset(), Point { x: 0, y: 1 });
        assert_eq!(Direction::NE.offset(), Point { x: 1, y: 1 });
        assert_eq!(Direction::E.offset(), Point { x: 1, y: 0 });
        assert_eq!(Direction::SE.offset(), Point { x: 1, y: -1 });
        assert_eq!(Direction::S.offset(), Point { x: 0, y: -1 });
        assert_eq!(Direction::SW.offset(), Point { x: -1, y: -1 });
        assert_eq!(Direction::W.offset(), Point { x: -1, y: 0 });
        assert_eq!(Direction::NW.offset(), Point { x: -1, y: 1 });
    }

    #[test]
    fn direction_offset_no_direction_is_zero() {
        assert_eq!(Direction::NoDirection.offset(), Point { x: 0, y: 0 });
    }

    #[test]
    fn direction_from_pos_roundtrip() {
        let origin = Position { x: 5, y: 5 };
        for dir in Direction::ALL {
            let off = dir.offset();
            let target = Position { x: origin.x + off.x, y: origin.y + off.y };
            let recovered = Direction::from_pos(&origin, &target);
            assert_eq!(recovered, dir, "roundtrip failed for {:?}", dir);
        }
    }

    #[test]
    fn direction_from_pos_same_position_gives_no_direction() {
        let pos = Position { x: 3, y: 7 };
        assert_eq!(Direction::from_pos(&pos, &pos), Direction::NoDirection);
    }

    #[test]
    fn direction_opposite_is_involutory() {
        for dir in Direction::ALL {
            assert_eq!(dir.opposite().opposite(), dir, "double opposite failed for {:?}", dir);
        }
        assert_eq!(Direction::NoDirection.opposite(), Direction::NoDirection);
    }

    #[test]
    fn direction_opposite_offsets_cancel() {
        for dir in Direction::ALL {
            let fwd = dir.offset();
            let back = dir.opposite().offset();
            assert_eq!(fwd.x + back.x, 0, "x offset didn't cancel for {:?}", dir);
            assert_eq!(fwd.y + back.y, 0, "y offset didn't cancel for {:?}", dir);
        }
    }
}
