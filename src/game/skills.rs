//! Phase 3 skill system. See `docs/design/SKILLS.md` for the canonical spec.
//!
//! This commit (Task 1) lands the foundational enum types. Subsequent
//! tasks add pure helpers, ECS components, runtime systems, and UI.

#![allow(dead_code, unused_imports)]

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// One of eight player skills. Tracks a float 0.0..=27.0 of progress;
/// effects unlock at integer breakpoints via `floor(level/4)`.
#[derive(
    Serialize, Deserialize, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash,
)]
pub enum Skill {
    Fighting,
    Axes,
    ShortBlades,
    LongBlades,
    RangedWeapons,
    Armor,
    Dodging,
    Evocations,
}

impl Skill {
    pub const ALL: [Skill; 8] = [
        Skill::Fighting,
        Skill::Axes,
        Skill::ShortBlades,
        Skill::LongBlades,
        Skill::RangedWeapons,
        Skill::Armor,
        Skill::Dodging,
        Skill::Evocations,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Skill::Fighting => "Fighting",
            Skill::Axes => "Axes",
            Skill::ShortBlades => "Short Blades",
            Skill::LongBlades => "Long Blades",
            Skill::RangedWeapons => "Ranged Weapons",
            Skill::Armor => "Armor",
            Skill::Dodging => "Dodging",
            Skill::Evocations => "Evocations",
        }
    }
}

impl std::fmt::Display for Skill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Weapon family — declared on `ItemAsset` for weapons. Drives which
/// skill `weapon_skill_bonus` returns for a given equipped weapon.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaponSkill {
    Axes,
    ShortBlades,
    LongBlades,
    Ranged,
}

impl WeaponSkill {
    /// Map a weapon-skill tag to its corresponding `Skill` enum value.
    pub const fn as_skill(self) -> Skill {
        match self {
            WeaponSkill::Axes => Skill::Axes,
            WeaponSkill::ShortBlades => Skill::ShortBlades,
            WeaponSkill::LongBlades => Skill::LongBlades,
            WeaponSkill::Ranged => Skill::RangedWeapons,
        }
    }
}

/// Per-skill training selection. Cycle order in the UI:
/// Normal → Focused → Disabled → Normal.
#[derive(Serialize, Deserialize, Reflect, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkillState {
    #[default]
    Normal,
    Focused,
    Disabled,
}

/// Global training mode — flipped from the skill screen.
#[derive(Serialize, Deserialize, Reflect, Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrainingMode {
    /// Skills selected (Normal or Focused) divide XP weighted by recent use.
    #[default]
    Auto,
    /// Skills selected divide XP evenly (Focused gets 2× share).
    Manual,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_skills_have_unique_names() {
        let names: std::collections::HashSet<_> =
            Skill::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names.len(), 8);
    }

    #[test]
    fn weapon_skill_maps_to_skill() {
        assert_eq!(WeaponSkill::Axes.as_skill(), Skill::Axes);
        assert_eq!(WeaponSkill::ShortBlades.as_skill(), Skill::ShortBlades);
        assert_eq!(WeaponSkill::LongBlades.as_skill(), Skill::LongBlades);
        assert_eq!(WeaponSkill::Ranged.as_skill(), Skill::RangedWeapons);
    }

    #[test]
    fn skill_deserializes_from_unit_variant_name() {
        // Each Skill variant round-trips through RON as its Debug name.
        for skill in Skill::ALL {
            let ron_str = format!("{:?}", skill);
            let parsed: Skill = ron::from_str(&ron_str)
                .unwrap_or_else(|e| panic!("failed to parse {ron_str}: {e}"));
            assert_eq!(parsed, skill);
        }
    }
}
