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
    fungal_light, rebuild_light_map_system, sync_entity_lights_system, LightMap, LightSource,
    LightSourceData, LightSources, LightingPlugin as EngineLightingPlugin, LightingSet,
    CANDLE_RADIUS, FUNGAL_LIGHT_COLOR, FUNGAL_LIGHT_INTENSITY, FUNGAL_LIGHT_RADIUS,
};

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
