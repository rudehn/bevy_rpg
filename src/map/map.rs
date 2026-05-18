use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::{Position, Viewshed},
    game::AppState,
    map::{
        tile::{TileExplored, TileVisibility},
    },
    player::Player,
    ui::game_log::GameLogMessage,
};

// Map, MapWithMode, and populate_blocked_tiles now live in the engine.
pub use roguelike_engine::map::{Map, MapWithMode, populate_blocked_tiles};

/*
There are two map types.

1. The Map struct defined here. This grid based map handles all game logic, from map generation
   to collision and fog of war.

2. The ECS Entity tiles. This handles the rendering of all entities on the level.
   This handles sprites, visibility, pixel location, etc.

*/
pub const GRID_SIZE: Vec2 = Vec2 { x: 16.0, y: 16.0 };
pub const MAP_SIZE: UVec2 = UVec2 { x: 80, y: 60 };

#[derive(Message, Debug, Clone, Copy)]
pub struct RevealMapMessage;

/// Set to true by spawn_dungeon when restoring a saved or cached floor so that
/// previously-explored tiles are rendered as dim/explored instead of hidden.
#[derive(Resource, Default)]
pub struct NeedsExploredInit(pub bool);

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Map::default())
            .init_resource::<NeedsExploredInit>()
            .init_resource::<crate::map::tile::TileEntityIndex>()
            .add_message::<RevealMapMessage>()
            .add_systems(
                Update,
                (
                    init_explored_tiles_system,
                    update_tile_visibility
                        .after(crate::map::light::rebuild_light_map_system)
                        // Skip during floor transitions: stale old-floor tile entities
                        // would be processed with new-floor FOV, marking wrong tiles explored.
                        // init_explored_tiles_system clears this flag once new tiles are ready.
                        .run_if(|init: Res<NeedsExploredInit>| !init.0)
                        .after(init_explored_tiles_system)
                        // Run after the interior-opaque cull so the
                        // viewshed reflects the post-filter set when
                        // tiles get marked Visible/Explored.
                        .after(crate::game::systems::cull_interior_opaque_from_fov),
                    crate::map::ascii_renderer::render_tile_ascii
                        .after(update_tile_visibility)
                        .after(crate::game::systems::fov_update_system)
                        .after(crate::game::combat::CombatDamageSet),
                    handle_reveal_map_system.run_if(on_message::<RevealMapMessage>),
                ).run_if(in_state(AppState::InGame)),
            );
    }
}

// Tag for the entity that holds the map storage
#[derive(Component)]
pub struct DungeonECSMap; // Tag for entity holding the active ECS map marker

// --------------------------------------------------------------------------------
// SYSTEMS
// --------------------------------------------------------------------------------

pub fn handle_reveal_map_system(
    mut messages: MessageReader<RevealMapMessage>,
    mut tile_render_query: Query<(&mut TileExplored, &mut Visibility)>,
    mut map: ResMut<Map>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for _ in messages.read() {
        // Mark all tiles as explored in the Map resource so the state
        // survives save/load and floor transitions.
        for flag in map.explored_tiles.iter_mut() {
            *flag = true;
        }
        for (mut tile_explored, mut visibility) in tile_render_query.iter_mut() {
            *tile_explored = TileExplored::Explored;
            *visibility = Visibility::Visible;
        }
        log_writer.write(GameLogMessage("The map has been revealed!".to_string()));
    }
}

/// Initializes tile explored state from the Map resource after a save load or
/// floor restore. Runs once (flag is cleared immediately after).
pub fn init_explored_tiles_system(
    mut needs_init: ResMut<NeedsExploredInit>,
    mut map: ResMut<Map>,
    mut tile_query: Query<(&Position, &mut TileExplored, &mut Visibility)>,
) {
    if !needs_init.0 {
        return;
    }
    // New tile entities are spawned via deferred commands — if none exist yet,
    // wait until next frame when the command queue has been flushed.
    if tile_query.is_empty() {
        return;
    }
    needs_init.0 = false;

    // Scrub interior-opaque tiles from the restored explored set BEFORE
    // we propagate it to tile-entity components. Older saves / caches
    // (built before the runtime FOV cull) can carry stale `true` flags
    // for trees buried inside dense clusters; without this pass they'd
    // render as dim memory glyphs forever.
    let w = map.width;
    let h = map.height;
    for y in 0..h {
        for x in 0..w {
            let idx = map.xy_idx(x, y);
            if idx < map.explored_tiles.len()
                && map.explored_tiles[idx]
                && crate::game::systems::is_interior_opaque(&map, x, y)
            {
                map.explored_tiles[idx] = false;
            }
        }
    }

    for (pos, mut tile_explored, mut visibility) in tile_query.iter_mut() {
        let idx = map.xy_idx(pos.x, pos.y);
        if map.explored_tiles.get(idx).copied().unwrap_or(false) {
            *tile_explored = TileExplored::Explored;
            *visibility = Visibility::Visible;
        }
    }
}

pub fn update_tile_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    viewshed_changed: Query<(), (With<Player>, Changed<Viewshed>)>,
    mut map: ResMut<Map>,
    fire_tiles: Res<crate::game::fire::FireTiles>,
    gas_tiles: Res<crate::game::gas::GasTiles>,
    omniscient: Res<crate::game::systems::Omniscient>,
    mut tile_query: Query<(
        &Position,
        &mut TileVisibility,
        &mut TileExplored,
        &mut Visibility,
    )>,
) {
    // Run when viewshed changes, omniscient toggles,
    // or fire/gas state changes (so extinguished tiles get repainted immediately).
    let viewshed_dirty = !viewshed_changed.is_empty();
    let omni_dirty = omniscient.is_changed();
    let effects_dirty = fire_tiles.is_changed() || gas_tiles.is_changed();
    if !viewshed_dirty && !omni_dirty && !effects_dirty {
        return;
    }

    let Ok(player_viewshed) = player_query.single() else {
        return;
    };

    let fov_tiles = &player_viewshed.visible_tiles;
    let omni = omniscient.0;

    let mut newly_explored = Vec::new();

    for (tile_pos, mut tile_visibility, mut tile_explored, mut visibility) in
        tile_query.iter_mut()
    {
        let current_point = Point::new(tile_pos.x, tile_pos.y);

        if omni || fov_tiles.contains(&current_point) {
            *tile_visibility = TileVisibility::Visible;
            *tile_explored = TileExplored::Explored;
            *visibility = Visibility::Visible;

            if map.in_bounds(current_point) {
                let idx = map.xy_idx(tile_pos.x, tile_pos.y);
                if !map.explored_tiles[idx] {
                    newly_explored.push(idx);
                }
            }
        } else {
            *tile_visibility = TileVisibility::Hidden;
            if *tile_explored == TileExplored::Explored {
                *visibility = Visibility::Visible;
            } else {
                *visibility = Visibility::Hidden;
            }
        }
    }

    if !newly_explored.is_empty() {
        for idx in newly_explored {
            map.explored_tiles[idx] = true;
        }
    }
}

// `populate_blocked_tiles`, `Map`, `MapWithMode` and their bracket-lib
// trait impls are re-exported from the engine above. All Map/MapWithMode
// tests also live in the engine crate now.
//
// Game-side only: nothing below — this is the end of the stripped section.
// The file above retains MapPlugin, GRID_SIZE, MAP_SIZE, DungeonECSMap,
// RevealMapMessage, NeedsExploredInit, init_explored_tiles_system,
// update_tile_visibility, and handle_reveal_map_system.
//
// (The block below is a compile-gate: everything from the old Map struct
// through the test module has been removed.)

// REMOVED: pub struct Map { ... }
// REMOVED: impl Map { ... }
// REMOVED: impl BaseMap for Map { ... }
// REMOVED: impl Algorithm2D for Map { ... }
// REMOVED: pub struct MapWithMode { ... }
// REMOVED: impl MapWithMode { ... }
// REMOVED: impl BaseMap for MapWithMode { ... }
// REMOVED: impl Algorithm2D for MapWithMode { ... }
// REMOVED: pub fn populate_blocked_tiles(...) { ... }
// REMOVED: mod tests { ... }
// All code replaced by `pub use roguelike_engine::map::*` above.
// If you see this marker after a merge conflict: delete everything below this line,
// the canonical code is in crates/roguelike_engine/src/map/map.rs.
