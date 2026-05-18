//! Shipping tactic implementations. One per file, grouped by theme.
//!
//! Every tactic in this directory is a zero-sized struct implementing
//! [`crate::game::tactics::resolve::Tactic`]. The [`lookup_tactic`]
//! function below maps RON-declared names to their static instances;
//! [`ALL_TACTIC_NAMES`] enumerates them for startup validation.
//!
//! Adding a new tactic:
//! 1. Create the struct + impl + tests in a new file (or in an
//!    existing themed file like `combat.rs`).
//! 2. Re-export it from this module.
//! 3. Add a `const` reference + `match` arm in `lookup_tactic`.
//! 4. Add the name to `ALL_TACTIC_NAMES`.
//! 5. Reference the name in `monsters.ron` `ai: TacticList([...])`
//!    entries.
//!
//! The startup validator (`validate_tactic_names` in the dispatch
//! plugin) panics if a `monsters.ron` entry references a name not in
//! `ALL_TACTIC_NAMES`, catching typos at boot rather than turn time.

mod aquatic;
mod combat;
mod erratic;
mod flee;
mod movement;
mod ranged;
mod squad;
mod wait;

pub use aquatic::SubmergeOrSurface;
pub use combat::MeleeAdjacent;
pub use erratic::ErraticMove;
pub use flee::{FleeAtLowHp, FleePanicked, KiteRetreat};
pub use movement::{FreeWander, HuntVisibleTarget, PursueLastKnownPosition};
pub use ranged::{RangedAttack, UseAbility};
pub use squad::SquadLeash;
pub use wait::WaitTactic;

use crate::game::tactics::resolve::Tactic;

// ---------------------------------------------------------------------
// Static tactic instances
// ---------------------------------------------------------------------
//
// Each tactic is a zero-sized struct, so a `const` reference to its
// unit value is trivially `'static`. The dispatcher reads these
// through the `lookup_tactic` table; the production tactic lists
// stored on `TacticBrain` are `&'static [&'static dyn Tactic]` slices
// of these references, allocated once per unique list at spawn time
// via `Vec::leak`.

const WAIT: &dyn Tactic = &WaitTactic;
const MELEE_ADJACENT: &dyn Tactic = &MeleeAdjacent;
const FLEE_AT_LOW_HP: &dyn Tactic = &FleeAtLowHp;
const FLEE_PANICKED: &dyn Tactic = &FleePanicked;
const KITE_RETREAT: &dyn Tactic = &KiteRetreat;
const HUNT_VISIBLE_TARGET: &dyn Tactic = &HuntVisibleTarget;
const PURSUE_LAST_KNOWN_POSITION: &dyn Tactic = &PursueLastKnownPosition;
const FREE_WANDER: &dyn Tactic = &FreeWander;
const RANGED_ATTACK: &dyn Tactic = &RangedAttack;
const USE_ABILITY: &dyn Tactic = &UseAbility;
const ERRATIC_MOVE: &dyn Tactic = &ErraticMove;
const SQUAD_LEASH: &dyn Tactic = &SquadLeash;
const SUBMERGE_OR_SURFACE: &dyn Tactic = &SubmergeOrSurface;

/// Every tactic name that may appear in `monsters.ron` `ai: TacticList`
/// entries. Used by `validate_tactic_names` at startup to fail loudly
/// on typos. Keep alphabetized for legibility.
pub const ALL_TACTIC_NAMES: &[&str] = &[
    "ErraticMove",
    "FleeAtLowHp",
    "FleePanicked",
    "FreeWander",
    "HuntVisibleTarget",
    "KiteRetreat",
    "MeleeAdjacent",
    "PursueLastKnownPosition",
    "RangedAttack",
    "SquadLeash",
    "SubmergeOrSurface",
    "UseAbility",
    "Wait",
];

/// The terminal tactic every well-formed list must end with. The
/// startup validator enforces this so the resolver's `FallbackWait`
/// outcome (which carries a different name and skips delta updates)
/// never fires in production.
pub const TERMINAL_TACTIC_NAME: &str = "Wait";

/// Resolve a RON-declared tactic name to its static instance. Returns
/// `None` for unknown names; the caller is expected to surface the
/// error via panic at startup (see `validate_tactic_names`).
pub fn lookup_tactic(name: &str) -> Option<&'static dyn Tactic> {
    match name {
        "Wait" => Some(WAIT),
        "MeleeAdjacent" => Some(MELEE_ADJACENT),
        "FleeAtLowHp" => Some(FLEE_AT_LOW_HP),
        "FleePanicked" => Some(FLEE_PANICKED),
        "KiteRetreat" => Some(KITE_RETREAT),
        "HuntVisibleTarget" => Some(HUNT_VISIBLE_TARGET),
        "PursueLastKnownPosition" => Some(PURSUE_LAST_KNOWN_POSITION),
        "FreeWander" => Some(FREE_WANDER),
        "RangedAttack" => Some(RANGED_ATTACK),
        "UseAbility" => Some(USE_ABILITY),
        "ErraticMove" => Some(ERRATIC_MOVE),
        "SquadLeash" => Some(SQUAD_LEASH),
        "SubmergeOrSurface" => Some(SUBMERGE_OR_SURFACE),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_resolves() {
        for name in ALL_TACTIC_NAMES {
            assert!(
                lookup_tactic(name).is_some(),
                "ALL_TACTIC_NAMES contains {name:?} but lookup_tactic doesn't handle it"
            );
        }
    }

    #[test]
    fn unknown_name_returns_none() {
        assert!(lookup_tactic("ThisTacticDoesNotExist").is_none());
    }

    #[test]
    fn terminal_tactic_name_resolves() {
        assert!(lookup_tactic(TERMINAL_TACTIC_NAME).is_some());
    }

    #[test]
    fn lookup_returns_matching_tactic_name() {
        // Spot-check: looked-up tactic carries the right name() string.
        for name in ALL_TACTIC_NAMES {
            let tactic = lookup_tactic(name).unwrap();
            assert_eq!(
                tactic.name(),
                *name,
                "lookup_tactic({name:?}) returned tactic with name() = {:?}",
                tactic.name()
            );
        }
    }
}
