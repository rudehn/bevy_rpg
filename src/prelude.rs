//! Convenience re-exports for common engine types.
//!
//! ```rust,ignore
//! use roguelike_engine::prelude::*;
//! ```
//!
//! This pulls in the types, traits, and functions that game code
//! typically needs in almost every file. For less-common items
//! (specific builders, save platform I/O, individual squad systems),
//! import from the submodule directly.

// ---- Abilities ----
pub use crate::abilities::{
    ability_action_cost, ability_aoe_tiles, is_in_ability_range, Abilities, AbilityDef,
    AbilityPlugin, AbilitySet, AbilitySlot, AbilityTarget, AbilityUseEvent, TargetingRule,
};

// ---- Components ----
pub use crate::components::{
    Collider, Faction, FactionKind, FovPlugin, FovRevealsMap, FovSet, Inventory, MovementMode,
    Name, PatrolRoute, PatrolState, Position, Viewshed, fov_update_system,
};

// ---- Combat ----
pub use crate::combat::{
    apply_damage_modifiers, apply_damage_multipliers, apply_resistance, compute_after_armor,
    DamageModifier, DamageSource, DamageType, DamageTypeTag, Health, HealthRegen,
    RegenSuppression, Resistances,
};
pub use crate::combat::events::{
    CombatEventSet, CombatPlugin, DamageEvent, DeathEvent, HealEvent,
};

// ---- Map data ----
pub use crate::map::tile::{
    can_entity_enter_tile, is_opaque, is_passable, is_pathing_blocker, is_walkable, Decoration,
    LiquidType, PromotionRule, PromotionTarget, TerrainType, Tile,
};
pub use crate::map::{populate_blocked_tiles, Map, MapWithMode};

// ---- Builder framework ----
pub use crate::map::builders::{
    BuildContext, BuilderChain, BuilderPhase, EngineBuilderMap, FloorProfile, MapBuilder,
};

// ---- Concrete builders ----
pub use crate::map::builders::brogelike::BrogueLikeBuilder;
pub use crate::map::builders::bsp_dungeon::{BspConfig, BspDungeonBuilder};
pub use crate::map::builders::cave_eroder::CaveEroder;
pub use crate::map::builders::corridors::{draw_corridor, NearestCorridors};
pub use crate::map::builders::diagonal_culler::DiagonalCuller;
pub use crate::map::builders::exit_points::DistantExit;
pub use crate::map::builders::finish_doors::FinishDoors;
pub use crate::map::builders::isolated_area_culler::IsolatedAreaCuller;
pub use crate::map::builders::lake_builder::LakeBuilder;
pub use crate::map::builders::pillar_culler::PillarCuller;
pub use crate::map::builders::room_drawer::RoomDrawer;
pub use crate::map::builders::start_point::StartPointBuilder;
pub use crate::map::builders::unseen_culler::UnseenCuller;

// ---- Turn scheduling ----
pub use crate::turn::{
    compute_reinsert_time, dequeue_next_batch_pure, DequeueOutcome, TurnManager, MAX_NPC_BATCH,
};

// ---- AI ----
pub use crate::ai::decisions::{
    advance_waypoint, flee_direction, pick_sentry_target, should_flee, should_give_up_chase,
    should_kite_retreat, should_move_erratically, should_retreat_to_squad, should_use_ranged,
    threat_priority,
};
pub use crate::ai::goap::{plan, plan_full, ActionDef, Goal, WorldState, WorldStateProp};
pub use crate::ai::pathfinding::{find_path, next_step_toward, next_step_toward_with_mode};
pub use crate::ai::{MonsterAI, MonsterAIMode, GUARD_PATROL_RADIUS};

// ---- Squad ----
pub use crate::squad::{
    AlertLevel, LeaderDeathBehavior, Morale, SquadAlertSet, SquadBlackboard, SquadConfig, SquadId,
    SquadIdCounter, SquadLeader, SquadPlugin, SquadReactionSet, SquadRole, SquadScatteredEvent,
    SquadTarget,
};

// ---- Factions ----
pub use crate::factions::{
    FactionMatrix, FactionMatrixAsset, FactionMatrixHandle, FactionsPlugin, Relation,
};

// ---- Geometry ----
pub use crate::geometry::{
    chebyshev_distance, clamp_cursor, is_adjacent, manhattan_distance, tiles_in_aoe, Direction,
};

// ---- Dice ----
pub use crate::dice::{avg_damage_from_dice, roll_dice_string};

// ---- Save framework ----
pub use crate::save::{
    apply_migrations, load_with_version, save_with_version, SaveEnvelope, SaveExists,
    SaveFrameworkConfig, SaveLoadError, SaveMigration,
};

// ---- Status effects ----
pub use crate::status::{
    compute_damage_modifier, compute_speed_modifier, StatusAppliedEvent, StatusEffectInstance,
    StatusEffectKind, StatusEffectPlugin, StatusEffectSet, StatusEffects, StatusExpiredEvent,
};

// ---- Constants ----
pub use crate::constants::{BASE_ACTION_COST, TILE_SIZE_X, TILE_SIZE_Y, Z_ITEM, Z_MONSTER, Z_PLAYER};
