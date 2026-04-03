//! Unified ASCII tile renderer — the sole writer to tile ASCII components.
//!
//! Replaces: animate_fire_backgrounds, animate_gas_backgrounds,
//! animate_water_shimmer (ASCII branch), update_tile_visibility (ASCII path),
//! apply_tile_mutations glyph updates, apply_decoration_mutations glyph/bg updates.

use bevy::prelude::*;

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
