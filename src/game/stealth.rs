//! Stealth system — game-side modifier composition + per-turn systems.
//! See docs/design/STEALTH.md for the canonical writeup.

use bevy::prelude::*;

/// Per-monster species perception modifier, copied from MonsterAsset
/// at spawn time. Read by perception_tick_system to build
/// PerceptionComponents.base. Inserted on every monster in Task F1.
#[derive(Component, Debug, Clone, Copy)]
pub struct MonsterPerception(pub i32);

/// Tile-light → stealth modifier. Bright = penalty, dark = bonus.
/// Thresholds are placeholders — expect post-implementation tuning.
pub fn light_modifier(intensity: f32) -> i32 {
    if intensity >= 0.75 {
        -3
    } else if intensity >= 0.40 {
        -1
    } else if intensity > 0.0 {
        2
    } else {
        3
    }
}

/// Distance → perception bonus. Closer = easier to see.
/// Chebyshev distance (matches 8-way movement).
pub fn close_range_bonus(chebyshev_distance: i32) -> i32 {
    match chebyshev_distance {
        d if d <= 1 => 2,
        2..=3 => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_buckets() {
        assert_eq!(light_modifier(1.0), -3);
        assert_eq!(light_modifier(0.75), -3);
        assert_eq!(light_modifier(0.74), -1);
        assert_eq!(light_modifier(0.40), -1);
        assert_eq!(light_modifier(0.39), 2);
        assert_eq!(light_modifier(0.01), 2);
        assert_eq!(light_modifier(0.0), 3);
    }

    #[test]
    fn close_range_buckets() {
        assert_eq!(close_range_bonus(0), 2);
        assert_eq!(close_range_bonus(1), 2);
        assert_eq!(close_range_bonus(2), 1);
        assert_eq!(close_range_bonus(3), 1);
        assert_eq!(close_range_bonus(4), 0);
        assert_eq!(close_range_bonus(99), 0);
    }
}
