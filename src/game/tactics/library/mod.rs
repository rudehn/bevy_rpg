//! Shipping tactic implementations. One per file, grouped by theme.
//!
//! Every tactic in this directory is a zero-sized struct implementing
//! [`crate::game::tactics::resolve::Tactic`]. Phase 2 wires them into
//! a `phf::Map` registry keyed by name; Phase 3+ migrates monsters
//! over to declarative `TacticList(["..."])` entries in `monsters.ron`.

mod combat;
mod flee;
mod movement;
mod wait;

pub use combat::MeleeAdjacent;
pub use flee::{FleeAtLowHp, KiteRetreat};
pub use movement::{HuntVisibleTarget, PursueLastKnownPosition};
pub use wait::WaitTactic;
