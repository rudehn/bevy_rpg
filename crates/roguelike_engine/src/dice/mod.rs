//! Dice notation helpers built on top of bracket-lib's parser.
//!
//! bracket-lib already provides `parse_dice_string` and
//! `RandomNumberGenerator::roll_dice`. The helpers here are thin
//! convenience wrappers that:
//!
//! - Combine parse + roll with a sensible fallback on parse errors
//!   ([`roll_dice_string`]), so caller code is a single function call
//!   instead of a match-on-Result.
//! - Compute the mathematical expected value of a dice expression
//!   without rolling ([`avg_damage_from_dice`]), which is useful for
//!   balance formulas (e.g. scaling runic proc chance inversely to
//!   weapon damage).
//!
//! Games that need the full `DiceType` struct or other bracket-lib
//! functionality should depend on `bracket-lib` directly.

use bracket_lib::random::{parse_dice_string, RandomNumberGenerator};

/// Parse a dice notation string (`"1d6"`, `"2d4+1"`, etc.) and roll it,
/// returning the total.
///
/// On parse failure (malformed expression, overflow, unknown token),
/// returns `1` rather than panicking. The engine intentionally picks
/// `1` as the fallback instead of `0` so upstream damage pipelines
/// don't silently produce "the attack did nothing" outcomes.
///
/// # Examples
/// ```ignore
/// let mut rng = bracket_lib::random::RandomNumberGenerator::new();
/// let damage = roll_dice_string(&mut rng, "2d6+1"); // 3..=13
/// let fallback = roll_dice_string(&mut rng, "not a dice"); // 1
/// ```
pub fn roll_dice_string(rng: &mut RandomNumberGenerator, dice_string: &str) -> i32 {
    match parse_dice_string(dice_string) {
        Ok(dice_type) => rng.roll_dice(dice_type.n_dice, dice_type.die_type) + dice_type.bonus,
        Err(_) => 1,
    }
}

/// Compute the mathematical expected value (mean) of a dice expression.
///
/// Handles the same notation [`roll_dice_string`] accepts:
/// - `"NdM"` → average is `N * (M + 1) / 2`
/// - `"NdM+B"` → `N * (M + 1) / 2 + B`
/// - `"N"` (flat damage) → `N`
///
/// On malformed input, falls back to `2.0` — a deliberately nonzero
/// default so callers dividing by this value never get division-by-zero.
///
/// This helper does NOT consume RNG state; it's pure math, suitable
/// for use in balance formulas, UI display, and inverse-scaling
/// calculations like "chance to proc decreases with higher damage".
pub fn avg_damage_from_dice(dice_str: &str) -> f32 {
    let dice_str = dice_str.trim();
    let (dice_part, bonus) = if let Some(plus_idx) = dice_str.find('+') {
        let bonus: f32 = dice_str[plus_idx + 1..].trim().parse().unwrap_or(0.0);
        (&dice_str[..plus_idx], bonus)
    } else {
        (dice_str, 0.0)
    };

    if let Some(d_idx) = dice_part.find('d') {
        let n: f32 = dice_part[..d_idx].trim().parse().unwrap_or(1.0);
        let m: f32 = dice_part[d_idx + 1..].trim().parse().unwrap_or(4.0);
        n * (m + 1.0) / 2.0 + bonus
    } else {
        // Flat damage like "5"
        dice_str.parse::<f32>().unwrap_or(2.0)
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use bracket_lib::random::RandomNumberGenerator;

    // --- roll_dice_string ---

    #[test]
    fn roll_dice_string_1d1_is_deterministic() {
        // d1 is always 1 regardless of RNG state.
        let mut rng = RandomNumberGenerator::new();
        for _ in 0..10 {
            assert_eq!(roll_dice_string(&mut rng, "1d1"), 1);
        }
    }

    #[test]
    fn roll_dice_string_1d6_within_bounds() {
        let mut rng = RandomNumberGenerator::new();
        for _ in 0..100 {
            let result = roll_dice_string(&mut rng, "1d6");
            assert!((1..=6).contains(&result), "1d6 rolled {}", result);
        }
    }

    #[test]
    fn roll_dice_string_2d6_within_bounds() {
        let mut rng = RandomNumberGenerator::new();
        for _ in 0..100 {
            let result = roll_dice_string(&mut rng, "2d6");
            assert!((2..=12).contains(&result), "2d6 rolled {}", result);
        }
    }

    #[test]
    fn roll_dice_string_with_bonus() {
        let mut rng = RandomNumberGenerator::new();
        for _ in 0..100 {
            let result = roll_dice_string(&mut rng, "1d4+2");
            assert!((3..=6).contains(&result), "1d4+2 rolled {}", result);
        }
    }

    #[test]
    fn roll_dice_string_malformed_returns_one() {
        let mut rng = RandomNumberGenerator::new();
        assert_eq!(roll_dice_string(&mut rng, "not-a-dice"), 1);
        assert_eq!(roll_dice_string(&mut rng, ""), 1);
        assert_eq!(roll_dice_string(&mut rng, "d"), 1);
    }

    // --- avg_damage_from_dice ---

    #[test]
    fn avg_1d4_is_two_and_a_half() {
        assert!((avg_damage_from_dice("1d4") - 2.5).abs() < 0.01);
    }

    #[test]
    fn avg_1d6_is_three_and_a_half() {
        assert!((avg_damage_from_dice("1d6") - 3.5).abs() < 0.01);
    }

    #[test]
    fn avg_2d6_is_seven() {
        assert!((avg_damage_from_dice("2d6") - 7.0).abs() < 0.01);
    }

    #[test]
    fn avg_1d4_plus_2_is_four_and_a_half() {
        assert!((avg_damage_from_dice("1d4+2") - 4.5).abs() < 0.01);
    }

    #[test]
    fn avg_flat_damage() {
        assert!((avg_damage_from_dice("5") - 5.0).abs() < 0.01);
    }

    #[test]
    fn avg_malformed_falls_back_to_two() {
        // The fallback value is nonzero so callers dividing by it
        // never trigger division-by-zero.
        assert!((avg_damage_from_dice("garbage") - 2.0).abs() < 0.01);
    }

    #[test]
    fn avg_handles_whitespace() {
        assert!((avg_damage_from_dice("  1d6  ") - 3.5).abs() < 0.01);
        assert!((avg_damage_from_dice("1d4 + 2") - 4.5).abs() < 0.01);
    }

    #[test]
    fn avg_does_not_consume_rng() {
        // Sanity check: avg_damage_from_dice is pure and doesn't need
        // an RNG at all. This test exists to pin the signature.
        let a = avg_damage_from_dice("2d6");
        let b = avg_damage_from_dice("2d6");
        assert_eq!(a, b);
    }
}
