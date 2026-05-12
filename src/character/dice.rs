//! d20 rolling helper. Phase 2: the Halfling Lucky reroll is gone (Halfling
//! removed). The helper survives as a thin wrapper so future racial /
//! class / skill d20 special-cases have a single canonical site to plug
//! into.

use bracket_lib::prelude::RandomNumberGenerator;

use crate::character::race::Race;

/// Roll a d20. Currently a straightforward `rng.roll_dice(1, 20)` since
/// no shipping race has a d20-affecting trait. Kept as a function (not
/// inlined) so when a race trait wants to interact with d20 rolls
/// (e.g., re-introducing a Lucky-style reroll), the change happens in
/// one place.
pub fn roll_d20_with_race(rng: &mut RandomNumberGenerator, _race: Option<Race>) -> i32 {
    rng.roll_dice(1, 20)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_d20_returns_value_in_1_through_20() {
        let mut rng = RandomNumberGenerator::seeded(42);
        for _ in 0..200 {
            let v = roll_d20_with_race(&mut rng, None);
            assert!((1..=20).contains(&v), "d20 produced out-of-range {}", v);
        }
    }

    /// Sanity-check that the wrapper consumes exactly one RNG draw — no
    /// stealth reroll lurking. Compare to a fresh RNG with the same seed.
    #[test]
    fn roll_d20_consumes_one_draw() {
        let seed = 1234;
        let mut a = RandomNumberGenerator::seeded(seed);
        let mut b = RandomNumberGenerator::seeded(seed);
        let v_a = roll_d20_with_race(&mut a, Some(Race::Human));
        let v_b = b.roll_dice(1, 20);
        assert_eq!(v_a, v_b);
    }
}
