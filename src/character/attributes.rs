//! Attribute helpers. The `Attributes` component itself is added in the next
//! commit; this module ships just the pure D&D 5e ability-modifier function
//! so that asset tests and downstream commits can use it independently.

/// D&D 5e ability modifier: `floor((score - 10) / 2)`.
///
/// Uses `div_euclid` so negative scores round toward negative infinity
/// (i.e. 8 → -1, not 0).
pub fn ability_mod(score: i32) -> i32 {
    (score - 10).div_euclid(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Matches the D&D 5e modifier table exactly. If any of these values
    /// drifts, downstream combat math will quietly produce wrong results.
    #[test]
    fn ability_mod_matches_5e_table() {
        let cases = [
            (1, -5),
            (2, -4),
            (3, -4),
            (4, -3),
            (5, -3),
            (6, -2),
            (7, -2),
            (8, -1),
            (9, -1),
            (10, 0),
            (11, 0),
            (12, 1),
            (13, 1),
            (14, 2),
            (15, 2),
            (16, 3),
            (17, 3),
            (18, 4),
            (19, 4),
            (20, 5),
            (21, 5),
            (22, 6),
            (29, 9),
            (30, 10),
        ];
        for (score, expected) in cases {
            assert_eq!(
                ability_mod(score),
                expected,
                "ability_mod({score}) should be {expected}"
            );
        }
    }

    /// Negative attribute scores aren't reachable through normal allocation
    /// (floor is 8), but the floor-division behavior is worth pinning down
    /// in case some future effect (curse, vampiric drain) pushes a stat low.
    #[test]
    fn ability_mod_handles_negative_scores() {
        assert_eq!(ability_mod(0), -5);
        assert_eq!(ability_mod(-1), -6);
        assert_eq!(ability_mod(-2), -6);
        assert_eq!(ability_mod(-3), -7);
    }
}
