//! Sets `GameLog.hover_description` based on what the mouse cursor is pointing at.
//! Shows tile terrain, liquid, decorations, and any entity (monster, item, prop) at that position.
//! Also triggered by the nearby panel's Tab selection.

use bevy::prelude::*;
use bracket_lib::prelude::Algorithm2D;

use crate::components::{InInventory, Item, Monster, Name, Position, Prop};
use crate::constants::TILE_SIZE_X;
use crate::game::camera::MainCamera;
use crate::game::combat::Health;
use crate::game::AppState;
use crate::map::map::Map;
use crate::map::tile::{TerrainType, LiquidType, Decoration};
use crate::player::Player;
use crate::ui::game_log::GameLog;
use crate::ui::nearby::NearbyState;

pub struct HoverInfoPlugin;

impl Plugin for HoverInfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update_hover_description
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_hover_description(
    windows: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    map: Res<Map>,
    mut game_log: ResMut<GameLog>,
    nearby_state: Res<NearbyState>,
    monster_query: Query<(&Position, &Name, &Health), With<Monster>>,
    item_query: Query<(&Position, &Name), (With<Item>, Without<InInventory>)>,
    prop_query: Query<(&Position, &Name), With<Prop>>,
    player_query: Query<&Position, With<Player>>,
    // For entities from the nearby list that might be items/props
    name_query: Query<&Name>,
    pos_query: Query<&Position>,
) {
    let mut description: Option<String> = None;

    // Determine grid position: mouse hover takes priority, then nearby selection
    let mut grid_pos: Option<(i32, i32)> = None;
    let mut nearby_entity: Option<Entity> = None;

    // Mouse hover → grid position
    if let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), q_camera.single())
        && let Some(screen_pos) = window.cursor_position()
            && let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
                let gx = (world_pos.x / TILE_SIZE_X as f32 + 0.5).floor() as i32;
                let gy = (world_pos.y / TILE_SIZE_X as f32 + 0.5).floor() as i32;
                if map.in_bounds(bracket_lib::prelude::Point::new(gx, gy)) {
                    grid_pos = Some((gx, gy));
                }
            }

    // Fallback: nearby panel selection
    if grid_pos.is_none()
        && let Some(idx) = nearby_state.selected_idx
            && let Some(&entity) = nearby_state.entity_list.get(idx) {
                nearby_entity = Some(entity);
                if let Ok(pos) = pos_query.get(entity) {
                    grid_pos = Some((pos.x, pos.y));
                }
            }

    if let Some((gx, gy)) = grid_pos {
        let idx = map.xy_idx(gx, gy);
        let tile = map.tiles[idx];

        // Only show info for explored tiles
        if !map.explored_tiles[idx] {
            game_log.hover_description = None;
            return;
        }

        let mut parts: Vec<String> = Vec::new();

        // Check for entities at this position (priority: monster > player > item > prop)
        let mut found_entity = false;

        // Monster (name only — HP shown in the tooltip panel)
        for (pos, name, _health) in monster_query.iter() {
            if pos.x == gx && pos.y == gy {
                parts.push(name.0.clone());
                found_entity = true;
                break;
            }
        }

        // Player
        if !found_entity {
            for pos in player_query.iter() {
                if pos.x == gx && pos.y == gy {
                    parts.push(format!("You ({}, {})", pos.x, pos.y));
                    found_entity = true;
                    break;
                }
            }
        }

        // Item
        if !found_entity {
            for (pos, name) in item_query.iter() {
                if pos.x == gx && pos.y == gy {
                    parts.push(name.0.clone());
                    found_entity = true;
                    break;
                }
            }
        }

        // Prop
        if !found_entity {
            for (pos, name) in prop_query.iter() {
                if pos.x == gx && pos.y == gy {
                    parts.push(name.0.clone());
                    found_entity = true;
                    break;
                }
            }
        }

        // If we came from the nearby panel and didn't find anything at grid pos,
        // use the nearby entity's name directly
        if !found_entity
            && let Some(entity) = nearby_entity
                && let Ok(name) = name_query.get(entity) {
                    parts.push(name.0.clone());
                    found_entity = true;
                }

        // Tile description
        let terrain_desc = match tile.terrain {
            TerrainType::Wall => "wall",
            TerrainType::Floor => "floor",
            TerrainType::DownStairs => "stairs leading down",
            TerrainType::UpStairs => "stairs leading up",
            TerrainType::Empty => "",
            TerrainType::Door => "a closed door",
            TerrainType::OpenDoor => "an open door",
            TerrainType::HiddenDoor => "wall", // looks like wall until discovered
            TerrainType::LockedDoor => "a locked door",
        };

        let liquid_desc = match tile.liquid {
            LiquidType::None => "",
            LiquidType::Water => "deep water",
            LiquidType::ShallowWater => "shallow water",
            LiquidType::Lava => "lava",
            LiquidType::Chasm => "a chasm",
        };

        let decoration_desc = match tile.decoration {
            Decoration::None => "",
            Decoration::Grass => "grass",
            Decoration::TallGrass => "tall grass",
            Decoration::DeadGrass => "dead grass",
            Decoration::Rubble => "rubble",
            Decoration::Moss => "moss",
            Decoration::Fungus => "fungus",
            Decoration::Cobweb => "cobwebs",
            Decoration::Bloodstain => "bloodstains",
            Decoration::ScorchedEarth => "scorched earth",
        };

        // Build description: entity on terrain/liquid
        if !found_entity {
            // No entity — describe the tile itself
            if !liquid_desc.is_empty() {
                parts.push(liquid_desc.to_string());
            } else if !decoration_desc.is_empty() {
                parts.push(format!("{} on {}", decoration_desc, terrain_desc));
            } else if !terrain_desc.is_empty() {
                parts.push(terrain_desc.to_string());
            }
        } else {
            // Entity found — add terrain context
            if !liquid_desc.is_empty() {
                parts.push(format!("in {}", liquid_desc));
            } else if !terrain_desc.is_empty() && terrain_desc != "floor" {
                parts.push(format!("on {}", terrain_desc));
            }
        }

        if parts.is_empty() {
            description = None;
        } else {
            // Capitalize first letter, append grid coordinates for debugging
            let desc = parts.join(" ");
            let mut chars = desc.chars();
            let capitalized = match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => desc,
            };
            description = Some(format!("{} ({}, {})", capitalized, gx, gy));
        }
    }

    game_log.hover_description = description;
}
