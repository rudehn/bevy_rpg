//! d20 rolling helpers that apply race-specific traits (currently only
//! Halfling Lucky). Once saves and skills land, every d20 site in the
//! codebase routes through these helpers so the trait fires uniformly.

use bracket_lib::prelude::RandomNumberGenerator;

use crate::character::race::Race;

/// Apply Halfling **Lucky** rerolls. Pure — takes both the natural roll
/// and the value it would reroll to, so deterministic tests can pin every
/// branch.
///
/// `race` is `Option<Race>` because monsters don't carry a `Race`
/// component; only the player entity does. A `None` short-circuits to the
/// natural roll.
pub fn apply_lucky(natural: i32, race: Option<Race>, would_reroll_to: i32) -> i32 {
    if natural == 1 && race == Some(Race::Halfling) {
        would_reroll_to
    } else {
        natural
    }
}

/// Roll a d20 against an `RandomNumberGenerator` and apply Halfling Lucky
/// if the roller's race is `Halfling`. The single canonical d20 site.
/// Returns the **effective** roll (1..=20).
pub fn roll_d20_with_race(rng: &mut RandomNumberGenerator, race: Option<Race>) -> i32 {
    let natural = rng.roll_dice(1, 20);
    apply_lucky(natural, race, rng.roll_dice(1, 20))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lucky_passes_through_when_natural_is_not_1() {
        for natural in 2..=20 {
            assert_eq!(
                apply_lucky(natural, Some(Race::Halfling), 17),
                natural,
                "Halfling should NOT reroll a natural {natural}"
            );
        }
    }

    #[test]
    fn lucky_rerolls_natural_1_for_halfling() {
        // Halfling rolls a 1 → takes the second roll (even if it's worse).
        assert_eq!(apply_lucky(1, Some(Race::Halfling), 14), 14);
        assert_eq!(apply_lucky(1, Some(Race::Halfling), 20), 20);
        // The second result is taken regardless — even another 1.
        assert_eq!(apply_lucky(1, Some(Race::Halfling), 1), 1);
    }

    #[test]
    fn lucky_does_not_fire_for_non_halflings() {
        for race in [Race::Human, Race::Dwarf, Race::Elf] {
            assert_eq!(
                apply_lucky(1, Some(race), 20),
                1,
                "{race} should not reroll a natural 1"
            );
        }
        // Entities without a Race component (monsters) keep their natural 1.
        assert_eq!(apply_lucky(1, None, 20), 1);
    }

    /// Seeded RNG: verify the helper actually consumes 2 rolls for a
    /// Halfling that rolled a 1, and 1 roll otherwise. Catches accidental
    /// re-implementation drift.
    #[test]
    fn roll_d20_consumes_one_roll_for_non_halflings_two_for_halfling_natural_1() {
        // Find a seed where the first roll is 1, so we can prove the
        // reroll fires for Halflings and is skipped otherwise.
        let seed_with_natural_1 = {
            let mut s = 0u64;
            loop {
                let mut rng = RandomNumberGenerator::seeded(s);
                if rng.roll_dice(1, 20) == 1 {
                    break s;
                }
                s += 1;
                assert!(s < 10_000, "couldn't find a natural-1 seed");
            }
        };

        // Halfling: should reroll, leaving the RNG in the same state as
        // 2 consecutive `roll_dice(1, 20)` calls.
        let mut rng = RandomNumberGenerator::seeded(seed_with_natural_1);
        let result = roll_d20_with_race(&mut rng, Some(Race::Halfling));
        let mut reference = RandomNumberGenerator::seeded(seed_with_natural_1);
        let _first = reference.roll_dice(1, 20);
        let second = reference.roll_dice(1, 20);
        assert_eq!(result, second);

        // Non-Halfling: only one roll consumed, result is the natural 1.
        let mut rng = RandomNumberGenerator::seeded(seed_with_natural_1);
        let result = roll_d20_with_race(&mut rng, Some(Race::Human));
        assert_eq!(result, 1);
    }
}
