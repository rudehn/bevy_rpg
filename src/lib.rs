//! # roguelike_engine
//!
//! A reusable engine for turn-based grid roguelikes, built on
//! [Bevy](https://bevyengine.org/) 0.17. Provides the plumbing every
//! roguelike needs — turn scheduling, combat math, map generation,
//! AI primitives, squad coordination, save I/O — while leaving
//! content and balance to the game.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use roguelike_engine::prelude::*;
//!
//! // Generate a dungeon
//! let ctx = EngineBuilderMap::with_seed(1, 80, 60, "Floor 1", 42);
//! let mut chain = BuilderChain::new(ctx);
//! chain.add(roguelike_engine::map::builders::brogelike::BrogueLikeBuilder::dungeon(
//!     1, 80, 60, FloorProfile { cavern_weight: 30, ..Default::default() },
//! ));
//! chain.add(roguelike_engine::map::builders::start_point::StartPointBuilder::new());
//! chain.add(roguelike_engine::map::builders::diagonal_culler::DiagonalCuller::new());
//! chain.add(roguelike_engine::map::builders::finish_doors::FinishDoors::new());
//! chain.add(roguelike_engine::map::builders::exit_points::DistantExit::new());
//! chain.build_map();
//! let finished = chain.finish();
//! // finished.map is a ready-to-play Map
//! ```
//!
//! # What the engine ships
//!
//! | Module | What it provides |
//! |--------|-----------------|
//! | [`map`] | `Map` resource, `Tile`/`TerrainType`/`LiquidType`/`Decoration` types, `BaseMap`/`Algorithm2D` bracket-lib integration |
//! | [`map::builders`] | `BuildContext` trait, `BuilderChain<C>`, `MapBuilder<C>`, 13 pure builders (BrogueLike, BSP, cullers, corridors, lakes, doors, exit placement, ...) |
//! | [`combat`] | `DamageType`, `Resistances`, `Health`, `compute_after_armor`, `apply_resistance`, `apply_damage_multipliers` |
//! | [`turn`] | `TurnManager`, `dequeue_next_batch_pure`, `compute_reinsert_time` |
//! | [`ai`] | `MonsterAI` state machine data, pure decision helpers, GOAP planner (`WorldState`, `Goal`, `ActionDef`, `plan`) |
//! | [`squad`] | `SquadPlugin` with alert propagation, leader-death effects, shared morale, tactical blackboard |
//! | [`factions`] | `FactionMatrix` (string-keyed, symmetric, data-driven from RON), `FactionsPlugin` |
//! | [`components`] | `Position`, `Viewshed`, `Name`, `Inventory`, `Faction`, `MovementMode`, `Collider`, `PatrolRoute` |
//! | [`geometry`] | `Direction`, Manhattan/Chebyshev distance, adjacency, AoE tiles |
//! | [`dice`] | `roll_dice_string`, `avg_damage_from_dice` (wraps bracket-lib) |
//! | [`save`] | Platform-agnostic save I/O (native RON + WASM localStorage), `SaveFrameworkConfig` |
//! | [`constants`] | Tile size, Z-layers, base action cost |
//!
//! # What the engine does NOT ship
//!
//! - Monsters, items, spells, or any content
//! - Ability/spell systems
//! - Asset manifest schemas or RON file names
//! - UI (inventory screens, character sheets, etc.)
//! - Rendering (the engine is headless; games own their sprites)
//!
//! # Extension points
//!
//! All enums are `#[non_exhaustive]` with `Custom { id: u32 }` variants
//! so games can extend the engine's blessed set without forking:
//!
//! - `TerrainType::Custom`, `LiquidType::Custom`, `Decoration::Custom`
//! - `DamageType::Custom`, `DamageSource::Custom`
//! - `WorldStateProp::Custom`, `MovementMode::Custom`
//! - `StatusEffectKind::Custom` (in the game crate, not yet engine-side)
//!
//! Bevy plugins expose empty `SystemSet` markers (`SquadAlertSet`,
//! `SquadReactionSet`, `CombatReactionSet`) that games configure with
//! `.after()` / `.before()` / `.run_if()` — the engine never names
//! game-side systems.

pub mod ai;
pub mod combat;
pub mod components;
pub mod constants;
pub mod dice;
pub mod factions;
pub mod geometry;
pub mod map;
pub mod prelude;
pub mod save;
pub mod squad;
pub mod turn;
