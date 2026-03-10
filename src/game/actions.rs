use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::{Collider, InInventory, Inventory, Monster, Name, Position, Viewshed, Item, AmuletOfBevy},
    constants::BASE_ACTION_COST,
    game::{combat::AttackIntentMessage, AppState},
    map::{Map, tile::{is_walkable, TerrainType, TileMarker}, map::DungeonECSMap},
    player::Player,
    assets::{TileManifest, TileManifestHandle, TileSpriteAssets},
    ui::game_log::GameLogMessage,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Wait,
    Move { dir: Direction },
    MeleeAttack { target: Entity },
    PickUp,
    EquipItem   { item: Entity },
    UnequipItem { item: Entity },
    DropItem    { item: Entity },
    UseItem     { item: Entity },
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

#[derive(Component, Clone)]
pub struct SpeedStats {
    pub delay: f32, // e.g., 0.5 for half time,  2.0 for double time
}

impl Default for SpeedStats {
    fn default() -> Self {
        Self { delay: 1.0 }
    }
}

/// Emitted by any action system when an action successfully resolves (or fails)
/// to signal the turn manager to move to the next entity.
#[derive(Message)]
pub struct ActionFinishedEvent {
    pub entity: Entity,
    pub base_cost: u32,
}

// --- Systems ---

pub fn handle_pickup(
    mut commands: Commands,
    mut intents: MessageReader<PickUpIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    actors_query: Query<(Entity, &Position, Has<Player>)>,
    items_query: Query<(Entity, &Position, &Name, Has<AmuletOfBevy>), (With<Item>, Without<InInventory>)>,
    mut inv_query: Query<&mut Inventory, With<Player>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for intent in intents.read() {
        let Ok((actor_entity, actor_pos, is_player)) = actors_query.get(intent.entity) else {
            continue;
        };

        let mut picked_up = false;
        for (item_entity, item_pos, item_name, is_amulet) in items_query.iter() {
            if actor_pos != item_pos {
                continue;
            }

            if is_player && is_amulet {
                info!("Player picked up the Amulet of Bevy! VICTORY!");
                next_state.set(AppState::Victory);
                commands.entity(item_entity).despawn();
                picked_up = true;
                break;
            }

            if is_player {
                if let Ok(mut inv) = inv_query.single_mut() {
                    if inv.items.len() < inv.capacity {
                        inv.items.push(item_entity);
                        commands
                            .entity(item_entity)
                            .insert(InInventory)
                            .insert(Visibility::Hidden)
                            .remove::<crate::components::FloorEntityMarker>();
                        log_writer.write(GameLogMessage(format!("You pick up the {}.", item_name.0)));
                        picked_up = true;
                        break;
                    } else {
                        log_writer.write(GameLogMessage("Your inventory is full!".to_string()));
                    }
                }
            } else {
                commands.entity(item_entity).despawn();
                picked_up = true;
                break;
            }
        }

        if picked_up || is_player {
            finish_writer.write(ActionFinishedEvent {
                entity: actor_entity,
                base_cost: BASE_ACTION_COST,
            });
        }
    }
}

/// Handles movement. If a collision with a hostile entity is detected,
/// it converts the movement into a MeleeIntent instead.
/// If the target tile is a closed door, it converts it into an OpenDoorIntent.
pub fn handle_movement(
    mut intents: MessageReader<MovementIntent>,
    mut melee_writer: MessageWriter<MeleeIntent>,
    mut open_door_writer: MessageWriter<OpenDoorIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut actors_query: Query<(
        Entity,
        &mut Position,
        Has<Player>,
        Has<Monster>,
        Has<Collider>,
    ), (Without<TileMarker>, Without<Item>)>,
    map: Res<Map>,
) {
    for intent in intents.read() {
        let Ok((_, pos, _, _, _)) = actors_query.get(intent.entity) else {
            finish_writer.write(ActionFinishedEvent {
                entity: intent.entity,
                base_cost: BASE_ACTION_COST,
            });
            continue;
        };

        let target_pt = pos.to_point() + intent.dir.offset();

        // 1. Bounds check
        if !map.in_bounds(target_pt) {
            finish_writer.write(ActionFinishedEvent {
                entity: intent.entity,
                base_cost: BASE_ACTION_COST,
            });
            continue;
        }

        let target_tile = map.tiles[map.xy_idx(target_pt.x, target_pt.y)];

        // 2. Closed Door Check
        if target_tile.terrain == TerrainType::Door {
            open_door_writer.write(OpenDoorIntent {
                entity: intent.entity,
                door_pos: target_pt,
            });
            continue;
        }

        // 3. Occupant Check (Bump-to-Attack / Block) — must happen before wall check
        //    so that monsters standing on non-walkable tiles can still be attacked.
        let mut bump_target = None;
        for (e, other_pos, other_is_player, other_is_monster, other_has_collider) in
            actors_query.iter()
        {
            if other_pos.to_point() == target_pt && e != intent.entity {
                bump_target = Some((e, other_is_player, other_is_monster, other_has_collider));
                break;
            }
        }

        if let Some((target_entity, target_is_player, target_is_monster, target_has_collider)) =
            bump_target
        {
            let actor_is_player = actors_query
                .get(intent.entity)
                .map(|(_, _, p, _, _)| p)
                .unwrap_or(false);
            let actor_is_monster = actors_query
                .get(intent.entity)
                .map(|(_, _, _, m, _)| m)
                .unwrap_or(false);

            let is_hostile =
                (actor_is_player && target_is_monster) || (actor_is_monster && target_is_player);

            if is_hostile {
                melee_writer.write(MeleeIntent {
                    attacker: intent.entity,
                    target: target_entity,
                });
                continue;
            } else if target_has_collider {
                // Blocked by friendly/neutral with a Collider
                finish_writer.write(ActionFinishedEvent {
                    entity: intent.entity,
                    base_cost: BASE_ACTION_COST,
                });
                continue;
            }
            // If neither hostile nor blocking collider, fall through to movement
        }

        // 4. Wall/Obstacle Check
        if !is_walkable(target_tile) {
            finish_writer.write(ActionFinishedEvent {
                entity: intent.entity,
                base_cost: BASE_ACTION_COST,
            });
            continue;
        }

        // 5. Apply Movement
        if let Ok((_, mut pos, _, _, _)) = actors_query.get_mut(intent.entity) {
            pos.x = target_pt.x;
            pos.y = target_pt.y;
        }
        finish_writer.write(ActionFinishedEvent {
            entity: intent.entity,
            base_cost: BASE_ACTION_COST,
        });
    }
}

pub fn handle_melee(
    mut intents: MessageReader<MeleeIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut attack_writer: MessageWriter<AttackIntentMessage>,
) {
    for intent in intents.read() {
        attack_writer.write(AttackIntentMessage {
            attacker: intent.attacker,
            target: intent.target,
        });
        finish_writer.write(ActionFinishedEvent {
            entity: intent.attacker,
            base_cost: BASE_ACTION_COST,
        });
    }
}

pub fn handle_wait(
    mut intents: MessageReader<WaitIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    for intent in intents.read() {
        finish_writer.write(ActionFinishedEvent {
            entity: intent.entity,
            base_cost: BASE_ACTION_COST,
        });
    }
}

pub fn handle_door_open(
    mut commands: Commands,
    mut intents: MessageReader<OpenDoorIntent>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut map: ResMut<Map>,
    mut tile_query: Query<(Entity, &Position, &mut TerrainType, &mut Sprite)>,
    mut viewshed_query: Query<&mut Viewshed>,
    tile_manifests: Res<Assets<TileManifest>>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_sprite_assets: Res<TileSpriteAssets>,
) {
    let Some(tile_manifest) = tile_manifests.get(&tile_manifest_handle.0) else {
        return;
    };

    for intent in intents.read() {
        let idx = map.xy_idx(intent.door_pos.x, intent.door_pos.y);
        
        // Logical Update
        map.tiles[idx].terrain = TerrainType::OpenDoor;

        // Visual Update by querying for the tile entity at the correct position
        for (tile_entity, pos, mut terrain_type, mut sprite) in tile_query.iter_mut() {
            if pos.x == intent.door_pos.x && pos.y == intent.door_pos.y {
                *terrain_type = TerrainType::OpenDoor;
                
                if let Some(asset) = tile_manifest.tiles.get(TerrainType::OpenDoor.name()) {
                    let sprite_path_parts: Vec<&str> = asset.sprite.split('#').collect();
                    let texture_path = sprite_path_parts[0];
                    let index = sprite_path_parts[1].parse::<usize>().unwrap_or_default();
                    
                    // Update Sprite image, index, and layout
                    if let Some(texture_handle) = tile_sprite_assets.handles.get(texture_path) {
                        sprite.image = texture_handle.clone();
                    }
                    if let Some(layout_handle) = tile_sprite_assets.layouts.get(texture_path) {
                        if let Some(ref mut texture_atlas) = sprite.texture_atlas {
                            texture_atlas.index = index;
                            texture_atlas.layout = layout_handle.clone();
                        }
                    }
                }

                // Remove collider so we can walk through it
                commands.entity(tile_entity).remove::<Collider>();
                break;
            }
        }

        // Trigger vision refresh for everyone
        for mut viewshed in viewshed_query.iter_mut() {
            viewshed.dirty = true;
        }

        finish_writer.write(ActionFinishedEvent {
            entity: intent.entity,
            base_cost: BASE_ACTION_COST,
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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
