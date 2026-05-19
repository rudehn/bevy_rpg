//! Map data structures, tile representations, and generation framework.

pub mod builders;
pub mod decoration_rule;
pub mod map;
pub mod mutation;
pub mod promotion;
pub mod tile;
pub mod tile_entity_index;

pub use self::decoration_rule::{DecorationChain, DecorationRule};
pub use self::map::{Map, MapWithMode, populate_blocked_tiles};
pub use self::mutation::{
    apply_decoration_mutations, apply_liquid_mutations, apply_tile_mutations,
    DecorationMutationMessage, LiquidMutationMessage, MapMutationPlugin, MapMutationSet,
    TileMutationMessage,
};
pub use self::promotion::{
    tile_promotion_tick_system, PromotionCooldown, TilePromotionPlugin, TilePromotionSet,
};
pub use self::tile_entity_index::TileEntityIndex;
