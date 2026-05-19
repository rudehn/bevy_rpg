//! Generic grid algorithms for map generation.
//!
//! The implementation lives in the engine crate at
//! `roguelike_engine::map::builders::algorithms`. This module re-exports
//! the public surface so game-side builders can `use
//! crate::map::builders::algorithms::*` without reaching across crates.

pub use roguelike_engine::map::builders::algorithms::*;
