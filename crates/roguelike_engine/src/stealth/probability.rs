//! Closed-form probability for the opposed d20 stealth/perception roll.

/// P(d20 + perception_mod > d20 + stealth_mod) where
/// `delta = perception_mod - stealth_mod`.
///
/// Enumerates the full 20×20 outcome space; cheap (400 ops) and exact.
pub fn notice_probability(delta: i32) -> f32 {
    let mut wins = 0u32;
    for x in 1..=20i32 {
        for y in 1..=20i32 {
            if x + delta > y {
                wins += 1;
            }
        }
    }
    wins as f32 / 400.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn delta_zero_is_just_under_half() {
        // P(d20 > d20) = (20·19/2) / 400 = 190/400 = 0.475
        assert!(approx(notice_probability(0), 0.475));
    }

    #[test]
    fn large_positive_delta_certain() {
        assert!(approx(notice_probability(20), 1.0));
    }

    #[test]
    fn large_negative_delta_zero() {
        assert!(approx(notice_probability(-20), 0.0));
    }

    #[test]
    fn delta_plus_ten_is_strongly_favoured() {
        // Sum over x in 1..=20 of |{y : y < x + 10}| = (10+11+...+19) + 10·20
        // = 145 + 200 = 345. 345/400 = 0.8625.
        assert!(approx(notice_probability(10), 0.8625));
    }

    #[test]
    fn delta_minus_ten_is_complement() {
        // Sum over x in 1..=20 of |{y : y < x - 10}| = 0+1+...+9 = 45.
        // 45/400 = 0.1125 (complement of +10 about 1: 1 - 0.8625 + (tie band) = 0.1125).
        assert!(approx(notice_probability(-10), 0.1125));
    }
}
