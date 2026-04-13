//! Reusable AI primitives for turn-based roguelikes.
//!
//! The engine ships two AI tiers:
//!
//! - [`decisions`] — pure decision helpers (flee threshold, kiting, chase
//!   leash, erratic movement, flee direction). No ECS, no Bevy, no RNG —
//!   just arithmetic answers to "should this monster do X?". Game AI loops
//!   call these to stay concise and testable.
//! - [`goap`] — a generic Goal-Oriented Action Planning framework. Provides
//!   [`goap::WorldState`], [`goap::Goal`], [`goap::ActionDef`], and the
//!   [`goap::plan`] function that finds the cheapest action sequence to
//!   reach a desired state. Game-specific goal/action content and the
//!   Bevy dispatch layer stay in the game crate.
//!
//! The full monster AI loop (`MonsterAI::execute`), ability dispatch,
//! ranged-attack integration, and pathfinding live in the game crate for
//! now — they depend on game-specific types (`StatusEffects`,
//! `MonsterAbilities`, `RangedAttackIntent`) that haven't been
//! decoupled yet.

pub mod decisions;
pub mod goap;
pub mod monster_ai;
pub mod pathfinding;

pub use monster_ai::{MonsterAI, MonsterAIMode, GUARD_PATROL_RADIUS};
