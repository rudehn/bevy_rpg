//! Game-side adapter for the engine's tile promotion module.
//!
//! The tick system, [`PromotionCooldown`] resource, and the
//! [`TilePromotionPlugin`] all live in `roguelike_engine::map::promotion`.
//! Re-exported here so existing
//! `crate::game::tile_promotion::PromotionCooldown` import sites compile
//! unchanged.

pub use roguelike_engine::map::promotion::{
    tile_promotion_tick_system, PromotionCooldown, TilePromotionPlugin, TilePromotionSet,
};
