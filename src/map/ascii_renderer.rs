//! Unified ASCII tile renderer — the sole writer to tile ASCII components.
//!
//! Replaces: animate_fire_backgrounds, animate_gas_backgrounds,
//! animate_water_shimmer (ASCII branch), update_tile_visibility (ASCII path),
//! apply_tile_mutations glyph updates, apply_decoration_mutations glyph/bg updates.

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::assets::{TileManifest, TileManifestHandle};
use crate::components::{Position, Viewshed};
use crate::game::ascii_mode::{AsciiBackground, AsciiGlyph, AsciiGlyphColor, LiquidOverlay};
use crate::game::fire::FireTiles;
use crate::game::gas::GasTiles;
use crate::game::systems::Omniscient;
use crate::game::water::WaterTiles;
use crate::map::light::LightMap;
use crate::map::map::Map;
use crate::map::tile::{
    LiquidType, TileExplored, TileMarker, TileVisibility, resolve_tile_bg, resolve_tile_display,
};
use crate::player::Player;
use bracket_lib::prelude::Algorithm2D;

/// Compute fire glyph foreground and background colors from sine-wave animation.
/// Returns `(fg_color, bg_color)`. Fire is self-luminous — no lighting tint needed.
pub fn compute_fire_colors(t: f32, phase: f32) -> (Color, Color) {
    let wave1 = (t * 1.2 + phase * 6.28).sin() * 0.5 + 0.5;
    let wave2 = (t * 0.7 + phase * 3.14).sin() * 0.5 + 0.5;
    let blend = wave1 * 0.6 + wave2 * 0.4;

    let fg = Color::srgb(
        0.85 + blend * 0.15,
        0.35 + blend * 0.25,
        0.05 + blend * 0.05,
    );
    let bg = Color::srgb(
        0.35 + blend * 0.25,
        0.08 + blend * 0.12,
        0.02,
    );
    (fg, bg)
}

/// Compute gas background color from type, concentration, and sine-wave animation.
/// Gas backgrounds are self-luminous — no lighting tint needed.
pub fn compute_gas_bg(
    gas_type: crate::game::gas::GasType,
    concentration: u8,
    t: f32,
    phase: f32,
) -> Color {
    let wave = (t * 0.6 + phase * 6.28).sin() * 0.5 + 0.5;
    let intensity = match concentration {
        3 => 0.6 + wave * 0.15,
        2 => 0.35 + wave * 0.15,
        _ => 0.15 + wave * 0.10,
    };
    let [br, bg, bb] = gas_type.ascii_bg_color();
    Color::srgb(br * intensity, bg * intensity, bb * intensity)
}

/// Add light tint to a base color. `light_amount` is 0.0 (ambient) to 1.0 (bright).
pub fn apply_light_to_color(base: Color, light_amount: f32, tint: [f32; 3]) -> Color {
    let srgba = base.to_srgba();
    let r = (srgba.red + light_amount * tint[0] * 0.2).min(1.0);
    let g = (srgba.green + light_amount * tint[1] * 0.2).min(1.0);
    let b = (srgba.blue + light_amount * tint[2] * 0.2).min(1.0);
    Color::srgba(r, g, b, srgba.alpha)
}

/// Dim a color by a multiplicative factor (0.0 = black, 1.0 = unchanged).
pub fn dim_color(base: Color, factor: f32) -> Color {
    let srgba = base.to_srgba();
    Color::srgba(
        srgba.red * factor,
        srgba.green * factor,
        srgba.blue * factor,
        srgba.alpha,
    )
}

/// Minimum brightness for tiles currently in the player's FOV but not near a candle.
const AMBIENT: f32 = 0.55;

/// Unified ASCII tile renderer. Runs every frame in ASCII mode.
///
/// For each tile entity, resolves what to display using a priority cascade
/// (fire > gas > water > base), then writes glyph char, fg color, and bg color
/// to the tile's children exactly once per tile.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn render_tile_ascii(
    time: Res<Time>,
    player_query: Query<&Viewshed, With<Player>>,
    map: Res<Map>,
    light_map: Res<LightMap>,
    fire_tiles: Res<FireTiles>,
    gas_tiles: Res<GasTiles>,
    water_tiles: Res<WaterTiles>,
    omniscient: Res<Omniscient>,
    tile_manifests: Res<Assets<TileManifest>>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_query: Query<
        (&Position, &TileVisibility, &TileExplored, Option<&Children>),
        With<TileMarker>,
    >,
    mut glyph_query: Query<
        (&mut Text2d, &mut TextColor, &mut AsciiGlyphColor),
        With<AsciiGlyph>,
    >,
    mut bg_query: Query<(&mut Sprite, &mut AsciiBackground), Without<LiquidOverlay>>,
) {
    let Ok(player_viewshed) = player_query.single() else {
        return;
    };
    let Some(manifest) = tile_manifests.get(&tile_manifest_handle.0) else {
        return;
    };
    let fov_tiles = &player_viewshed.visible_tiles;
    let omni = omniscient.0;
    let t = time.elapsed_secs();

    for (tile_pos, tile_vis, tile_explored, children) in tile_query.iter() {
        let x = tile_pos.x;
        let y = tile_pos.y;
        let phase = (x as f32 * 1.7 + y as f32 * 2.3).fract();

        let Some(children) = children else {
            continue;
        };

        // Unexplored tiles: skip entirely.
        if *tile_explored != TileExplored::Explored {
            continue;
        }

        let current_point = bracket_lib::prelude::Point::new(x, y);
        let in_fov = omni || fov_tiles.contains(&current_point);

        let idx = if map.in_bounds(current_point) {
            map.xy_idx(x, y)
        } else {
            continue;
        };
        let tile = if idx < map.tiles.len() {
            map.tiles[idx]
        } else {
            continue;
        };

        let (glyph_char, fg_color, bg_color);

        if in_fov && *tile_vis == TileVisibility::Visible {
            // --- Priority cascade for visible tiles ---
            let is_fire = fire_tiles.0.contains(&(x, y));
            let gas_data = gas_tiles.0.get(&(x, y));
            let water_data = water_tiles.0.get(&(x, y));

            if is_fire {
                // 1. Fire: self-luminous, special glyph
                glyph_char = "^".to_string();
                let (fg, bg) = compute_fire_colors(t, phase);
                fg_color = fg;
                bg_color = bg;
            } else if let Some(gas) = gas_data {
                // 2. Gas: keep base glyph, apply lighting to fg, gas bg is self-luminous
                let (base_char, base_fg, _) = resolve_tile_display(tile, manifest);
                glyph_char = base_char;

                let (light, light_color) = get_light(idx, &light_map);
                let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
                fg_color = apply_light_to_color(base_fg, light_amount, light_color);
                bg_color = compute_gas_bg(gas.gas_type, gas.concentration, t, phase);
            } else if let Some(liquid) = water_data {
                // 3. Water: shimmer with per-channel sine waves
                let (base_char, _, _) = resolve_tile_display(tile, manifest);
                glyph_char = base_char;

                let (light, light_color) = get_light(idx, &light_map);

                let (fg_base, bg_base, variation) = match liquid {
                    LiquidType::Water => (
                        [0.50, 0.50, 1.0],
                        [0.37, 0.37, 0.79],
                        0.10,
                    ),
                    _ => (
                        [0.63, 0.75, 1.0],
                        [0.44, 0.63, 0.93],
                        0.05,
                    ),
                };

                let r_wave = (t * 2.0 + phase * TAU).sin();
                let g_wave = (t * 1.7 + phase * TAU + 1.0).sin();
                let b_wave = (t * 1.3 + phase * TAU + 2.0).sin();

                fn shimmer(base: [f32; 3], light: f32, light_color: [f32; 3], waves: [f32; 3], variation: f32) -> Color {
                    let r = (base[0] * light * light_color[0] * (1.0 + waves[0] * variation)).clamp(0.0, 1.0);
                    let g = (base[1] * light * light_color[1] * (1.0 + waves[1] * variation)).clamp(0.0, 1.0);
                    let b = (base[2] * light * light_color[2] * (1.0 + waves[2] * variation)).clamp(0.0, 1.0);
                    Color::srgb(r, g, b)
                }

                let waves = [r_wave, g_wave, b_wave];
                fg_color = shimmer(fg_base, light, light_color, waves, variation);
                bg_color = shimmer(bg_base, light, light_color, waves, variation);
            } else {
                // 4. Base: normal tile with lighting
                let (base_char, base_fg, _) = resolve_tile_display(tile, manifest);
                let base_bg = resolve_tile_bg(tile, manifest);
                glyph_char = base_char;

                let (light, light_color) = get_light(idx, &light_map);
                let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
                fg_color = apply_light_to_color(base_fg, light_amount, light_color);
                bg_color = apply_light_to_color(base_bg, light_amount, light_color);
            }
        } else {
            // Explored but not visible: dim base glyph
            let (base_char, base_fg, _) = resolve_tile_display(tile, manifest);
            let base_bg = resolve_tile_bg(tile, manifest);
            glyph_char = base_char;
            fg_color = dim_color(base_fg, 0.45);
            bg_color = dim_color(base_bg, 0.35);
        }

        // Write to children
        for child in children.iter() {
            if let Ok((mut text, mut text_color, mut glyph_base)) = glyph_query.get_mut(child) {
                **text = glyph_char.clone();
                text_color.0 = fg_color;
                glyph_base.0 = fg_color;
            }
            if let Ok((mut sprite, mut bg)) = bg_query.get_mut(child) {
                sprite.color = bg_color;
                bg.base_color = bg_color;
            }
        }
    }
}

/// Look up lighting for a tile index. Returns (light_level, light_color).
fn get_light(idx: usize, light_map: &LightMap) -> (f32, [f32; 3]) {
    let light = light_map.values.get(idx).copied().unwrap_or(0.0).max(AMBIENT);
    let color = light_map.colors.get(idx).copied().unwrap_or([1.0, 1.0, 1.0]);
    (light, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_colors_in_range() {
        for i in 0..100 {
            let t = i as f32 * 0.1;
            let phase = (i as f32 * 0.37).fract();
            let (fg, bg) = compute_fire_colors(t, phase);
            let fg_s = fg.to_srgba();
            let bg_s = bg.to_srgba();
            assert!(fg_s.red >= 0.0 && fg_s.red <= 1.0);
            assert!(fg_s.green >= 0.0 && fg_s.green <= 1.0);
            assert!(fg_s.blue >= 0.0 && fg_s.blue <= 1.0);
            assert!(bg_s.red >= 0.0 && bg_s.red <= 1.0);
            assert!(bg_s.green >= 0.0 && bg_s.green <= 1.0);
            assert!(bg_s.blue >= 0.0 && bg_s.blue <= 1.0);
        }
    }

    #[test]
    fn fire_colors_vary_over_time() {
        let (fg1, _) = compute_fire_colors(0.0, 0.5);
        let (fg2, _) = compute_fire_colors(2.0, 0.5);
        let s1 = fg1.to_srgba();
        let s2 = fg2.to_srgba();
        assert!(
            (s1.red - s2.red).abs() > 0.001
                || (s1.green - s2.green).abs() > 0.001
                || (s1.blue - s2.blue).abs() > 0.001,
        );
    }

    #[test]
    fn gas_bg_scales_with_concentration() {
        let low = compute_gas_bg(crate::game::gas::GasType::Poison, 1, 0.0, 0.0);
        let high = compute_gas_bg(crate::game::gas::GasType::Poison, 3, 0.0, 0.0);
        let low_s = low.to_srgba();
        let high_s = high.to_srgba();
        assert!(high_s.green > low_s.green, "higher concentration should be brighter");
    }

    #[test]
    fn gas_bg_in_range() {
        for conc in 1..=3 {
            for gas in [crate::game::gas::GasType::Poison, crate::game::gas::GasType::Steam] {
                let c = compute_gas_bg(gas, conc, 3.14, 0.7);
                let s = c.to_srgba();
                assert!(s.red >= 0.0 && s.red <= 1.0);
                assert!(s.green >= 0.0 && s.green <= 1.0);
                assert!(s.blue >= 0.0 && s.blue <= 1.0);
            }
        }
    }

    #[test]
    fn dim_color_reduces_brightness() {
        let base = Color::srgb(0.8, 0.6, 0.4);
        let dimmed = dim_color(base, 0.5);
        let s = dimmed.to_srgba();
        assert!((s.red - 0.4).abs() < 0.01);
        assert!((s.green - 0.3).abs() < 0.01);
        assert!((s.blue - 0.2).abs() < 0.01);
    }

    #[test]
    fn apply_light_brightens() {
        let base = Color::srgb(0.3, 0.3, 0.3);
        let lit = apply_light_to_color(base, 1.0, [1.0, 1.0, 1.0]);
        let s = lit.to_srgba();
        assert!(s.red > 0.3);
        assert!(s.green > 0.3);
        assert!(s.blue > 0.3);
    }
}
