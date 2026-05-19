//! Decoration spawn rules consumed by [`DecorationPropagator`].
//!
//! Pure data types describing where and how a [`Decoration`] may seed and
//! propagate during map generation. Loaded from RON by the game (via
//! `bevy_common_assets`); the engine itself does no asset I/O.
//!
//! [`DecorationPropagator`]: crate::map::builders::decoration_propagator::DecorationPropagator

use serde::Deserialize;

use crate::map::tile::{Decoration, TerrainType};

/// Optional secondary decoration that a propagation step may pick instead
/// of the rule's primary decoration. Lets a single rule paint visually
/// varied clusters (e.g. a fungus patch with occasional dead-fungus tiles).
#[derive(Deserialize, Debug, Clone)]
pub struct DecorationChain {
    pub decoration: Decoration,
    pub chance: f32,
}

/// One decoration spawn rule.
///
/// Floor range gates eligibility, seed counts control how many BFS roots
/// are placed, and the `propagation_*` fields tune the BFS spread.
#[derive(Deserialize, Debug, Clone)]
pub struct DecorationRule {
    #[allow(dead_code)]
    pub name: String,
    pub min_floor: i32,
    pub max_floor: i32,
    pub min_seeds: i32,
    pub max_seeds: i32,
    pub decoration: Decoration,
    pub requires_terrain: Vec<TerrainType>,
    #[serde(default)]
    pub propagation_chance: f32,
    #[serde(default)]
    pub propagation_decay: f32,
    #[serde(default)]
    pub max_propagation_depth: i32,
    #[serde(default)]
    pub wall_adjacent_only: bool,
    #[serde(default)]
    pub corner_only: bool,
    #[serde(default)]
    pub requires_nearby_liquid: bool,
    #[serde(default)]
    pub chain: Option<DecorationChain>,
}
