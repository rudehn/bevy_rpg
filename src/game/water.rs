use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    components::{Drifting, FloorEntityMarker, InInventory, Inventory, Name, Position},
    game::{
        combat::GameRng,
        magic::{StatusEffectKind, StatusEffects},
        turns::TurnEndEvent,
        AppState,
    },
    map::{tile::{LiquidType, is_walkable}, Map},
    ui::game_log::GameLogMessage,
};

// --- Water shimmer resources ---

/// Spatial index of water tile positions and their liquid type.
/// Populated during floor materialization; used by the shimmer animation system.
#[derive(Resource, Default)]
pub struct WaterTiles(pub HashMap<(i32, i32), LiquidType>);

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                deep_water_item_sweep_system,
                item_drift_system.after(deep_water_item_sweep_system),
                water_extinguish_system,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Sweep items out of inventories of entities standing in deep water.
/// Each item has a 50% chance per turn to be swept away.
fn deep_water_item_sweep_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut actors: Query<(&Position, &mut Inventory, &Name)>,
    item_names: Query<&Name>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    mut log_writer: MessageWriter<GameLogMessage>,
    map: Res<Map>,
) {
    // Consume all turn events but only sweep once per frame to prevent
    // items being ejected multiple times when many turns resolve at once.
    if turn_end.read().count() == 0 {
        return;
    }

    for (pos, mut inventory, _actor_name) in actors.iter_mut() {
        let idx = map.xy_idx(pos.x, pos.y);
        if idx >= map.tiles.len() || map.tiles[idx].liquid != LiquidType::Water {
            continue;
        }

        // Iterate in reverse to avoid index invalidation when removing
        let mut i = inventory.items.len();
        while i > 0 {
            i -= 1;
            let roll = game_rng.0.range(0, 100);
            if roll < 50 {
                let item_entity = inventory.items.remove(i);
                let item_name = item_names
                    .get(item_entity)
                    .map(|n| n.0.clone())
                    .unwrap_or_else(|_| "item".to_string());

                commands.entity(item_entity)
                    .remove::<InInventory>()
                    .insert(Position { x: pos.x, y: pos.y })
                    .insert(FloorEntityMarker)
                    .insert(Drifting)
                    .insert(Visibility::Inherited);

                log_writer.write(GameLogMessage(format!(
                    "Your {} is swept away by the current!",
                    item_name
                )));
            }
        }
    }
}

/// Drift items with the `Drifting` component 1 tile per turn.
/// Only drifts once per frame regardless of how many turn events fired.
/// If they reach a non-deep-water tile, they stop drifting.
fn item_drift_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut drifting_items: Query<(Entity, &mut Position), With<Drifting>>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    map: Res<Map>,
) {
    const OFFSETS: [(i32, i32); 8] = [
        (0, 1), (0, -1), (1, 0), (-1, 0),
        (1, 1), (1, -1), (-1, 1), (-1, -1),
    ];

    // Consume all turn events but only drift once per frame to prevent
    // items teleporting across the map when many turns resolve at once.
    if turn_end.read().count() == 0 {
        return;
    }

    for (entity, mut pos) in drifting_items.iter_mut() {
        // Collect walkable adjacent tiles
        let mut candidates: Vec<(i32, i32, bool)> = Vec::new();
        for &(dx, dy) in &OFFSETS {
            let nx = pos.x + dx;
            let ny = pos.y + dy;
            if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height {
                continue;
            }
            let idx = map.xy_idx(nx, ny);
            if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                let is_deep = map.tiles[idx].liquid == LiquidType::Water;
                candidates.push((nx, ny, is_deep));
            }
        }

        if candidates.is_empty() {
            // Stuck — stop drifting
            commands.entity(entity).remove::<Drifting>();
        } else {
            let pick = game_rng.0.range(0, candidates.len() as i32) as usize;
            let (nx, ny, is_deep) = candidates[pick];
            pos.x = nx;
            pos.y = ny;
            if !is_deep {
                // Washed ashore
                commands.entity(entity).remove::<Drifting>();
            }
        }
    }
}

/// Extinguish burning status on entities standing in water.
fn water_extinguish_system(
    mut commands: Commands,
    mut turn_end: MessageReader<TurnEndEvent>,
    mut query: Query<(&Position, &mut StatusEffects)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    map: Res<Map>,
    mut gas_tiles: ResMut<crate::game::gas::GasTiles>,
) {
    for _ in turn_end.read() {
        for (pos, mut effects) in query.iter_mut() {
            let idx = map.xy_idx(pos.x, pos.y);
            if idx >= map.tiles.len() {
                continue;
            }
            let liquid = map.tiles[idx].liquid;
            if liquid != LiquidType::Water && liquid != LiquidType::ShallowWater {
                continue;
            }
            let had_burning = effects.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Burning { .. }));
            if had_burning {
                effects.remove_kind(|k| matches!(k, StatusEffectKind::Burning { .. }));
                log_writer.write(GameLogMessage("The water extinguishes the flames!".to_string()));
                // Burning creature extinguished by water → dramatic steam burst
                crate::game::gas::spawn_gas(
                    &mut commands, pos.x, pos.y,
                    crate::game::gas::GasType::Steam,
                    crate::game::gas::MAX_CONCENTRATION,
                    &mut gas_tiles,
                );
            }
        }
    }
}

// --- Water shimmer animation ---

/// Compute the shimmer color for a water tile given its type, light level, light color,
/// elapsed time, and per-tile phase offset.
///
/// Returns an `(r, g, b)` tuple clamped to `[0, 1]`.
pub(crate) fn compute_shimmer_color(
    liquid: LiquidType,
    light: f32,
    light_color: [f32; 3],
    t: f32,
    phase: f32,
) -> (f32, f32, f32) {
    use std::f32::consts::TAU;

    // The sprite texture is already blue — the tint is light modulation, not coloring.
    // Base tint matches update_tile_visibility: (light, light*0.95, light*0.8).
    // Shimmer adds per-channel sine variation at different frequencies for color dancing.
    let variation = match liquid {
        LiquidType::Water => 0.10_f32,
        LiquidType::ShallowWater => 0.05,
        _ => return (light, light * 0.95, light * 0.8),
    };

    // Three sine waves at different frequencies per channel — creates color shifts, not
    // just uniform brightness change. Each channel dances independently.
    let r_wave = (t * 2.0 + phase * TAU).sin();
    let g_wave = (t * 1.7 + phase * TAU + 1.0).sin();
    let b_wave = (t * 1.3 + phase * TAU + 2.0).sin();

    let r = (light * light_color[0] * (1.0 + r_wave * variation)).clamp(0.0, 1.0);
    let g = (light * light_color[1] * 0.95 * (1.0 + g_wave * variation)).clamp(0.0, 1.0);
    let b = (light * light_color[2] * 0.8 * (1.0 + b_wave * variation)).clamp(0.0, 1.0);
    (r, g, b)
}

/// Brogue-style water shimmer animation. Modulates liquid overlay sprite colors
/// using sine waves with per-tile phase offsets for organic ripple patterns.
pub fn animate_water_shimmer(
    time: Res<Time>,
    water_tiles: Res<WaterTiles>,
    tile_index: Res<crate::map::tile::TileEntityIndex>,
    map: Res<Map>,
    light_map: Res<crate::map::light::LightMap>,
    tile_query: Query<
        (&crate::map::tile::TileVisibility, Option<&Children>),
        With<crate::map::tile::TileMarker>,
    >,
    mut liquid_sprite_query: Query<
        &mut Sprite,
        With<crate::game::ascii_mode::LiquidOverlay>,
    >,
) {
    let t = time.elapsed_secs();

    for (&(x, y), &liquid) in water_tiles.0.iter() {
        let Some(&tile_entity) = tile_index.0.get(&(x, y)) else {
            continue;
        };
        let Ok((tile_vis, children)) = tile_query.get(tile_entity) else {
            continue;
        };
        // Only animate visible tiles — explored-but-hidden keeps its dim gray
        if *tile_vis != crate::map::tile::TileVisibility::Visible {
            continue;
        }

        let idx = map.xy_idx(x, y);
        let light = light_map.values.get(idx).copied().unwrap_or(0.0).max(0.55);
        let light_color = light_map.colors.get(idx).copied().unwrap_or([1.0, 1.0, 1.0]);

        let phase = (x as f32 * 1.7 + y as f32 * 2.3).fract();
        let (r, g, b) = compute_shimmer_color(liquid, light, light_color, t, phase);

        let Some(children) = children else { continue };

        for child in children.iter() {
            if let Ok(mut sprite) = liquid_sprite_query.get_mut(child) {
                sprite.color = Color::srgb(r, g, b);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shimmer_color_range() {
        // Verify computed colors stay within [0, 1] for various inputs
        for light in [0.0, 0.15, 0.5, 1.0] {
            for t in [0.0, 1.0, 5.0, 100.0] {
                for phase in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let (r, g, b) = compute_shimmer_color(
                        LiquidType::Water,
                        light,
                        [1.0, 1.0, 1.0],
                        t,
                        phase,
                    );
                    assert!(r >= 0.0 && r <= 1.0, "deep r={r} out of range");
                    assert!(g >= 0.0 && g <= 1.0, "deep g={g} out of range");
                    assert!(b >= 0.0 && b <= 1.0, "deep b={b} out of range");

                    let (r, g, b) = compute_shimmer_color(
                        LiquidType::ShallowWater,
                        light,
                        [1.0, 1.0, 1.0],
                        t,
                        phase,
                    );
                    assert!(r >= 0.0 && r <= 1.0, "shallow r={r} out of range");
                    assert!(g >= 0.0 && g <= 1.0, "shallow g={g} out of range");
                    assert!(b >= 0.0 && b <= 1.0, "shallow b={b} out of range");
                }
            }
        }
    }

    #[test]
    fn test_shimmer_color_range_with_colored_light() {
        // Warm light (fire-like) should still produce in-range values
        let light_color = [1.2, 0.9, 0.6]; // warm tint can exceed 1.0
        for liquid in [LiquidType::Water, LiquidType::ShallowWater] {
            let (r, g, b) = compute_shimmer_color(liquid, 1.0, light_color, 3.14, 0.5);
            assert!(r >= 0.0 && r <= 1.0, "r={r} out of range with colored light");
            assert!(g >= 0.0 && g <= 1.0, "g={g} out of range with colored light");
            assert!(b >= 0.0 && b <= 1.0, "b={b} out of range with colored light");
        }
    }

    #[test]
    fn test_phase_offset_varies_by_position() {
        // Different positions should produce different phase values
        let phase_a = (0.0_f32 * 1.7 + 0.0_f32 * 2.3).fract();
        let phase_b = (1.0_f32 * 1.7 + 0.0_f32 * 2.3).fract();
        let phase_c = (0.0_f32 * 1.7 + 1.0_f32 * 2.3).fract();
        assert_ne!(phase_a, phase_b);
        assert_ne!(phase_a, phase_c);
        assert_ne!(phase_b, phase_c);
    }

    #[test]
    fn test_deep_has_more_variation_than_shallow() {
        // Deep water (variation=0.10) should swing further from baseline than
        // shallow (variation=0.05) over time.
        let light = 0.8;
        let light_color = [1.0, 1.0, 1.0];
        let phase = 0.3;

        let (mut d_min, mut d_max) = (f32::MAX, f32::MIN);
        let (mut s_min, mut s_max) = (f32::MAX, f32::MIN);

        for i in 0..1000 {
            let t = i as f32 * 0.01;
            let (dr, _, _) = compute_shimmer_color(LiquidType::Water, light, light_color, t, phase);
            let (sr, _, _) =
                compute_shimmer_color(LiquidType::ShallowWater, light, light_color, t, phase);
            d_min = d_min.min(dr);
            d_max = d_max.max(dr);
            s_min = s_min.min(sr);
            s_max = s_max.max(sr);
        }
        let deep_range = d_max - d_min;
        let shallow_range = s_max - s_min;

        assert!(
            deep_range > shallow_range,
            "deep range ({deep_range}) should exceed shallow range ({shallow_range})"
        );
    }

    #[test]
    fn test_shimmer_varies_over_time() {
        // The same tile should produce different colors at different times
        let liquid = LiquidType::Water;
        let light = 0.8;
        let light_color = [1.0, 1.0, 1.0];
        let phase = 0.5;

        let (r1, g1, b1) = compute_shimmer_color(liquid, light, light_color, 0.0, phase);
        let (r2, g2, b2) = compute_shimmer_color(liquid, light, light_color, 2.0, phase);

        // At least one channel should differ
        assert!(
            (r1 - r2).abs() > 0.001 || (g1 - g2).abs() > 0.001 || (b1 - b2).abs() > 0.001,
            "color should change over time"
        );
    }

}
