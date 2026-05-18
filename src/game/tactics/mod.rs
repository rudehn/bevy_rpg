//! Per-monster AI decision-making.
//!
//! See [`docs/design/TACTICS.md`] for the architecture overview, the
//! relationship to the FSM (`MonsterAIMode`), and the migration phases.
//!
//! ## Module map
//!
//! - [`resolve`] — pure resolver types ([`resolve::TurnSnapshot`],
//!   [`resolve::Tactic`], [`resolve::resolve_turn`]). No Bevy imports.
//! - [`library`] — shipping tactic implementations + the
//!   name-to-tactic registry ([`library::lookup_tactic`]).
//! - [`dispatch`] — the Bevy adapter: snapshot construction, state
//!   delta application, intent writing, and the
//!   [`dispatch::TacticsPlugin`].
//!
//! [`docs/design/TACTICS.md`]: ../../../../docs/design/TACTICS.md

pub mod dispatch;
pub mod library;
pub mod resolve;

pub use dispatch::{TacticBrain, TacticsPlugin};
