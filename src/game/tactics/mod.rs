//! Per-monster AI decision-making.
//!
//! See [`docs/design/TACTICS.md`] for the architecture overview, the
//! relationship to the FSM (`MonsterAIMode`), and the migration phases.
//!
//! ## Module map
//!
//! - [`resolve`] — pure resolver types ([`resolve::TurnSnapshot`],
//!   [`resolve::Tactic`], [`resolve::resolve_turn`]). No Bevy imports.
//! - [`library`] — shipping tactic implementations, one per file.
//!
//! Phase 1 ships only the pure module + tactics. The Bevy adapter
//! lands in Phase 2.
//!
//! [`docs/design/TACTICS.md`]: ../../../../docs/design/TACTICS.md

pub mod library;
pub mod resolve;
