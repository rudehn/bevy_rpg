//! Player race: marker component and its associated passive trait.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// One of four player races chosen during character creation.
///
/// Race contributes baseline attribute adjustments (defined in
/// `assets/races.ron`) and a single passive trait (see [`RaceTrait`]).
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
    Halfling,
}

impl Race {
    /// The race's intrinsic passive trait. Hardcoded to keep the trait→race
    /// mapping a compile-time invariant — every race always has exactly one
    /// trait, and the trait enum is exhaustive at every consumer.
    pub const fn racial_trait(self) -> RaceTrait {
        match self {
            Race::Human => RaceTrait::Versatile,
            Race::Dwarf => RaceTrait::Stoneblood,
            Race::Elf => RaceTrait::KeenSenses,
            Race::Halfling => RaceTrait::Lucky,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Race::Human => "Human",
            Race::Dwarf => "Dwarf",
            Race::Elf => "Elf",
            Race::Halfling => "Halfling",
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
    /// Human: one of the 4 free allocation points may exceed the per-stat cap by 1.
    /// Allocation-screen only — no runtime effect.
    Versatile,
    /// Dwarf: 50% poison resistance, applied at spawn.
    Stoneblood,
    /// Elf: vision range +2 at spawn.
    KeenSenses,
    /// Halfling: reroll any natural 1 on a d20 (no cooldown). Must take the
    /// second result. Implemented via a shared `d20_roll` helper that every
    /// d20 call site consumes.
    Lucky,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_race_has_a_trait() {
        // Compile-time-ish guard: walking all variants ensures the match arm
        // in `racial_trait` is kept exhaustive when new races are added.
        for race in [Race::Human, Race::Dwarf, Race::Elf, Race::Halfling] {
            let _ = race.racial_trait();
        }
    }

    #[test]
    fn trait_mapping_is_stable() {
        assert_eq!(Race::Human.racial_trait(), RaceTrait::Versatile);
        assert_eq!(Race::Dwarf.racial_trait(), RaceTrait::Stoneblood);
        assert_eq!(Race::Elf.racial_trait(), RaceTrait::KeenSenses);
        assert_eq!(Race::Halfling.racial_trait(), RaceTrait::Lucky);
    }

    #[test]
    fn race_deserializes_from_unit_variant_name() {
        let cases = [
            ("Human", Race::Human),
            ("Dwarf", Race::Dwarf),
            ("Elf", Race::Elf),
            ("Halfling", Race::Halfling),
        ];
        for (input, expected) in cases {
            let parsed: Race = ron::from_str(input).expect("parse race");
            assert_eq!(parsed, expected);
        }
    }
}
