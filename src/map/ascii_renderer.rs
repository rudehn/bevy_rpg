//! Unified ASCII tile renderer — the sole writer to tile ASCII components.
//!
//! Handles: fire/gas/water background animation, tile visibility coloring,
//! entity glyph overlay, and lighting tint for all tiles.

use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::assets::{TileManifest, TileManifestHandle};
use crate::components::{InInventory, Item, Monster, Position, Prop, Submerged, Viewshed};
use crate::game::ascii_mode::AsciiDisplay;
use crate::map::tile::TileEntityIndex;
use crate::game::ascii_mode::{AsciiBackground, AsciiGlyph, AsciiGlyphColor};
use crate::game::fire::FireTiles;
use crate::game::gas::GasTiles;
use crate::game::magic::{GameStatusEffectsExt, StatusEffects};
use crate::game::systems::Omniscient;
use crate::game::water::WaterTiles;
use crate::map::light::LightMap;
use crate::map::map::Map;
use crate::map::tile::{LiquidType, TileExplored, TileMarker, TileVisibility};
use crate::map::world::{FloorTheme, themed_tile_bg, themed_tile_display};
use crate::player::Player;
use bracket_lib::prelude::Algorithm2D;

/// Compute fire glyph foreground and background colors from sine-wave animation.
/// Returns `(fg_color, bg_color)`. Fire is self-luminous — no lighting tint needed.
pub fn compute_fire_colors(t: f32, phase: f32) -> (Color, Color) {
    use std::f32::consts::{PI, TAU};
    let wave1 = (t * 1.2 + phase * TAU).sin() * 0.5 + 0.5;
    let wave2 = (t * 0.7 + phase * PI).sin() * 0.5 + 0.5;
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
    concentration: u16,
    t: f32,
    phase: f32,
) -> Color {
    let wave = (t * 0.6 + phase * std::f32::consts::TAU).sin() * 0.5 + 0.5;
    // Scale intensity: 0 at concentration 0, ~0.75 at concentration 200+
    let base_intensity = (concentration as f32 / 200.0).clamp(0.0, 0.75);
    let intensity = base_intensity + wave * 0.1;
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

/// Pool of crack-suggesting glyphs, Brogue-style: a small asymmetric set so
/// neighboring cracked tiles don't look like clones of each other. Selection
/// is deterministic per tile (via `phase`) so the pattern doesn't flicker.
const CRACKED_GLYPHS: &[&str] = &["x", ",", "`", "/", "\\"];

/// Pick a crack glyph for a given position hash (`phase` ∈ [0, 1)).
pub fn cracked_glyph(phase: f32) -> &'static str {
    let i = (phase * CRACKED_GLYPHS.len() as f32) as usize;
    CRACKED_GLYPHS[i.min(CRACKED_GLYPHS.len() - 1)]
}

/// Cracked floor foreground — monochromatic warm pulse (rust → amber).
/// A single-hue breathing effect reads as "stressed stone" far better than
/// the old amber/magenta/teal cycle, which flickered like a hazard sign.
pub fn compute_cracked_fg(t: f32, phase: f32) -> Color {
    let pulse = (t * 2.0 + phase * TAU).sin() * 0.5 + 0.5; // [0, 1]
    let shimmer = (t * 4.5 + phase * 3.0).sin() * 0.05;    // [-0.05, 0.05]
    Color::srgb(
        (0.70 + pulse * 0.18 + shimmer).clamp(0.0, 1.0), // R ~0.65–0.93
        (0.38 + pulse * 0.10).clamp(0.0, 1.0),           // G ~0.38–0.48
        (0.15 + shimmer).clamp(0.0, 1.0),                // B ~0.10–0.20
    )
}

/// Cracked floor background — unsettled, per-tile rust-toned color with a
/// slow breathing pulse and occasional "warning flares" that briefly brighten
/// a tile as if something is shifting underneath it.
///
/// Three layered sources of variation:
/// 1. **Per-tile hue jitter** keyed off `phase` so each cracked tile has its
///    own base tint (redder / browner / darker) — kills the uniform look.
/// 2. **Slow pulse** (~1.3 Hz, staggered per tile) — the field never fully
///    settles; no two neighbors breathe in unison.
/// 3. **Rare flares** — each tile has its own schedule; brief brightening
///    that reads as "this one's about to go."
pub fn compute_cracked_bg(t: f32, phase: f32) -> Color {
    // Per-tile hue variation: ±0.05 in red, small jitter in green/blue.
    let variant = (phase * TAU).sin();        // [-1, 1]
    let r_variant = variant * 0.05;
    let g_variant = (phase * 3.7).sin() * 0.03;
    let b_variant = -variant.abs() * 0.02;    // deeper shadow when hue shifts

    // Slow breathing pulse, phase-staggered.
    let pulse = (t * 1.3 + phase * TAU).sin() * 0.5 + 0.5; // [0, 1]
    let pulse_brightness = pulse * 0.06;

    // Occasional per-tile warning flare — threshold on a low-frequency sine
    // with a big phase multiplier so each tile flares on its own schedule.
    let flare_wave = (t * 0.6 + phase * 17.0).sin();
    let flare = ((flare_wave - 0.85) / 0.15).max(0.0); // 0 most of the time, up to 1 in bursts

    Color::srgb(
        (0.26 + r_variant + pulse_brightness + flare * 0.22).clamp(0.0, 1.0),
        (0.14 + g_variant + pulse_brightness * 0.6 + flare * 0.08).clamp(0.0, 1.0),
        (0.10 + b_variant + pulse_brightness * 0.3).clamp(0.0, 1.0),
    )
}

/// Minimum brightness for tiles currently in the player's FOV but not near a candle.
const AMBIENT: f32 = 0.55;

/// Resolve the background color for a visible cell, applying tile effect cascade
/// (fire glow > gas blend > water shimmer > lit base).
fn resolve_cell_bg(
    tile: crate::map::tile::Tile,
    manifest: &TileManifest,
    idx: usize,
    light_map: &LightMap,
    fire_tiles: &FireTiles,
    gas_tiles: &GasTiles,
    water_tiles: &WaterTiles,
    x: i32, y: i32,
    t: f32,
    phase: f32,
    theme: FloorTheme,
) -> Color {
    if fire_tiles.0.contains(&(x, y)) {
        let (_, fire_bg) = compute_fire_colors(t, phase);
        return fire_bg;
    }
    if let Some(gas) = gas_tiles.0.get(&(x, y)) {
        let (light, light_color) = get_light(idx, light_map);
        let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
        let base_bg = apply_light_to_color(themed_tile_bg(tile, manifest, theme), light_amount, light_color);
        let gas_bg = compute_gas_bg(gas.gas_type, gas.concentration, t, phase);
        let alpha = (gas.concentration as f32 / 300.0).clamp(0.2, 0.85);
        let g = gas_bg.to_srgba();
        let b = base_bg.to_srgba();
        return Color::srgb(
            g.red * alpha + b.red * (1.0 - alpha),
            g.green * alpha + b.green * (1.0 - alpha),
            g.blue * alpha + b.blue * (1.0 - alpha),
        );
    }
    if let Some(liquid) = water_tiles.0.get(&(x, y)) {
        let (light, _) = get_light(idx, light_map);
        let variation = if *liquid == LiquidType::Water { 0.10 } else { 0.05 };
        let bg_base = match liquid {
            LiquidType::Water => [0.37_f32, 0.37, 0.79],
            _ => [0.44_f32, 0.63, 0.93],
        };
        let r_wave = (t * 2.0 + phase * TAU).sin();
        let g_wave = (t * 1.7 + phase * TAU + 1.0).sin();
        let b_wave = (t * 1.3 + phase * TAU + 2.0).sin();
        return Color::srgb(
            (bg_base[0] * light * (1.0 + r_wave * variation)).clamp(0.0, 1.0),
            (bg_base[1] * light * (1.0 + g_wave * variation)).clamp(0.0, 1.0),
            (bg_base[2] * light * (1.0 + b_wave * variation)).clamp(0.0, 1.0),
        );
    }
    // Cracked floor: unsettled per-tile warm background with breathing pulse
    // and occasional warning flares (see compute_cracked_bg).
    if tile.decoration == crate::map::tile::Decoration::CrackedFloor {
        let (light, light_color) = get_light(idx, light_map);
        let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
        return apply_light_to_color(compute_cracked_bg(t, phase), light_amount, light_color);
    }
    // Normal lit base bg
    let (light, light_color) = get_light(idx, light_map);
    let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
    apply_light_to_color(themed_tile_bg(tile, manifest, theme), light_amount, light_color)
}

struct CellEntity {
    glyph: String,
    color: Color,
}

/// Bundled entity queries for spatial lookup in the ASCII tile renderer.
#[derive(bevy::ecs::system::SystemParam)]
pub struct EntityCellQueries<'w, 's> {
    player_pos: Query<'w, 's, (&'static Position, &'static AsciiDisplay), With<Player>>,
    monsters: Query<
        'w,
        's,
        (&'static Position, &'static AsciiDisplay, Option<&'static StatusEffects>, Has<Submerged>),
        With<Monster>,
    >,
    items: Query<'w, 's, (&'static Position, &'static AsciiDisplay), (With<Item>, Without<InInventory>)>,
    props: Query<'w, 's, (&'static Position, &'static AsciiDisplay), With<Prop>>,
}

/// Bundled tile-effect resources (fire/gas/water/lighting/omniscience/theme).
/// Exists to keep `render_tile_ascii` under Bevy's 16-param system limit.
#[derive(bevy::ecs::system::SystemParam)]
pub struct TileEffectRes<'w> {
    pub light_map: Res<'w, LightMap>,
    pub fire_tiles: Res<'w, FireTiles>,
    pub gas_tiles: Res<'w, GasTiles>,
    pub water_tiles: Res<'w, WaterTiles>,
    pub omniscient: Res<'w, Omniscient>,
    pub floor_theme: Res<'w, FloorTheme>,
}

/// Unified ASCII tile renderer. Runs every frame in ASCII mode.
///
/// For each tile entity, resolves what to display using a priority cascade
/// (fire > gas > water > base), then writes glyph char, fg color, and bg color
/// to the tile's children exactly once per tile.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn render_tile_ascii(
    time: Res<Time>,
    player_query: Query<&Viewshed, With<Player>>,
    viewshed_changed: Query<(), (With<Player>, Changed<Viewshed>)>,
    map: Res<Map>,
    effects: TileEffectRes,
    tile_manifests: Res<Assets<TileManifest>>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_index: Res<TileEntityIndex>,
    tile_query: Query<
        (&TileVisibility, &TileExplored, Option<&Children>),
        With<TileMarker>,
    >,
    mut glyph_query: Query<
        (&mut Text2d, &mut TextColor, &mut AsciiGlyphColor),
        With<AsciiGlyph>,
    >,
    mut bg_query: Query<(&mut Sprite, &mut AsciiBackground)>,
    entity_queries: EntityCellQueries,
) {
    let Ok(player_viewshed) = player_query.single() else {
        return;
    };
    let Some(manifest) = tile_manifests.get(&tile_manifest_handle.0) else {
        return;
    };
    let light_map = &effects.light_map;
    let fire_tiles = &effects.fire_tiles;
    let gas_tiles = &effects.gas_tiles;
    let water_tiles = &effects.water_tiles;
    let omniscient = &effects.omniscient;
    let fov_tiles = &player_viewshed.visible_tiles;
    let omni = omniscient.0;
    let t = time.elapsed_secs();
    let theme = *effects.floor_theme;
    let fov_changed = !viewshed_changed.is_empty()
        || fire_tiles.is_changed()
        || gas_tiles.is_changed()
        || omniscient.is_changed();

    // Build per-cell entity lookup. Priority: Player > Monster > Item > Prop.
    // Insert lowest priority first — higher priority overwrites.
    let mut cell_entities: HashMap<(i32, i32), CellEntity> = HashMap::new();

    // Props (lowest priority — inserted first, overwritten by higher)
    for (pos, display) in entity_queries.props.iter() {
        let pt = bracket_lib::prelude::Point::new(pos.x, pos.y);
        if !(omni || fov_tiles.contains(&pt)) { continue; }
        cell_entities.insert((pos.x, pos.y), CellEntity { glyph: display.ch.clone(), color: display.color });
    }
    // Items (above props)
    for (pos, display) in entity_queries.items.iter() {
        let pt = bracket_lib::prelude::Point::new(pos.x, pos.y);
        if !(omni || fov_tiles.contains(&pt)) { continue; }
        cell_entities.insert((pos.x, pos.y), CellEntity { glyph: display.ch.clone(), color: display.color });
    }
    // Monsters (above items — stunned get yellow tint, submerged hidden)
    for (pos, display, effects, is_submerged) in entity_queries.monsters.iter() {
        if is_submerged { continue; }
        let pt = bracket_lib::prelude::Point::new(pos.x, pos.y);
        if !(omni || fov_tiles.contains(&pt)) { continue; }
        let color = if effects.map(|e| e.is_stunned()).unwrap_or(false) {
            Color::srgba(1.0, 1.0, 0.3, 1.0)
        } else {
            display.color
        };
        cell_entities.insert((pos.x, pos.y), CellEntity { glyph: display.ch.clone(), color });
    }
    // Player (highest priority) — still rendered via own AsciiGlyph child for camera sync,
    // but also added to cell_entities so the tile background reflects their position.
    if let Ok((pos, display)) = entity_queries.player_pos.single() {
        cell_entities.insert((pos.x, pos.y), CellEntity { glyph: display.ch.clone(), color: display.color });
    }

    // Collect positions to process: FOV tiles (always) + explored tiles (only on change).
    // This avoids iterating all 4800 tile entities every frame.
    let mut positions_to_process: Vec<(i32, i32)> = Vec::new();

    if omni {
        // Omniscient: process all tiles, but only on change
        if !fov_changed { return; }
        for y in 0..map.height {
            for x in 0..map.width {
                positions_to_process.push((x, y));
            }
        }
    } else {
        // Always process FOV tiles (animated effects need per-frame updates)
        for pt in fov_tiles.iter() {
            positions_to_process.push((pt.x, pt.y));
        }
        // On viewshed change, also process explored-but-not-visible tiles
        // (they need dimming when they leave FOV)
        if fov_changed {
            for y in 0..map.height {
                for x in 0..map.width {
                    let idx = map.xy_idx(x, y);
                    if idx < map.explored_tiles.len()
                        && map.explored_tiles[idx]
                        && !fov_tiles.contains(&bracket_lib::prelude::Point::new(x, y))
                    {
                        positions_to_process.push((x, y));
                    }
                }
            }
        }
    }

    for (x, y) in &positions_to_process {
        let x = *x;
        let y = *y;
        let Some(&tile_entity) = tile_index.0.get(&(x, y)) else {
            continue;
        };
        let Ok((tile_vis, tile_explored, children)) = tile_query.get(tile_entity) else {
            continue;
        };
        let Some(children) = children else {
            continue;
        };
        if *tile_explored != TileExplored::Explored {
            continue;
        }

        let phase = (x as f32 * 1.7 + y as f32 * 2.3).fract();
        let in_fov = omni || fov_tiles.contains(&bracket_lib::prelude::Point::new(x, y));

        let idx = map.xy_idx(x, y);
        if idx >= map.tiles.len() {
            continue;
        }
        let tile = map.tiles[idx];

        let (glyph_char, fg_color, bg_color);

        if in_fov && *tile_vis == TileVisibility::Visible {
            // --- Priority cascade for visible tiles ---
            let is_fire = fire_tiles.0.contains(&(x, y));
            let gas_data = gas_tiles.0.get(&(x, y));
            let water_data = water_tiles.0.get(&(x, y));

            if let Some(entity_cell) = cell_entities.get(&(x, y)) {
                // Entity overrides tile glyph; background reflects tile effects
                glyph_char = entity_cell.glyph.clone();
                fg_color = entity_cell.color;
                bg_color = resolve_cell_bg(
                    tile, manifest, idx, &light_map, &fire_tiles, &gas_tiles,
                    &water_tiles, x, y, t, phase, theme,
                );
            } else if is_fire {
                // 1. Fire: self-luminous, special glyph
                glyph_char = "^".to_string();
                let (fg, bg) = compute_fire_colors(t, phase);
                fg_color = fg;
                bg_color = bg;
            } else if let Some(gas) = gas_data {
                // 2. Gas: keep base glyph, apply lighting to fg, gas bg is self-luminous.
                // Blend gas bg over the tile's base bg so thin gas doesn't black out the tile.
                let (base_char, base_fg, _) = themed_tile_display(tile, manifest, theme);
                glyph_char = base_char;

                let (light, light_color) = get_light(idx, &light_map);
                let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
                fg_color = apply_light_to_color(base_fg, light_amount, light_color);

                let gas_bg = compute_gas_bg(gas.gas_type, gas.concentration, t, phase);
                let base_bg = apply_light_to_color(
                    themed_tile_bg(tile, manifest, theme), light_amount, light_color,
                );
                // Alpha blend: thicker gas covers more of the base
                let alpha = (gas.concentration as f32 / 300.0).clamp(0.2, 0.85);
                let gas_s = gas_bg.to_srgba();
                let base_s = base_bg.to_srgba();
                bg_color = Color::srgb(
                    gas_s.red * alpha + base_s.red * (1.0 - alpha),
                    gas_s.green * alpha + base_s.green * (1.0 - alpha),
                    gas_s.blue * alpha + base_s.blue * (1.0 - alpha),
                );
            } else if let Some(liquid) = water_data {
                // 3. Water: shimmer with per-channel sine waves
                let (base_char, _, _) = themed_tile_display(tile, manifest, theme);
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

                // Water uses light level only (not warm light_color tint) to stay blue.
                fn shimmer(base: [f32; 3], light: f32, waves: [f32; 3], variation: f32) -> Color {
                    let r = (base[0] * light * (1.0 + waves[0] * variation)).clamp(0.0, 1.0);
                    let g = (base[1] * light * (1.0 + waves[1] * variation)).clamp(0.0, 1.0);
                    let b = (base[2] * light * (1.0 + waves[2] * variation)).clamp(0.0, 1.0);
                    Color::srgb(r, g, b)
                }

                let waves = [r_wave, g_wave, b_wave];
                fg_color = shimmer(fg_base, light, waves, variation);
                bg_color = shimmer(bg_base, light, waves, variation);
            } else if tile.decoration == crate::map::tile::Decoration::CrackedFloor {
                // 4. Cracked floor: per-tile crack glyph + warm amber pulse on
                // an unsettled, per-tile background that occasionally flares.
                glyph_char = cracked_glyph(phase).to_string();
                fg_color = compute_cracked_fg(t, phase);
                let (light, light_color) = get_light(idx, &light_map);
                let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
                bg_color = apply_light_to_color(
                    compute_cracked_bg(t, phase),
                    light_amount,
                    light_color,
                );
            } else {
                // 5. Base: normal tile with lighting
                let (base_char, base_fg, _) = themed_tile_display(tile, manifest, theme);
                let base_bg = themed_tile_bg(tile, manifest, theme);
                glyph_char = base_char;

                let (light, light_color) = get_light(idx, &light_map);
                let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
                fg_color = apply_light_to_color(base_fg, light_amount, light_color);
                bg_color = apply_light_to_color(base_bg, light_amount, light_color);
            }
        } else {
            // Explored but not visible: dim base glyph
            let (base_char, base_fg, _) = themed_tile_display(tile, manifest, theme);
            let base_bg = themed_tile_bg(tile, manifest, theme);
            glyph_char = base_char;
            fg_color = dim_color(base_fg, 0.45);
            bg_color = dim_color(base_bg, 0.35);
        }

        // Write to children — conditional to avoid triggering Bevy change detection
        for child in children.iter() {
            if let Ok((mut text, mut text_color, mut glyph_base)) = glyph_query.get_mut(child) {
                if text.as_str() != glyph_char {
                    **text = glyph_char.clone();
                }
                if text_color.0 != fg_color {
                    text_color.0 = fg_color;
                }
                if glyph_base.0 != fg_color {
                    glyph_base.0 = fg_color;
                }
            }
            if let Ok((mut sprite, mut bg)) = bg_query.get_mut(child) {
                if sprite.color != bg_color {
                    sprite.color = bg_color;
                }
                if bg.base_color != bg_color {
                    bg.base_color = bg_color;
                }
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
        let low = compute_gas_bg(crate::game::gas::GasType::Poison, 10, 0.0, 0.0);
        let high = compute_gas_bg(crate::game::gas::GasType::Poison, 300, 0.0, 0.0);
        let low_s = low.to_srgba();
        let high_s = high.to_srgba();
        assert!(high_s.green > low_s.green, "higher concentration should be brighter");
    }

    #[test]
    fn gas_bg_in_range() {
        for conc in [1_u16, 50, 100, 200, 500] {
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

    #[test]
    fn cracked_glyph_is_deterministic_per_phase() {
        // Same phase ⇒ same glyph (no flicker across frames).
        for i in 0..20 {
            let phase = (i as f32 * 0.137).fract();
            assert_eq!(cracked_glyph(phase), cracked_glyph(phase));
        }
    }

    #[test]
    fn cracked_glyph_uses_full_pool() {
        // Sweeping phase through [0, 1) should hit every glyph in the pool.
        let mut seen: std::collections::HashSet<&'static str> = Default::default();
        for i in 0..100 {
            let phase = i as f32 / 100.0;
            seen.insert(cracked_glyph(phase));
        }
        assert_eq!(seen.len(), CRACKED_GLYPHS.len());
    }

    #[test]
    fn cracked_glyph_is_not_capital_x() {
        // Guard against regressing to the old single-glyph "X" rendering.
        for i in 0..20 {
            let phase = (i as f32 * 0.137).fract();
            assert_ne!(cracked_glyph(phase), "X");
        }
    }

    #[test]
    fn cracked_fg_is_warm_amber() {
        // Invariant: red dominates, blue stays low — no more magenta/teal flashes.
        for i in 0..50 {
            let t = i as f32 * 0.13;
            let phase = (i as f32 * 0.37).fract();
            let c = compute_cracked_fg(t, phase).to_srgba();
            assert!(c.red > c.green, "red should dominate at t={t} phase={phase}");
            assert!(c.green > c.blue, "green should exceed blue at t={t} phase={phase}");
            assert!(c.red >= 0.0 && c.red <= 1.0);
            assert!(c.green >= 0.0 && c.green <= 1.0);
            assert!(c.blue >= 0.0 && c.blue <= 1.0);
        }
    }

    #[test]
    fn cracked_bg_stays_warm_and_in_range() {
        // Background must remain warm (R > B) and inside [0, 1] in every state,
        // including during flare bursts.
        for i in 0..200 {
            let t = i as f32 * 0.11;
            let phase = (i as f32 * 0.37).fract();
            let c = compute_cracked_bg(t, phase).to_srgba();
            assert!(c.red >= 0.0 && c.red <= 1.0);
            assert!(c.green >= 0.0 && c.green <= 1.0);
            assert!(c.blue >= 0.0 && c.blue <= 1.0);
            assert!(c.red > c.blue, "warm bias (R>B) should hold at t={t} phase={phase}");
        }
    }

    #[test]
    fn cracked_bg_varies_across_tiles() {
        // Two different tiles at the same instant should not render the same
        // background — that was the whole complaint with the old flat color.
        let t = 1.0;
        let a = compute_cracked_bg(t, 0.10).to_srgba();
        let b = compute_cracked_bg(t, 0.63).to_srgba();
        let dr = (a.red - b.red).abs();
        let dg = (a.green - b.green).abs();
        let db = (a.blue - b.blue).abs();
        assert!(
            dr > 0.005 || dg > 0.005 || db > 0.005,
            "per-tile bg should differ: a={a:?} b={b:?}"
        );
    }

    #[test]
    fn cracked_bg_flare_brightens_tile() {
        // A flare on a tile (sampled at the phase/t where flare saturates)
        // must produce a measurably brighter red than the tile's non-flare state.
        // flare_wave = sin(t * 0.6 + phase * 17.0); pick t so flare_wave ≈ 1.
        let phase = 0.0_f32;
        // sin(x) = 1 at x = π/2 → t = (π/2) / 0.6
        let flare_t = std::f32::consts::FRAC_PI_2 / 0.6;
        let quiet_t = 0.0;
        let flare_r = compute_cracked_bg(flare_t, phase).to_srgba().red;
        let quiet_r = compute_cracked_bg(quiet_t, phase).to_srgba().red;
        assert!(
            flare_r > quiet_r + 0.10,
            "flare peak should add noticeable red: quiet={quiet_r} flare={flare_r}"
        );
    }
}
