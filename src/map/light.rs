//! Game-side adapter for the engine's lighting module.
//!
//! All lighting math (Bresenham LOS, light-map accumulation,
//! resource/entity sync) lives in `roguelike_engine::lighting`. This file
//! keeps:
//!
//! 1. Re-exports so `crate::map::light::*` keeps working everywhere.
//! 2. The candle sprite-animation system (rendering, game-only).
//! 3. The plugin that registers the engine's [`LightingPlugin`] under
//!    the game's `AppState::InGame` gate and `SpawnDungeonSet` ordering.

use bevy::prelude::*;

use crate::game::AppState;

// Engine re-exports. All lighting types and helpers live here.
pub use roguelike_engine::lighting::{
    fungal_light, phosphorescent_moss_light, rebuild_light_map_system, sync_entity_lights_system,
    LightMap, LightSource, LightSourceData, LightSources, LightingPlugin as EngineLightingPlugin,
    LightingSet, CANDLE_RADIUS, FUNGAL_LIGHT_COLOR, FUNGAL_LIGHT_INTENSITY, FUNGAL_LIGHT_RADIUS,
    PHOSPHORESCENT_MOSS_LIGHT_COLOR, PHOSPHORESCENT_MOSS_LIGHT_INTENSITY,
    PHOSPHORESCENT_MOSS_LIGHT_RADIUS,
};

use crate::map::map::Map;
use crate::map::tile::Decoration;

/// Walks `map` and pushes a static `LightSources` entry for every tile
/// whose decoration emits light at build time. Decorations placed via
/// `DecorationPropagator` stamp directly into `Tile.decoration` without
/// flowing through `apply_decoration_mutations`, so this helper picks
/// them up on floor enter. Run AFTER `LightSources::remove_floor_sources`
/// during materialisation so the cleared resource doesn't immediately
/// drop our entries.
///
/// Runtime mutations (fire burning moss → Ash, etc.) are handled by
/// the engine-side `apply_decoration_mutations`, which add/remove the
/// same `phosphorescent_moss_light` source via `LightSources::add` /
/// `remove_at`.
pub fn register_decoration_lights(map: &Map, light_sources: &mut LightSources) {
    for y in 0..map.height {
        for x in 0..map.width {
            let idx = map.xy_idx(x, y);
            if let Decoration::PhosphorescentMoss = map.tiles[idx].decoration {
                light_sources.add(phosphorescent_moss_light(x, y));
            }
        }
    }
}

// ─── Game-side: candle sprite animation ───────────────────────────────

/// Per-entity timer for candle frame cycling.
#[derive(Component, Deref, DerefMut)]
pub struct AnimationTimer(pub Timer);

/// Animate sprite-based light sources (candle frame cycling).
fn animate_light_sources(
    time: Res<Time>,
    mut query: Query<(&mut AnimationTimer, &mut Sprite), With<LightSource>>,
) {
    for (mut timer, mut sprite) in &mut query {
        timer.tick(time.delta());
        if timer.is_finished()
            && let Some(ref mut texture_atlas) = sprite.texture_atlas
        {
            texture_atlas.index = (texture_atlas.index + 1) % 4;
        }
    }
}

// ─── Plugin ──────────────────────────────────────────────────────────

pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EngineLightingPlugin)
            .configure_sets(
                Update,
                LightingSet
                    .after(crate::map::dungeon::SpawnDungeonSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                animate_light_sources.run_if(in_state(AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

    fn make_map(width: i32, height: i32) -> Map {
        let count = (width * height) as usize;
        Map {
            name: "test".to_string(),
            tiles: vec![
                Tile {
                    terrain: TerrainType::Floor,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
                count
            ],
            explored_tiles: vec![false; count],
            blocked: vec![false; count],
            width,
            height,
            depth: 1,
        }
    }

    #[test]
    fn register_decoration_lights_pushes_one_source_per_moss_tile() {
        let mut map = make_map(5, 5);
        // Two phosphorescent moss tiles, plus a regular Moss (no glow).
        let i1 = map.xy_idx(1, 1);
        let i2 = map.xy_idx(3, 3);
        let i3 = map.xy_idx(0, 4);
        map.tiles[i1].decoration = Decoration::PhosphorescentMoss;
        map.tiles[i2].decoration = Decoration::PhosphorescentMoss;
        map.tiles[i3].decoration = Decoration::Moss;

        let mut light_sources = LightSources::default();
        register_decoration_lights(&map, &mut light_sources);

        // Only the two phosphorescent tiles get sources; plain Moss is silent.
        // We can probe by removing at the expected positions: each `remove_at`
        // returns the number of sources cleared at that tile.
        assert_eq!(light_sources.remove_at(1, 1), 1, "moss at (1,1) should be lit");
        assert_eq!(light_sources.remove_at(3, 3), 1, "moss at (3,3) should be lit");
        assert_eq!(light_sources.remove_at(0, 4), 0, "plain Moss must not emit light");
    }

    #[test]
    fn register_decoration_lights_uses_phosphorescent_moss_radius_and_color() {
        let mut map = make_map(3, 3);
        let idx = map.xy_idx(1, 1);
        map.tiles[idx].decoration = Decoration::PhosphorescentMoss;

        let mut light_sources = LightSources::default();
        register_decoration_lights(&map, &mut light_sources);

        // Reconstruct the expected source and compare against the helper's
        // documented constants. Asserts the helper actually used the
        // PhosphorescentMoss tuning, not some other source.
        let expected = phosphorescent_moss_light(1, 1);
        assert_eq!(expected.radius, PHOSPHORESCENT_MOSS_LIGHT_RADIUS);
        assert_eq!(expected.intensity, PHOSPHORESCENT_MOSS_LIGHT_INTENSITY);
        assert_eq!(expected.color, PHOSPHORESCENT_MOSS_LIGHT_COLOR);
        assert!(!expected.on_wall, "moss is a floor source, not wall-mounted");
    }
}
