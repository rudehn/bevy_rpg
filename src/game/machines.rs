//! Machine system — interactive dungeon elements (altars, levers, pressure plates, gates).
//!
//! Machines are entities with a `MachineTrigger` (how to activate) and `MachineEffect`
//! (what happens). They're placed by the `MachineBuilder` during map generation.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{Name, Position};
use crate::constants::BASE_ACTION_COST;
use crate::game::actions::{finish_turn, ActionFinishedEvent, ActionKind};
use crate::game::combat::Health;
use crate::game::{AppState, TurnManager};
use crate::map::map::Map;
use crate::map::tile::TileMutationMessage;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

// =====================================================================
// Components
// =====================================================================

/// Marker: this entity is a machine element.
#[derive(Component, Debug)]
pub struct Machine;

/// How this machine is activated.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub enum MachineTrigger {
    /// Activated when a player bumps into the tile (like a door).
    BumpActivate,
    /// Activated when any entity steps onto the tile.
    StepActivate,
}

/// What happens when the machine is triggered.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub enum MachineEffect {
    /// Heal the activator to full HP.
    HealFull,
    /// Spawn an item at this position.
    SpawnItem { item_name: String },
    /// Spawn monsters at adjacent walkable tiles.
    /// If monster_name is empty, picks level-appropriate monsters from the spawn table.
    SpawnMonsters { monster_name: String, count: u32 },
    /// Apply multiple effects in sequence.
    Multi(Vec<MachineEffect>),
}

/// Whether this machine has already been activated (single-use machines).
#[derive(Component, Debug)]
pub struct MachineUsed(pub bool);

/// If present, the machine entity is despawned after activation (e.g., trapped chest disappears).
#[derive(Component, Debug)]
pub struct MachineConsumeOnUse;

/// Links machines together (e.g., lever group 1 → gate group 1).
#[derive(Component, Debug, Clone)]
pub struct MachineGroup(pub u32);

// =====================================================================
// Messages
// =====================================================================

/// Sent when a player bumps into a machine entity.
#[derive(Message, Debug)]
pub struct MachineBumpMessage {
    pub activator: Entity,
    pub machine_entity: Entity,
}

// =====================================================================
// Systems
// =====================================================================

/// Handle bump-activated machines: apply effects, mark used, cost a turn.
pub fn handle_machine_bump(
    mut commands: Commands,
    mut messages: MessageReader<MachineBumpMessage>,
    mut machine_query: Query<(
        &MachineTrigger,
        &MachineEffect,
        &mut MachineUsed,
        &Position,
        &Name,
        Has<MachineConsumeOnUse>,
    ), With<Machine>>,
    mut health_query: Query<&mut Health>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    for msg in messages.read() {
        let Ok((trigger, effect, mut used, pos, name, consume_on_use)) = machine_query.get_mut(msg.machine_entity) else { continue; };

        if !matches!(trigger, MachineTrigger::BumpActivate) { continue; }
        if used.0 {
            log_writer.write(GameLogMessage(format!("The {} has already been used.", name.0)));
            finish_turn(&mut commands, &mut finish_writer, msg.activator, BASE_ACTION_COST, ActionKind::Movement);
            continue;
        }

        used.0 = true;

        log_writer.write(GameLogMessage(format!("You activate the {}.", name.0)));

        // Apply inline effects (simple ones handled here)
        apply_effect_inline(
            &effect.clone(), msg.activator, pos,
            &mut health_query, &mut log_writer,
        );

        // For effects needing heavy resources (SpawnItem, SpawnMonsters),
        // insert a pending marker processed by a deferred system.
        queue_deferred_effects(
            &effect.clone(), pos, &mut commands,
        );

        // Despawn the machine if it's marked as consume-on-use (e.g., trapped chest)
        if consume_on_use {
            commands.entity(msg.machine_entity).despawn();
        }

        finish_turn(&mut commands, &mut finish_writer, msg.activator, BASE_ACTION_COST, ActionKind::Movement);
    }
}

/// Apply effects that only need Health access (no heavy resources).
fn apply_effect_inline(
    effect: &MachineEffect,
    activator: Entity,
    _pos: &Position,
    health_query: &mut Query<&mut Health>,
    log_writer: &mut MessageWriter<GameLogMessage>,
) {
    match effect {
        MachineEffect::HealFull => {
            if let Ok(mut health) = health_query.get_mut(activator) {
                let healed = health.max - health.current;
                health.current = health.max;
                if healed > 0 {
                    log_writer.write(GameLogMessage(format!("You are healed for {} HP!", healed)));
                }
            }
        }
        MachineEffect::Multi(effects) => {
            for e in effects {
                apply_effect_inline(e, activator, _pos, health_query, log_writer);
            }
        }
        _ => {} // Handled by deferred system
    }
}

/// Queue effects that need heavy resources (spawning items/monsters).
fn queue_deferred_effects(
    effect: &MachineEffect,
    pos: &Position,
    commands: &mut Commands,
) {
    match effect {
        MachineEffect::SpawnItem { item_name } => {
            commands.insert_resource(PendingMachineSpawnItem {
                item_name: item_name.clone(),
                pos: *pos,
            });
        }
        MachineEffect::SpawnMonsters { monster_name, count } => {
            commands.insert_resource(PendingMachineSpawnMonsters {
                monster_name: monster_name.clone(),
                count: *count,
                pos: *pos,
            });
        }
        MachineEffect::Multi(effects) => {
            for e in effects {
                queue_deferred_effects(e, pos, commands);
            }
        }
        _ => {}
    }
}

// =====================================================================
// Deferred Spawn Resources
// =====================================================================

/// Pending item spawn from a machine effect.
#[derive(Resource)]
pub struct PendingMachineSpawnItem {
    pub item_name: String,
    pub pos: Position,
}

/// Pending monster spawn from a machine effect.
#[derive(Resource)]
pub struct PendingMachineSpawnMonsters {
    pub monster_name: String,
    pub count: u32,
    pub pos: Position,
}

/// Process deferred item spawns from machine effects.
/// Spawns the item at an adjacent walkable tile (not on the machine itself).
pub fn process_pending_machine_item(
    mut commands: Commands,
    pending: Option<Res<PendingMachineSpawnItem>>,
    item_manifests: Res<bevy::asset::Assets<crate::assets::ItemManifest>>,
    item_manifest_handle: Res<crate::assets::ItemManifestHandle>,
    item_sprite_assets: Res<crate::assets::ItemSpriteAssets>,
    ascii_font: Option<Res<crate::game::ascii_mode::AsciiFont>>,
    map: Res<Map>,
    collider_query: Query<&Position, With<crate::components::Collider>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    let Some(pending) = pending else { return; };
    use bracket_lib::prelude::Point;
    use crate::map::tile::is_walkable;

    // Find an adjacent walkable tile that isn't blocked by a collider
    let occupied: std::collections::HashSet<(i32, i32)> = collider_query
        .iter()
        .map(|p| (p.x, p.y))
        .collect();

    let directions = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
    let mut spawn_point = Point::new(pending.pos.x, pending.pos.y); // fallback to machine pos
    for (dx, dy) in &directions {
        let nx = pending.pos.x + dx;
        let ny = pending.pos.y + dy;
        let idx = map.xy_idx(nx, ny);
        if idx < map.tiles.len() && is_walkable(map.tiles[idx]) && !occupied.contains(&(nx, ny)) {
            spawn_point = Point::new(nx, ny);
            break;
        }
    }

    crate::game::spawner::spawn_item(
        &mut commands,
        &pending.item_name,
        &spawn_point,
        &item_manifests,
        &item_manifest_handle,
        &item_sprite_assets,
        ascii_font.as_deref(),
        None,
    );
    log_writer.write(GameLogMessage(format!("A {} appears!", pending.item_name)));
    commands.remove_resource::<PendingMachineSpawnItem>();
}

/// Process deferred monster spawns from machine effects.
/// If monster_name is empty, picks level-appropriate monsters from the spawn table.
pub fn process_pending_machine_monsters(
    mut commands: Commands,
    pending: Option<Res<PendingMachineSpawnMonsters>>,
    mut turn_manager: ResMut<TurnManager>,
    monster_manifests: Res<bevy::asset::Assets<crate::assets::MonsterManifest>>,
    monster_manifest_handle: Res<crate::assets::MonsterManifestHandle>,
    monster_sprite_assets: Res<crate::assets::MonsterSpriteAssets>,
    monster_spawn_table_handle: Res<crate::assets::MonsterSpawnTableHandle>,
    monster_spawn_tables: Res<bevy::asset::Assets<crate::assets::MonsterSpawnTable>>,
    floor: Res<crate::map::dungeon::Floor>,
    ascii_font: Option<Res<crate::game::ascii_mode::AsciiFont>>,
    map: Res<Map>,
    collider_query: Query<&Position, With<crate::components::Collider>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    let Some(pending) = pending else { return; };
    use bracket_lib::prelude::Point;
    use bracket_lib::random::RandomNumberGenerator;

    let occupied: std::collections::HashSet<(i32, i32)> = collider_query
        .iter()
        .map(|p| (p.x, p.y))
        .collect();

    // Build list of level-appropriate monsters if name is empty
    let mut rng = RandomNumberGenerator::new();
    let depth = floor.0 as i32;

    let directions = [(0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)];
    let mut spawned = 0u32;
    for (dx, dy) in &directions {
        if spawned >= pending.count { break; }
        let nx = pending.pos.x + dx;
        let ny = pending.pos.y + dy;
        let idx = map.xy_idx(nx, ny);
        if idx >= map.tiles.len() || !crate::map::tile::is_walkable(map.tiles[idx]) || occupied.contains(&(nx, ny)) {
            continue;
        }

        // Resolve monster name — pick from spawn table if empty
        let monster_name = if pending.monster_name.is_empty() {
            pick_level_monster(&monster_spawn_tables, &monster_spawn_table_handle, depth, &mut rng)
        } else {
            Some(pending.monster_name.clone())
        };

        if let Some(name) = monster_name {
            crate::game::spawner::spawn_monster_by_name(
                &mut commands,
                &name,
                &Point::new(nx, ny),
                &mut turn_manager,
                &monster_manifests,
                &monster_manifest_handle,
                &monster_sprite_assets,
                ascii_font.as_deref(),
            );
            spawned += 1;
        }
    }
    if spawned > 0 {
        log_writer.write(GameLogMessage("Monsters emerge from the shadows!".to_string()));
    }
    commands.remove_resource::<PendingMachineSpawnMonsters>();
}

/// Pick a random monster from the spawn table appropriate for the given floor depth.
fn pick_level_monster(
    spawn_tables: &Res<bevy::asset::Assets<crate::assets::MonsterSpawnTable>>,
    handle: &Res<crate::assets::MonsterSpawnTableHandle>,
    depth: i32,
    rng: &mut bracket_lib::random::RandomNumberGenerator,
) -> Option<String> {
    let table = spawn_tables.get(&handle.0)?;
    let eligible: Vec<&crate::assets::MonsterSpawnInfo> = table.spawns.iter()
        .filter(|s| depth >= s.min_floor && depth <= s.max_floor && !s.monster.is_empty())
        .collect();
    if eligible.is_empty() { return None; }
    let idx = rng.range(0, eligible.len() as i32) as usize;
    Some(eligible[idx].monster.clone())
}

/// Detect when the player steps onto a StepActivate machine tile.
/// Only triggers for Player entities during Processing to avoid false triggers on spawn.
pub fn machine_step_system(
    mut commands: Commands,
    moved_query: Query<(Entity, &Position), (Changed<Position>, With<Player>)>,
    mut machine_query: Query<(
        Entity,
        &MachineTrigger,
        &MachineEffect,
        &mut MachineUsed,
        &Position,
        &Name,
        Has<MachineConsumeOnUse>,
    ), With<Machine>>,
    mut health_query: Query<&mut Health>,
    mut log_writer: MessageWriter<GameLogMessage>,
    turn_state: Res<bevy::state::state::State<crate::game::turns::TurnState>>,
) {
    // Only trigger during Processing — not during floor setup or other states
    if *turn_state.get() != crate::game::turns::TurnState::Processing { return; }

    for (mover, mover_pos) in moved_query.iter() {
        for (machine_entity, trigger, effect, mut used, machine_pos, name, consume) in machine_query.iter_mut() {
            if !matches!(trigger, MachineTrigger::StepActivate) { continue; }
            if used.0 { continue; }
            if mover_pos.x != machine_pos.x || mover_pos.y != machine_pos.y { continue; }

            used.0 = true;

            log_writer.write(GameLogMessage("It's a trap!".to_string()));

            apply_effect_inline(
                &effect.clone(), mover, machine_pos,
                &mut health_query, &mut log_writer,
            );
            queue_deferred_effects(
                &effect.clone(), machine_pos, &mut commands,
            );

            if consume {
                commands.entity(machine_entity).despawn();
            }
            break; // One trigger per move
        }
    }
}

// =====================================================================
// Plugin
// =====================================================================

pub struct MachinesPlugin;

impl Plugin for MachinesPlugin {
    fn build(&self, app: &mut App) {
        use crate::game::turns::ProcessingPhase;
        app.add_message::<MachineBumpMessage>()
            .add_systems(
                Update,
                handle_machine_bump.in_set(ProcessingPhase::ResolveActions),
            )
            .add_systems(
                Update,
                (
                    machine_step_system,
                    process_pending_machine_item,
                    process_pending_machine_monsters,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
