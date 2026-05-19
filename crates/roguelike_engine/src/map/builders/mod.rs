//! Map builder framework and generic algorithms.
//!
//! The engine provides:
//!
//! - [`BuildContext`] — a trait abstracting map access, room metadata,
//!   starting position, exclusion zones, and a seeded RNG. Engine
//!   builders operate through this trait; games extend it with their
//!   own context struct that adds spawn lists.
//! - [`EngineBuilderMap`] — the engine's concrete [`BuildContext`]
//!   implementation. Holds the bare minimum for pure terrain builders.
//! - [`MapBuilder`] — the unified builder trait (replaces the old
//!   `InitialMapBuilder` / `MetaMapBuilder` split). Generic over `C:
//!   BuildContext` so engine builders work with any context and game
//!   builders can access game-specific fields.
//! - [`BuilderChain`] — a pipeline runner that holds a list of boxed
//!   builders and a context, runs them in sequence, and enforces
//!   monotonic [`BuilderPhase`] ordering.
//! - [`BuilderPhase`] — an ordered enum for pipeline phases
//!   (Geometry → TerrainCleanup → StructurePlacement →
//!   ConnectivityCull → Spawning → Finalization).
//!
//! Generic algorithms ([`algorithms`]) are also re-exported here.

pub mod algorithms;
pub mod brogelike;
pub mod bsp_dungeon;
pub mod cave_eroder;
pub mod choke_map;
pub mod corridors;
pub mod decoration_propagator;
pub mod diagonal_culler;
pub mod exit_points;
pub mod finish_doors;
pub mod isolated_area_culler;
pub mod lake_builder;
pub mod pillar_culler;
pub mod room_drawer;
pub mod start_point;
pub mod unseen_culler;

use bevy::log::debug;
use bracket_lib::prelude::Rect;
use bracket_lib::random::RandomNumberGenerator;
use std::time::Instant;

use crate::components::Position;
use crate::map::map::Map;

// =====================================================================
// BuildContext trait
// =====================================================================

/// The boundary between engine builders and game builders.
///
/// Engine builders see only this trait — they never touch game-specific
/// spawn lists, squad counters, or content manifests. Games either use
/// [`EngineBuilderMap`] directly for pure-engine chains or wrap it in
/// their own context struct that implements this trait by delegation.
pub trait BuildContext {
    fn map(&self) -> &Map;
    fn map_mut(&mut self) -> &mut Map;

    fn width(&self) -> i32;
    fn height(&self) -> i32;

    fn rooms(&self) -> Option<&Vec<Rect>>;
    fn set_rooms(&mut self, rooms: Vec<Rect>);
    fn starting_position(&self) -> Option<Position>;
    fn set_starting_position(&mut self, pos: Position);

    fn exclusion_zones(&self) -> &[Rect];
    fn add_exclusion_zone(&mut self, rect: Rect);

    /// Seeded RNG — tests pass fixed seeds for reproducibility.
    fn rng(&mut self) -> &mut RandomNumberGenerator;

    /// Hook for mapgen snapshot capture (default no-op).
    fn take_snapshot(&mut self) {}
}

// =====================================================================
// EngineBuilderMap
// =====================================================================

/// The engine's concrete [`BuildContext`] implementation.
///
/// Holds everything engine builders need: the map, structural metadata,
/// exclusion zones, and a seeded RNG. Games that need spawn lists
/// should wrap this in their own struct that also implements
/// [`BuildContext`] via delegation.
pub struct EngineBuilderMap {
    pub map: Map,
    pub starting_position: Option<Position>,
    pub rooms: Option<Vec<Rect>>,
    pub width: i32,
    pub height: i32,
    pub exclusion_zones: Vec<Rect>,
    pub rng: RandomNumberGenerator,
    pub snapshots: Vec<Map>,
}

impl EngineBuilderMap {
    /// Create a new builder map with an unseeded (random) RNG.
    pub fn new(depth: i32, width: i32, height: i32, name: impl ToString) -> Self {
        Self {
            map: Map::new(depth, width, height, name),
            starting_position: None,
            rooms: None,
            width,
            height,
            exclusion_zones: Vec::new(),
            rng: RandomNumberGenerator::new(),
            snapshots: Vec::new(),
        }
    }

    /// Create a builder map with a fixed seed for deterministic output.
    pub fn with_seed(
        depth: i32,
        width: i32,
        height: i32,
        name: impl ToString,
        seed: u64,
    ) -> Self {
        Self {
            map: Map::new(depth, width, height, name),
            starting_position: None,
            rooms: None,
            width,
            height,
            exclusion_zones: Vec::new(),
            rng: RandomNumberGenerator::seeded(seed),
            snapshots: Vec::new(),
        }
    }

    /// Helper for unit tests: create a small builder map with a floor interior.
    #[cfg(test)]
    pub fn with_open_room(width: i32, height: i32, seed: u64) -> Self {
        use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

        let mut ctx = Self::with_seed(1, width, height, "test", seed);
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let idx = ctx.map.xy_idx(x, y);
                ctx.map.tiles[idx] = Tile {
                    terrain: TerrainType::Floor,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
            }
        }
        ctx.rooms = Some(vec![Rect::with_size(1, 1, width - 2, height - 2)]);
        ctx
    }
}

impl BuildContext for EngineBuilderMap {
    fn map(&self) -> &Map {
        &self.map
    }
    fn map_mut(&mut self) -> &mut Map {
        &mut self.map
    }
    fn width(&self) -> i32 {
        self.width
    }
    fn height(&self) -> i32 {
        self.height
    }
    fn rooms(&self) -> Option<&Vec<Rect>> {
        self.rooms.as_ref()
    }
    fn set_rooms(&mut self, rooms: Vec<Rect>) {
        self.rooms = Some(rooms);
    }
    fn starting_position(&self) -> Option<Position> {
        self.starting_position
    }
    fn set_starting_position(&mut self, pos: Position) {
        self.starting_position = Some(pos);
    }
    fn exclusion_zones(&self) -> &[Rect] {
        &self.exclusion_zones
    }
    fn add_exclusion_zone(&mut self, rect: Rect) {
        self.exclusion_zones.push(rect);
    }
    fn rng(&mut self) -> &mut RandomNumberGenerator {
        &mut self.rng
    }
    fn take_snapshot(&mut self) {
        self.snapshots.push(self.map.clone());
    }
}

// =====================================================================
// BuilderPhase
// =====================================================================

// =====================================================================
// FloorProfile — tunables for depth-based map generation
// =====================================================================

/// Controls how organic vs. structured the generated floor feels.
///
/// Games populate this from their depth-based difficulty curve and
/// pass it to builders like `BrogueLikeBuilder` and `CaveEroder`.
#[derive(Clone, Copy, Default)]
pub struct FloorProfile {
    /// Probability weight for cavern rooms (0-100). Higher = more caves.
    pub cavern_weight: i32,
    /// Whether the first room is forced to be a large cavern.
    pub force_cavern_start: bool,
    /// Target number of rooms to place.
    pub target_rooms: i32,
    /// Hallway attachment chance (0-100).
    pub hallway_chance: i32,
    /// Erosion chance per eligible wall tile (0-100).
    pub erosion_percent: i32,
    /// Whether cavern rooms can use relaxed (no-padding) fitting.
    pub relaxed_fitting: bool,
    /// Decoration density multiplier (0.0-1.0). Scales seed count per rule.
    pub decoration_density: f32,
}

/// Logical pipeline phases. Builders declare their phase;
/// [`BuilderChain`] enforces monotonically non-decreasing phase order
/// via `assert!`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuilderPhase {
    /// Initial map generation (rooms, corridors, terrain).
    Geometry,
    /// Terrain post-processing (culling, doors, lakes).
    TerrainCleanup,
    /// Prefabs, boss rooms, connectivity validation.
    StructurePlacement,
    /// Connectivity culling (IsolatedAreaCuller).
    ConnectivityCull,
    /// Entity spawning (monsters, items, props, shrines).
    Spawning,
    /// Exit placement, decoration, final passes.
    Finalization,
}

// =====================================================================
// MapBuilder trait
// =====================================================================

/// A single step in the map generation pipeline.
///
/// Generic over `C: BuildContext` so engine builders work with any
/// context type (the engine uses `EngineBuilderMap`; games wrap it)
/// and game builders can fix `C = GameBuilderMap` to access spawn
/// lists and other game-specific fields.
pub trait MapBuilder<C: BuildContext + ?Sized>: Send + 'static {
    /// Human-readable name for timing logs.
    fn name(&self) -> &'static str;

    /// Pipeline phase this builder belongs to. Returning `None`
    /// disables phase-ordering enforcement for this step (legacy
    /// builders may not declare a phase).
    fn phase(&self) -> Option<BuilderPhase> {
        None
    }

    /// Execute the builder on the given context.
    fn build(&mut self, ctx: &mut C);
}

// =====================================================================
// BuilderChain
// =====================================================================

/// A pipeline that runs a sequence of builders on a context.
///
/// Parameterized over `C: BuildContext` so the chain holds the
/// concrete context by value. Engine tests use
/// `BuilderChain<EngineBuilderMap>`; games use
/// `BuilderChain<GameBuilderMap>`.
pub struct BuilderChain<C: BuildContext> {
    builders: Vec<(&'static str, Option<BuilderPhase>, Box<dyn FnMut(&mut C)>)>,
    pub build_data: C,
}

impl<C: BuildContext> BuilderChain<C> {
    pub fn new(ctx: C) -> Self {
        Self {
            builders: Vec::new(),
            build_data: ctx,
        }
    }

    /// Register a builder step.
    pub fn add<B: MapBuilder<C>>(&mut self, mut builder: B) -> &mut Self {
        let name = builder.name();
        let phase = builder.phase();
        self.builders
            .push((name, phase, Box::new(move |ctx: &mut C| builder.build(ctx))));
        self
    }

    /// Run all builders in sequence. Enforces monotonic phase ordering
    /// via `assert!`.
    pub fn build_map(&mut self) {
        let total_start = Instant::now();
        let mut last_phase: Option<BuilderPhase> = None;

        for (i, (name, phase, builder_fn)) in self.builders.iter_mut().enumerate() {
            let label = if name.is_empty() {
                format!("step_{}", i)
            } else {
                name.to_string()
            };

            if let Some(p) = *phase {
                if let Some(prev) = last_phase {
                    assert!(
                        p >= prev,
                        "Builder [{label}] phase {p:?} is earlier than previous phase {prev:?}. \
                         Reorder the pipeline."
                    );
                }
                last_phase = Some(p);
            }

            let start = Instant::now();
            builder_fn(&mut self.build_data);
            debug!(
                "Builder [{}]: {:.1}ms",
                label,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }

        debug!(
            "Total build_map: {:.1}ms",
            total_start.elapsed().as_secs_f64() * 1000.0
        );
    }

    /// Run all builders in sequence without phase-ordering enforcement.
    ///
    /// Prefer [`build_map`] for normal use. This variant is for pipelines
    /// that intentionally mix phases (e.g. emergency post-generation fixups).
    pub fn build_map_unchecked(&mut self) {
        let total_start = Instant::now();
        for (i, (name, _phase, builder_fn)) in self.builders.iter_mut().enumerate() {
            let label = if name.is_empty() {
                format!("step_{}", i)
            } else {
                name.to_string()
            };
            let start = Instant::now();
            builder_fn(&mut self.build_data);
            debug!(
                "Builder [{}]: {:.1}ms",
                label,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
        debug!(
            "Total build_map_unchecked: {:.1}ms",
            total_start.elapsed().as_secs_f64() * 1000.0
        );
    }

    /// Consume the chain and return the finished context.
    pub fn finish(self) -> C {
        self.build_data
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod framework_tests {
    use super::*;
    use crate::map::tile::TerrainType;

    /// A trivial engine builder for testing.
    struct FillFloor;

    impl<C: BuildContext> MapBuilder<C> for FillFloor {
        fn name(&self) -> &'static str {
            "FillFloor"
        }
        fn phase(&self) -> Option<BuilderPhase> {
            Some(BuilderPhase::Geometry)
        }
        fn build(&mut self, ctx: &mut C) {
            let w = ctx.width();
            let h = ctx.height();
            for y in 1..h - 1 {
                for x in 1..w - 1 {
                    let idx = ctx.map().xy_idx(x, y);
                    ctx.map_mut().tiles[idx].terrain = TerrainType::Floor;
                }
            }
        }
    }

    #[test]
    fn chain_runs_builder_and_modifies_map() {
        let ctx = EngineBuilderMap::with_seed(1, 10, 10, "test", 42);
        let mut chain = BuilderChain::new(ctx);
        chain.add(FillFloor);
        chain.build_map();
        let finished = chain.finish();

        // Interior should be floor
        let center = finished.map.xy_idx(5, 5);
        assert_eq!(finished.map.tiles[center].terrain, TerrainType::Floor);

        // Border should still be wall
        let corner = finished.map.xy_idx(0, 0);
        assert_eq!(finished.map.tiles[corner].terrain, TerrainType::Wall);
    }

    #[test]
    fn chain_preserves_builder_order() {
        /// A builder that sets starting_position.
        struct SetStart;
        impl<C: BuildContext> MapBuilder<C> for SetStart {
            fn name(&self) -> &'static str {
                "SetStart"
            }
            fn phase(&self) -> Option<BuilderPhase> {
                Some(BuilderPhase::TerrainCleanup)
            }
            fn build(&mut self, ctx: &mut C) {
                ctx.set_starting_position(Position { x: 3, y: 4 });
            }
        }

        let ctx = EngineBuilderMap::with_seed(1, 10, 10, "test", 42);
        let mut chain = BuilderChain::new(ctx);
        chain.add(FillFloor);
        chain.add(SetStart);
        chain.build_map();
        let finished = chain.finish();
        assert_eq!(
            finished.starting_position,
            Some(Position { x: 3, y: 4 })
        );
    }

    #[test]
    fn seeded_rng_is_deterministic() {
        fn run_chain(seed: u64) -> Vec<i32> {
            let ctx = EngineBuilderMap::with_seed(1, 5, 5, "test", seed);
            let mut chain = BuilderChain::new(ctx);

            struct RollDice;
            impl<C: BuildContext> MapBuilder<C> for RollDice {
                fn name(&self) -> &'static str {
                    "RollDice"
                }
                fn build(&mut self, ctx: &mut C) {
                    // Store dice rolls in the map's depth field (hack for testing)
                    let r1 = ctx.rng().roll_dice(1, 100);
                    let r2 = ctx.rng().roll_dice(1, 100);
                    ctx.map_mut().depth = r1 + r2;
                }
            }

            chain.add(RollDice);
            chain.build_map();
            let f = chain.finish();
            vec![f.map.depth]
        }

        let a = run_chain(42);
        let b = run_chain(42);
        let c = run_chain(99);
        assert_eq!(a, b, "same seed should produce same result");
        // Different seeds MIGHT collide but it's astronomically unlikely
        // for two rolls of 1d100+1d100. Don't assert inequality.
        let _ = c;
    }

    #[test]
    fn engine_builder_map_with_open_room_has_floor_interior() {
        let ctx = EngineBuilderMap::with_open_room(10, 10, 1);
        let center = ctx.map.xy_idx(5, 5);
        assert_eq!(ctx.map.tiles[center].terrain, TerrainType::Floor);
        assert!(ctx.rooms.is_some());
        assert_eq!(ctx.rooms.unwrap().len(), 1);
    }

    #[test]
    fn take_snapshot_captures_map_state() {
        let mut ctx = EngineBuilderMap::with_seed(1, 5, 5, "test", 42);
        assert!(ctx.snapshots.is_empty());
        ctx.take_snapshot();
        assert_eq!(ctx.snapshots.len(), 1);
        // Modify map and snapshot again
        ctx.map.tiles[0].terrain = TerrainType::Floor;
        ctx.take_snapshot();
        assert_eq!(ctx.snapshots.len(), 2);
        // First snapshot should still have Wall at [0]
        assert_eq!(
            ctx.snapshots[0].tiles[0].terrain,
            TerrainType::Wall
        );
        assert_eq!(
            ctx.snapshots[1].tiles[0].terrain,
            TerrainType::Floor
        );
    }

    #[test]
    #[should_panic(expected = "earlier than previous phase")]
    fn chain_panics_on_out_of_order_phases() {
        struct LatePhaseBuilder;
        impl<C: BuildContext> MapBuilder<C> for LatePhaseBuilder {
            fn name(&self) -> &'static str {
                "LatePhaseBuilder"
            }
            fn phase(&self) -> Option<BuilderPhase> {
                Some(BuilderPhase::Finalization)
            }
            fn build(&mut self, _ctx: &mut C) {}
        }

        struct EarlyPhaseBuilder;
        impl<C: BuildContext> MapBuilder<C> for EarlyPhaseBuilder {
            fn name(&self) -> &'static str {
                "EarlyPhaseBuilder"
            }
            fn phase(&self) -> Option<BuilderPhase> {
                Some(BuilderPhase::Geometry)
            }
            fn build(&mut self, _ctx: &mut C) {}
        }

        let ctx = EngineBuilderMap::with_seed(1, 10, 10, "test", 42);
        let mut chain = BuilderChain::new(ctx);
        chain.add(LatePhaseBuilder);
        chain.add(EarlyPhaseBuilder); // Out of order!
        chain.build_map(); // Should panic
    }

    #[test]
    fn chain_unchecked_allows_out_of_order_phases() {
        struct LatePhaseBuilder;
        impl<C: BuildContext> MapBuilder<C> for LatePhaseBuilder {
            fn name(&self) -> &'static str {
                "LatePhaseBuilder"
            }
            fn phase(&self) -> Option<BuilderPhase> {
                Some(BuilderPhase::Finalization)
            }
            fn build(&mut self, _ctx: &mut C) {}
        }

        struct EarlyPhaseBuilder;
        impl<C: BuildContext> MapBuilder<C> for EarlyPhaseBuilder {
            fn name(&self) -> &'static str {
                "EarlyPhaseBuilder"
            }
            fn phase(&self) -> Option<BuilderPhase> {
                Some(BuilderPhase::Geometry)
            }
            fn build(&mut self, _ctx: &mut C) {}
        }

        let ctx = EngineBuilderMap::with_seed(1, 10, 10, "test", 42);
        let mut chain = BuilderChain::new(ctx);
        chain.add(LatePhaseBuilder);
        chain.add(EarlyPhaseBuilder);
        chain.build_map_unchecked(); // Should NOT panic
    }
}
