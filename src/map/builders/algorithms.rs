//! Generic grid algorithms for map generation.
//!
//! The implementation has been moved to the engine crate at
//! `roguelike_engine::map::builders::algorithms`. This module re-exports the
//! full public surface for backwards compatibility so existing game code
//! (brogelike.rs, lake_builder.rs, etc.) doesn't have to change its imports.
//!
//! New game code should prefer importing directly from `roguelike_engine`.

pub use roguelike_engine::map::builders::algorithms::*;
