//! Faction hostility matrix — game-side re-export.
//!
//! The full implementation now lives in `roguelike_engine::factions`.
//! Veiled Tyrant's specific faction roster is defined in
//! `assets/factions.ron`; the engine ships only the type machinery.
//!
//! This module exists so existing game code that imports from
//! `crate::game::factions::*` (spawner, AI, squad, assets/mod.rs)
//! continues to work unchanged.

pub use roguelike_engine::factions::{
    apply_faction_matrix_asset, FactionMatrix, FactionMatrixAsset, FactionMatrixHandle,
    FactionRelationEntry, FactionsPlugin, Relation,
};
