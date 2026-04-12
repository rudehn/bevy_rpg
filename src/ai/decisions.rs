//! Pure AI decision helpers.
//!
//! Each function answers a single tactical question ("should this monster
//! flee?", "should it kite backwards?", "has it chased long enough to give
//! up?", "which cardinal direction leads away from the threat?") using
//! only `i32` / `f32` / `u32` inputs. No ECS, no Bevy `World`, no RNG.
//!
//! Game AI loops compose these helpers to stay concise: instead of
//! inlining the arithmetic, the loop reads like a sequence of named
//! decisions, and the decisions themselves are independently tested.

/// Should this monster flee?
///
/// Returns `true` when the current HP ratio is strictly below
/// `flee_threshold`. A threshold of `0.0` disables fleeing entirely;
/// zero-max-HP entities (ghosts, freshly spawned) never flee.
///
/// The strict inequality is deliberate — a monster AT its threshold
/// stands its ground one more turn, which avoids a flicker where a
/// monster oscillates in and out of flee at borderline HP.
pub fn should_flee(current_hp: i32, max_hp: i32, flee_threshold: f32) -> bool {
    if flee_threshold <= 0.0 || max_hp <= 0 {
        return false;
    }
    (current_hp as f32 / max_hp as f32) < flee_threshold
}

/// Should this monster move erratically this turn?
///
/// `erratic_chance` is a probability in `[0.0, 1.0]`. `roll` is a uniform
/// random value in the same range (typically from `rand::Rng::random`).
/// Callers own the RNG so this function stays pure and testable.
///
/// A chance of `0.0` disables erratic movement unconditionally.
pub fn should_move_erratically(erratic_chance: f32, roll: f32) -> bool {
    erratic_chance > 0.0 && roll < erratic_chance
}

/// Should this monster give up chasing and return to idle?
///
/// `chase_distance` is the number of turns the monster has been chasing
/// without seeing the player. `chase_leash` is the maximum allowed. A
/// leash of `0` means "never give up".
pub fn should_give_up_chase(chase_distance: u32, chase_leash: u32) -> bool {
    chase_leash > 0 && chase_distance >= chase_leash
}

/// Should a kiting monster retreat from the player?
///
/// Returns `true` when the squared Euclidean distance between the
/// monster and the player is less than `kite_distance^2`. Using squared
/// distance avoids a sqrt and stays in integer math.
pub fn should_kite_retreat(
    monster_x: i32,
    monster_y: i32,
    player_x: i32,
    player_y: i32,
    kite_distance: u32,
) -> bool {
    let dx = (monster_x - player_x).abs();
    let dy = (monster_y - player_y).abs();
    (dx * dx + dy * dy) < (kite_distance as i32 * kite_distance as i32)
}

/// Pick the best cardinal direction to flee AWAY from a threat position.
///
/// Returns `(dx, dy)` where each component is `-1`, `0`, or `1`. Favours
/// the axis with the larger displacement, so a monster one tile east of
/// a threat flees east; a monster three tiles south and one tile east
/// flees south first. A monster standing on top of the threat returns
/// `(0, 0)` — the caller decides whether to wait, attack, or pick a
/// random direction.
pub fn flee_direction(monster_x: i32, monster_y: i32, threat_x: i32, threat_y: i32) -> (i32, i32) {
    let dx = monster_x - threat_x;
    let dy = monster_y - threat_y;
    if dx == 0 && dy == 0 {
        return (0, 0);
    }
    if dx.abs() >= dy.abs() {
        (dx.signum(), 0)
    } else {
        (0, dy.signum())
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- should_flee ---

    #[test]
    fn flee_when_below_threshold() {
        assert!(should_flee(2, 10, 0.3));
    }

    #[test]
    fn no_flee_when_above_threshold() {
        assert!(!should_flee(5, 10, 0.3));
    }

    #[test]
    fn no_flee_when_threshold_zero() {
        assert!(!should_flee(1, 10, 0.0));
    }

    #[test]
    fn no_flee_when_at_exact_threshold() {
        assert!(!should_flee(3, 10, 0.3));
    }

    #[test]
    fn no_flee_when_max_hp_zero() {
        assert!(!should_flee(0, 0, 0.5));
    }

    // --- should_move_erratically ---

    #[test]
    fn erratic_with_low_roll() {
        assert!(should_move_erratically(0.3, 0.1));
    }

    #[test]
    fn not_erratic_with_high_roll() {
        assert!(!should_move_erratically(0.3, 0.5));
    }

    #[test]
    fn never_erratic_when_chance_zero() {
        assert!(!should_move_erratically(0.0, 0.0));
    }

    // --- should_give_up_chase ---

    #[test]
    fn give_up_when_leash_exceeded() {
        assert!(should_give_up_chase(10, 8));
    }

    #[test]
    fn keep_chasing_within_leash() {
        assert!(!should_give_up_chase(5, 8));
    }

    #[test]
    fn give_up_at_exact_leash() {
        assert!(should_give_up_chase(8, 8));
    }

    #[test]
    fn never_give_up_when_leash_zero() {
        assert!(!should_give_up_chase(100, 0));
    }

    // --- should_kite_retreat ---

    #[test]
    fn kite_when_adjacent() {
        assert!(should_kite_retreat(5, 5, 6, 5, 3));
    }

    #[test]
    fn kite_when_close() {
        assert!(should_kite_retreat(5, 5, 7, 5, 3));
    }

    #[test]
    fn no_kite_when_at_distance() {
        assert!(!should_kite_retreat(5, 5, 8, 5, 3));
    }

    #[test]
    fn no_kite_when_far() {
        assert!(!should_kite_retreat(5, 5, 10, 5, 3));
    }

    // --- flee_direction ---

    #[test]
    fn flee_away_east() {
        assert_eq!(flee_direction(5, 5, 2, 5), (1, 0));
    }

    #[test]
    fn flee_away_west() {
        assert_eq!(flee_direction(5, 5, 8, 5), (-1, 0));
    }

    #[test]
    fn flee_away_north() {
        assert_eq!(flee_direction(5, 5, 5, 8), (0, -1));
    }

    #[test]
    fn flee_away_south() {
        assert_eq!(flee_direction(5, 5, 5, 2), (0, 1));
    }

    #[test]
    fn flee_on_top_of_threat() {
        assert_eq!(flee_direction(5, 5, 5, 5), (0, 0));
    }
}
