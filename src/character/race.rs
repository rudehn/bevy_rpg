//! Player race: marker component, its passive trait, and the level-up
//! stat-gain schedule.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::character::class::Attribute;

/// One of three player races chosen during character creation.
///
/// Race contributes:
/// - A baseline attribute distribution (defined in `assets/races.ron`,
///   typically 24 points across STR/DEX/INT, no negatives)
/// - An HP multiplier applied every level via the HP formula
/// - A stat-gain schedule that fires every N XP levels (Human 4:SDI,
///   Dwarf 4:SID, Elf 4:DI)
/// - One passive trait (see [`RaceTrait`]) applied at spawn
///
/// **Halfling has been removed** in Phase 2; the `Lucky` d20 reroll and
/// the `Versatile` chargen exception (which depended on the now-removed
/// free-point allocation step) are gone.
#[derive(
    Component,
    Reflect,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
)]
#[reflect(Component)]
pub enum Race {
    #[default]
    Human,
    Dwarf,
    Elf,
}

impl Race {
    /// The race's intrinsic passive trait. Hardcoded so the trait→race
    /// mapping is a compile-time invariant.
    pub const fn racial_trait(self) -> RaceTrait {
        match self {
            Race::Human => RaceTrait::Adaptive,
            Race::Dwarf => RaceTrait::Stoneblood,
            Race::Elf => RaceTrait::KeenSenses,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Race::Human => "Human",
            Race::Dwarf => "Dwarf",
            Race::Elf => "Elf",
        }
    }
}

impl std::fmt::Display for Race {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Passive trait conferred by a race. See `docs/design/CHARACTER.md` §Races
/// for the mechanics each variant implies.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RaceTrait {
    /// Human: gains a stat point from any of S/D/I at racial-schedule
    /// levels (the racial schedule itself encodes the gain — this trait
    /// is a marker for future systems that want to identify "the
    /// versatile race").
    Adaptive,
    /// Dwarf: 50% poison resistance, applied at spawn.
    Stoneblood,
    /// Elf: vision range +2 at spawn.
    KeenSenses,
}

/// How a race gains stat points on level-up. Fires every `interval` XP
/// levels; when it fires the player picks +1 in one of the `allowed`
/// attributes (single-element `allowed` means auto-apply, no prompt).
///
/// DCSS notation `4:SDI` corresponds to
/// `{ interval: 4, allowed: [Str, Dex, Int] }`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RaceGainSchedule {
    pub interval: u32,
    pub allowed: Vec<Attribute>,
}

impl RaceGainSchedule {
    /// Does the given XP level trigger a racial gain?
    pub fn fires_at(&self, xp_level: u32) -> bool {
        self.interval > 0 && xp_level > 0 && xp_level.is_multiple_of(self.interval)
    }

    /// DCSS-style display: `"4:SDI"`.
    pub fn notation(&self) -> String {
        let letters: String = self.allowed.iter().map(|a| a.letter()).collect();
        format!("{}:{}", self.interval, letters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_race_has_a_trait() {
        for race in [Race::Human, Race::Dwarf, Race::Elf] {
            let _ = race.racial_trait();
        }
    }

    #[test]
    fn trait_mapping_is_stable() {
        assert_eq!(Race::Human.racial_trait(), RaceTrait::Adaptive);
        assert_eq!(Race::Dwarf.racial_trait(), RaceTrait::Stoneblood);
        assert_eq!(Race::Elf.racial_trait(), RaceTrait::KeenSenses);
    }

    #[test]
    fn race_deserializes_from_unit_variant_name() {
        let cases = [
            ("Human", Race::Human),
            ("Dwarf", Race::Dwarf),
            ("Elf", Race::Elf),
        ];
        for (input, expected) in cases {
            let parsed: Race = ron::from_str(input).expect("parse race");
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn schedule_fires_at_multiples_of_interval() {
        let schedule = RaceGainSchedule {
            interval: 4,
            allowed: vec![Attribute::Str, Attribute::Dex, Attribute::Int],
        };
        // Multiples of 4 fire; non-multiples don't.
        for xl in 0..=27 {
            let fires = schedule.fires_at(xl);
            let expected = xl > 0 && xl % 4 == 0;
            assert_eq!(
                fires, expected,
                "fires_at({}) should be {}",
                xl, expected
            );
        }
    }

    #[test]
    fn notation_matches_dcss_style() {
        let dwarf = RaceGainSchedule {
            interval: 4,
            allowed: vec![Attribute::Str, Attribute::Int, Attribute::Dex],
        };
        assert_eq!(dwarf.notation(), "4:SID");

        let elf = RaceGainSchedule {
            interval: 4,
            allowed: vec![Attribute::Dex, Attribute::Int],
        };
        assert_eq!(elf.notation(), "4:DI");
    }
}
