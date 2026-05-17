//! Runtime tile mutation: messages, apply systems, and the
//! [`MapMutationPlugin`] that wires them together.
//!
//! # Why it lives in the engine
//!
//! Mutation messages and their basic apply systems operate purely on
//! engine-owned tile data ([`Tile`], [`TerrainType`], [`LiquidType`],
//! [`Decoration`]) and on engine-owned resources
//! ([`Map`], [`TileEntityIndex`], [`LightSources`]). The systems do
//! universal data sync — Map ↔ tile entity ↔ Viewshed ↔ Collider ↔ light
//! map — and nothing game-specific.
//!
//! # Game-specific reactions
//!
//! Anything that depends on game content (chasm fall consequences, lava
//! kills, log messages, animation triggers) belongs in a **reaction
//! system** the game runs `.after(MapMutationSet)`. The reaction reads
//! the same mutation messages and queries the post-mutation [`Map`]
//! state.
//!
//! [`Tile`]: crate::map::tile::Tile

use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::components::{Collider, Viewshed};
use crate::lighting::LightSources;
use crate::map::map::Map;
use crate::map::promotion::PromotionCooldown;
use crate::map::tile::{is_opaque, is_walkable, Decoration, LiquidType, TerrainType};
use crate::map::tile_entity_index::TileEntityIndex;

// ─── Messages ────────────────────────────────────────────────────────

/// Request to change a tile's terrain at runtime. Handled by
/// [`apply_tile_mutations`], which updates both the [`Map`] resource and
/// the ECS tile entity (terrain component, collider, viewshed dirty
/// flags). Game-side reactions (sprite refresh, log notice) run
/// `.after(MapMutationSet)`.
#[derive(Message)]
pub struct TileMutationMessage {
    pub position: Point,
    pub new_terrain: TerrainType,
}

/// Request to change a tile's decoration at runtime. Handled by
/// [`apply_decoration_mutations`].
#[derive(Message)]
pub struct DecorationMutationMessage {
    pub position: Point,
    pub new_decoration: Decoration,
}

/// Request to change a tile's liquid layer at runtime
/// (e.g. CrackedFloor → Chasm). Handled by [`apply_liquid_mutations`]
/// for data sync; game-side reactions (chasm fall, lava kill)
/// subscribe separately.
#[derive(Message)]
pub struct LiquidMutationMessage {
    pub position: Point,
    pub new_liquid: LiquidType,
}

// ─── System set & plugin ─────────────────────────────────────────────

/// Set marker for the engine's mutation apply systems. Game reaction
/// systems should be ordered `.after(MapMutationSet)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MapMutationSet;

/// Registers mutation message types and their apply systems.
///
/// The plugin does not gate the systems on any state — games configure
/// `.run_if(...)` on [`MapMutationSet`] to fit it into their schedule.
pub struct MapMutationPlugin;

impl Plugin for MapMutationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TileMutationMessage>()
            .add_message::<DecorationMutationMessage>()
            .add_message::<LiquidMutationMessage>()
            .add_systems(
                Update,
                (
                    apply_tile_mutations,
                    apply_decoration_mutations,
                    apply_liquid_mutations,
                )
                    .in_set(MapMutationSet),
            );
    }
}

// ─── Apply systems ───────────────────────────────────────────────────

/// Applies queued tile mutations: updates [`Map`], the tile entity's
/// terrain component, walkability-driven [`Collider`], opacity-driven
/// light dirty, viewshed dirty, and the [`PromotionCooldown`] grace.
pub fn apply_tile_mutations(
    mut commands: Commands,
    mut messages: MessageReader<TileMutationMessage>,
    mut map: ResMut<Map>,
    tile_index: Res<TileEntityIndex>,
    mut tile_query: Query<&mut TerrainType>,
    mut viewshed_query: Query<&mut Viewshed>,
    mut promotion_cooldown: ResMut<PromotionCooldown>,
    mut light_sources: ResMut<LightSources>,
) {
    let mut any = false;

    for msg in messages.read() {
        // 1. Update Map resource (source of truth for game logic).
        let idx = map.xy_idx(msg.position.x, msg.position.y);
        let old_opaque = is_opaque(map.tiles[idx]);
        map.tiles[idx].terrain = msg.new_terrain;
        // If opacity changed (door opened/closed), rebuild the light map.
        if old_opaque != is_opaque(map.tiles[idx]) {
            light_sources.dirty = true;
        }

        // Mark this tile on cooldown so the promotion tick doesn't revert it same-turn.
        promotion_cooldown
            .0
            .insert((msg.position.x, msg.position.y));

        // 2. Look up the ECS tile entity via spatial index.
        let Some(&tile_entity) = tile_index.0.get(&(msg.position.x, msg.position.y)) else {
            warn!(
                "TileMutationMessage at ({}, {}) — no tile entity in index",
                msg.position.x, msg.position.y
            );
            continue;
        };

        let Ok(mut terrain_type) = tile_query.get_mut(tile_entity) else {
            warn!(
                "TileMutationMessage at ({}, {}) — tile entity {:?} missing components",
                msg.position.x, msg.position.y, tile_entity
            );
            continue;
        };

        // 3. Update ECS terrain component.
        *terrain_type = msg.new_terrain;

        // 4. Add or remove Collider based on walkability of the full tile.
        let full_tile = map.tiles[idx];
        if is_walkable(full_tile) {
            commands.entity(tile_entity).remove::<Collider>();
        } else {
            commands.entity(tile_entity).insert(Collider);
        }

        any = true;
    }

    // 5. Mark all viewsheds dirty so FOV is recalculated.
    if any {
        for mut viewshed in viewshed_query.iter_mut() {
            viewshed.dirty = true;
        }
    }
}

/// Applies queued decoration mutations: updates [`Map`], handles
/// [`Decoration::CrackedFloor`] terrain coercion, and marks viewsheds
/// dirty when FOV-blocking changes. No decoration currently emits
/// light; rewire here if a future plant variant ships with a glow.
pub fn apply_decoration_mutations(
    mut commands: Commands,
    mut messages: MessageReader<DecorationMutationMessage>,
    mut map: ResMut<Map>,
    tile_index: Res<TileEntityIndex>,
    mut tile_query: Query<&mut TerrainType>,
    mut viewshed_query: Query<&mut Viewshed>,
    mut promotion_cooldown: ResMut<PromotionCooldown>,
) {
    let mut fov_changed = false;

    for msg in messages.read() {
        let idx = map.xy_idx(msg.position.x, msg.position.y);
        let old_decoration = map.tiles[idx].decoration;
        map.tiles[idx].decoration = msg.new_decoration;

        // Set promotion cooldown so timed promotions don't fire next turn.
        if msg.new_decoration.timed_promotion().is_some() {
            promotion_cooldown.0.insert((msg.position.x, msg.position.y));
        }

        // CrackedFloor converts any non-floor terrain to Floor so the tile
        // renders correctly and the chasm promotion works on all tile types.
        if msg.new_decoration == Decoration::CrackedFloor
            && map.tiles[idx].terrain != TerrainType::Floor
        {
            map.tiles[idx].terrain = TerrainType::Floor;
            if let Some(&tile_entity) = tile_index.0.get(&(msg.position.x, msg.position.y)) {
                if let Ok(mut terrain) = tile_query.get_mut(tile_entity) {
                    *terrain = TerrainType::Floor;
                }
                // Walls becoming floor need Collider removed.
                commands.entity(tile_entity).remove::<Collider>();
            }
            fov_changed = true;
        }

        // No decoration currently emits light. `fungal_light` is retained as a
        // helper for future glow-emitting plants; rewire here when one ships.

        if old_decoration.blocks_fov() != msg.new_decoration.blocks_fov() {
            fov_changed = true;
        }
    }

    if fov_changed {
        for mut viewshed in viewshed_query.iter_mut() {
            viewshed.dirty = true;
        }
    }
}

/// Applies queued liquid mutations: updates [`Map`], clears decoration
/// (so e.g. CrackedFloor doesn't persist on a chasm), syncs the tile
/// entity's liquid component and walkability collider, and marks
/// viewsheds dirty.
///
/// **Game-specific reactions** (chasm fall, lava kill, log notices)
/// must be implemented as separate systems that read
/// [`LiquidMutationMessage`] and run `.after(MapMutationSet)`.
pub fn apply_liquid_mutations(
    mut commands: Commands,
    mut messages: MessageReader<LiquidMutationMessage>,
    mut map: ResMut<Map>,
    tile_index: Res<TileEntityIndex>,
    mut liquid_query: Query<&mut LiquidType>,
    mut viewshed_query: Query<&mut Viewshed>,
    mut promotion_cooldown: ResMut<PromotionCooldown>,
) {
    let mut any = false;

    for msg in messages.read() {
        let idx = map.xy_idx(msg.position.x, msg.position.y);

        // 1. Update Map resource.
        map.tiles[idx].liquid = msg.new_liquid;
        // Clear decoration (e.g. CrackedFloor shouldn't persist on a chasm).
        map.tiles[idx].decoration = Decoration::None;

        // Mark this tile on cooldown so promotion tick doesn't revert it.
        promotion_cooldown
            .0
            .insert((msg.position.x, msg.position.y));

        // 2. Update ECS tile entity.
        let Some(&tile_entity) = tile_index.0.get(&(msg.position.x, msg.position.y)) else {
            warn!(
                "LiquidMutationMessage at ({}, {}) — no tile entity in index",
                msg.position.x, msg.position.y
            );
            continue;
        };

        if let Ok(mut liquid_type) = liquid_query.get_mut(tile_entity) {
            *liquid_type = msg.new_liquid;
        }

        // 3. Update Collider based on walkability.
        let full_tile = map.tiles[idx];
        if is_walkable(full_tile) {
            commands.entity(tile_entity).remove::<Collider>();
        } else {
            commands.entity(tile_entity).insert(Collider);
        }

        any = true;
    }

    if any {
        for mut viewshed in viewshed_query.iter_mut() {
            viewshed.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Position;
    use crate::map::tile::Tile;

    fn make_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(MapMutationPlugin);
        // Init lighting resource without the LightingPlugin so the rebuild
        // system doesn't consume the dirty flag inside the same frame.
        app.init_resource::<LightSources>();
        app.init_resource::<TileEntityIndex>();
        app.init_resource::<PromotionCooldown>();

        // Build a small open map.
        let mut map = Map::new(1, 5, 5, "test");
        for y in 1..4 {
            for x in 1..4 {
                let idx = map.xy_idx(x, y);
                map.tiles[idx] = Tile {
                    terrain: TerrainType::Floor,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
            }
        }
        app.insert_resource(map);
        app
    }

    /// Spawn a tile entity at (x, y) and register it in TileEntityIndex.
    fn spawn_tile(app: &mut App, x: i32, y: i32, terrain: TerrainType, liquid: LiquidType) {
        let entity = app
            .world_mut()
            .spawn((Position { x, y }, terrain, liquid))
            .id();
        app.world_mut()
            .resource_mut::<TileEntityIndex>()
            .0
            .insert((x, y), entity);
    }

    #[test]
    fn apply_tile_mutation_updates_map_and_entity() {
        let mut app = make_test_app();
        spawn_tile(&mut app, 2, 2, TerrainType::Floor, LiquidType::None);

        // Add a viewshed entity to verify dirty propagation
        let viewer = app.world_mut().spawn(Viewshed::default()).id();

        app.world_mut()
            .resource_mut::<Messages<TileMutationMessage>>()
            .write(TileMutationMessage {
                position: Point::new(2, 2),
                new_terrain: TerrainType::Wall,
            });
        app.update();

        let map = app.world().resource::<Map>();
        let idx = map.xy_idx(2, 2);
        assert_eq!(map.tiles[idx].terrain, TerrainType::Wall);

        let v = app.world().get::<Viewshed>(viewer).unwrap();
        assert!(v.dirty, "viewshed should be marked dirty");

        let cooldown = app.world().resource::<PromotionCooldown>();
        assert!(cooldown.0.contains(&(2, 2)));
    }

    #[test]
    fn opacity_change_marks_lights_dirty() {
        let mut app = make_test_app();
        spawn_tile(&mut app, 2, 2, TerrainType::Floor, LiquidType::None);

        // Reset light dirty flag set by plugin init
        app.world_mut().resource_mut::<LightSources>().dirty = false;

        // Floor → Wall flips opacity from transparent to opaque.
        app.world_mut()
            .resource_mut::<Messages<TileMutationMessage>>()
            .write(TileMutationMessage {
                position: Point::new(2, 2),
                new_terrain: TerrainType::Wall,
            });
        app.update();

        assert!(app.world().resource::<LightSources>().dirty);
    }

    #[test]
    fn fungus_decoration_does_not_add_light() {
        // No shipping decoration currently emits light. If a future glow-plant
        // is wired into apply_decoration_mutations, replace this test.
        let mut app = make_test_app();
        spawn_tile(&mut app, 2, 2, TerrainType::Floor, LiquidType::None);
        app.world_mut().resource_mut::<LightSources>().dirty = false;

        app.world_mut()
            .resource_mut::<Messages<DecorationMutationMessage>>()
            .write(DecorationMutationMessage {
                position: Point::new(2, 2),
                new_decoration: Decoration::Fungus,
            });
        app.update();

        let lights = app.world().resource::<LightSources>();
        assert!(lights.sources.is_empty(), "Fungus must not emit light");
    }

    #[test]
    fn cracked_floor_coerces_terrain_to_floor() {
        let mut app = make_test_app();
        // Place a wall tile.
        let map = app.world_mut().resource_mut::<Map>();
        let idx = map.xy_idx(0, 0);
        // (Already wall by default.)
        let _ = idx;
        spawn_tile(&mut app, 0, 0, TerrainType::Wall, LiquidType::None);

        app.world_mut()
            .resource_mut::<Messages<DecorationMutationMessage>>()
            .write(DecorationMutationMessage {
                position: Point::new(0, 0),
                new_decoration: Decoration::CrackedFloor,
            });
        app.update();

        let map = app.world().resource::<Map>();
        let idx = map.xy_idx(0, 0);
        assert_eq!(map.tiles[idx].terrain, TerrainType::Floor);
    }

    #[test]
    fn liquid_mutation_clears_decoration() {
        let mut app = make_test_app();
        // Seed a tile with a decoration.
        {
            let mut map = app.world_mut().resource_mut::<Map>();
            let idx = map.xy_idx(2, 2);
            map.tiles[idx].decoration = Decoration::Grass;
        }
        spawn_tile(&mut app, 2, 2, TerrainType::Floor, LiquidType::None);

        app.world_mut()
            .resource_mut::<Messages<LiquidMutationMessage>>()
            .write(LiquidMutationMessage {
                position: Point::new(2, 2),
                new_liquid: LiquidType::Chasm,
            });
        app.update();

        let map = app.world().resource::<Map>();
        let idx = map.xy_idx(2, 2);
        assert_eq!(map.tiles[idx].liquid, LiquidType::Chasm);
        assert_eq!(map.tiles[idx].decoration, Decoration::None);
    }
}
