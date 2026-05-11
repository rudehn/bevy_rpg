//! Game-side adapter for the engine's decoration propagator.
//!
//! The actual algorithm lives in
//! `roguelike_engine::map::builders::decoration_propagator`. This adapter
//! exists only so the game's `BuilderChain` (which still uses the
//! game-side `MetaMapBuilder` trait) can register the engine's builder.

pub use roguelike_engine::map::builders::decoration_propagator::DecorationPropagator;

use crate::map::builders::{BuilderMap, BuilderPhase, MetaMapBuilder};

impl MetaMapBuilder for DecorationPropagator {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        use roguelike_engine::map::builders::MapBuilder;
        self.build(build_data);
    }

    fn phase(&self) -> Option<BuilderPhase> {
        Some(BuilderPhase::Finalization)
    }
}
