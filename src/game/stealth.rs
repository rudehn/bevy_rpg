//! Stealth system — game-side modifier composition + per-turn systems.
//! See docs/design/STEALTH.md for the canonical writeup.

use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::character::Attributes;
use crate::game::skills::{Skill, Skills};
use roguelike_engine::stealth::{noise_modifier, NoiseMap};

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

/// Component breakdown for the stealth side of the opposed roll.
/// Returned by `compute_stealth_components` for UI display.
#[derive(Debug, Clone, Copy)]
pub struct StealthComponents {
    pub skill_half: i32,
    pub dex_mod: i32,
    pub armor_penalty: i32,
    pub light_mod: i32,
    pub noise_mod: i32,
}

impl StealthComponents {
    pub fn total(&self) -> i32 {
        self.skill_half + self.dex_mod - self.armor_penalty + self.light_mod + self.noise_mod
    }
}

/// Component breakdown for the perception side.
#[derive(Debug, Clone, Copy)]
pub struct PerceptionComponents {
    pub base: i32,
    /// -10 if the monster is asleep, 0 otherwise.
    pub asleep_penalty: i32,
    pub close_range_bonus: i32,
}

impl PerceptionComponents {
    pub fn total(&self) -> i32 {
        self.base + self.asleep_penalty + self.close_range_bonus
    }
}

/// Build the stealth breakdown for `target_pos`. Callers resolve
/// `light_intensity` from `LightMap` (which has no `intensity_at`
/// helper — index via `map.xy_idx(pos.x, pos.y)` into
/// `light_map.values`) and `equipped_armor_penalty` via
/// [`equipped_armor_stealth_penalty`].
pub fn compute_stealth_components(
    skills: Option<&Skills>,
    attrs: Option<&Attributes>,
    equipped_armor_penalty: i32,
    target_pos: Point,
    light_intensity: f32,
    noise_map: &NoiseMap,
) -> StealthComponents {
    let stealth_level = skills.map(|s| s.get(Skill::Stealth)).unwrap_or(0.0) as i32;
    let dex_mod = attrs.map(|a| a.dex_mod()).unwrap_or(0);
    StealthComponents {
        skill_half: stealth_level / 2,
        dex_mod,
        armor_penalty: equipped_armor_penalty,
        light_mod: light_modifier(light_intensity),
        noise_mod: noise_modifier(target_pos, noise_map),
    }
}

pub fn compute_perception_components(
    monster_base_perception: i32,
    is_asleep: bool,
    chebyshev_distance: i32,
) -> PerceptionComponents {
    PerceptionComponents {
        base: monster_base_perception,
        asleep_penalty: if is_asleep { -10 } else { 0 },
        close_range_bonus: close_range_bonus(chebyshev_distance),
    }
}

pub fn compute_stealth_mod(
    skills: Option<&Skills>,
    attrs: Option<&Attributes>,
    equipped_armor_penalty: i32,
    target_pos: Point,
    light_intensity: f32,
    noise_map: &NoiseMap,
) -> i32 {
    compute_stealth_components(
        skills,
        attrs,
        equipped_armor_penalty,
        target_pos,
        light_intensity,
        noise_map,
    )
    .total()
}

pub fn compute_perception_mod(
    monster_base_perception: i32,
    is_asleep: bool,
    chebyshev_distance: i32,
) -> i32 {
    compute_perception_components(monster_base_perception, is_asleep, chebyshev_distance).total()
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

    #[test]
    fn stealth_components_total_subtracts_armor() {
        let parts = StealthComponents {
            skill_half: 6,
            dex_mod: 4,
            armor_penalty: 1,
            light_mod: 2,
            noise_mod: 0,
        };
        assert_eq!(parts.total(), 11); // 6 + 4 - 1 + 2 + 0
    }

    #[test]
    fn perception_components_total_adds_all() {
        let parts = PerceptionComponents {
            base: 3,
            asleep_penalty: 0,
            close_range_bonus: 2,
        };
        assert_eq!(parts.total(), 5);
    }

    #[test]
    fn asleep_monster_carries_minus_ten() {
        let parts = PerceptionComponents {
            base: 0,
            asleep_penalty: -10,
            close_range_bonus: 0,
        };
        assert_eq!(parts.total(), -10);
    }
}
