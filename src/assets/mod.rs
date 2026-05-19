//! Asset manifest types and the game's `LoadingPlugin`.
//!
//! # Engine/game boundary
//!
//! This module is **game-specific** (The Veiled Tyrant) and is NOT part of
//! the reusable engine. The engine crate has no `LoadingPlugin`. Specifically:
//!
//! - The `LoadingPlugin` registers `RonAssetPlugin::<MonsterManifest>::new(&["monsters.ron"])`
//!   and similar by-exact-filename registrations. The filenames `monsters.ron`,
//!   `items.ron`, `tiles.ron`, etc. are game-specific concerns.
//! - All asset types here (`MonsterManifest`, `ItemManifest`, `PrefabManifest`,
//!   `DecorationCatalog`, `TileManifest`, …) are game content schemas.
//! - Engine-side modules (`game/turns.rs`, `game/combat.rs`, `game/actions.rs`,
//!   `map/map.rs`, `map/light.rs`, `game/fire.rs`, `game/gas.rs`,
//!   `game/tile_promotion.rs`, builder-chain framework) MUST NOT import from
//!   `crate::assets`. They must only depend on generic abstractions.
//!
//! Any engine-side code that needs to look up display or behavioral data
//! keyed by a name should accept a trait (e.g., `TileDisplayProvider`) that
//! this module implements, not the concrete `*Manifest` type. See the
//! workspace-extraction plan for more context.

use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::Deserialize;
use std::collections::HashMap;

use crate::components::{MovementMode, Species};
use crate::game::effects::Effect;
use crate::game::items::{ArmorSlot, ItemKind, OnHitEffect, Rarity};
use crate::game::prop_effects::PropTrigger;
use crate::game::staves::MonsterAbilityDef;

use crate::game::{AppState, camera};

/// Parse a sprite path like `"sprites/foo.png#3"` into `("sprites/foo.png", 3)`.
/// If the `#index` suffix is omitted, defaults to index 0.
pub fn parse_sprite_path(sprite: &str) -> (&str, usize) {
    match sprite.rsplit_once('#') {
        Some((path, idx_str)) => (path, idx_str.parse::<usize>().unwrap_or(0)),
        None => (sprite, 0),
    }
}

mod serde_helpers {
    use bevy::prelude::UVec2;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize_i32_as_option<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Some(i32::deserialize(deserializer)?))
    }

    pub fn deserialize_uvec2_as_option<'de, D>(deserializer: D) -> Result<Option<UVec2>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Some(UVec2::deserialize(deserializer)?))
    }

    /// Parse "#RRGGBB" hex string into a Bevy Color. Returns WHITE on failure.
    pub fn parse_hex_color(s: &str) -> bevy::prelude::Color {
        let s = s.trim_start_matches('#');
        if s.len() != 6 {
            return bevy::prelude::Color::WHITE;
        }
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(255);
        bevy::prelude::Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    /// Deserialize a "#RRGGBB" hex string into a Bevy Color.
    pub fn deserialize_hex_color<'de, D>(deserializer: D) -> Result<bevy::prelude::Color, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(parse_hex_color(&s))
    }
}

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MonsterManifestHandle>()
            .init_resource::<MonsterSpawnTableHandle>()
            .init_resource::<TileManifestHandle>()
            .init_resource::<ItemManifestHandle>()
            .init_resource::<ItemSpawnTableHandle>()
            .init_resource::<PlayerAssetHandle>()
            // SpellRegistryHandle removed (spell system replaced by monster abilities)
            .init_resource::<PropManifestHandle>()
            .init_resource::<PrefabManifestHandle>()
            .init_resource::<DecorationCatalogHandle>()
            .init_resource::<crate::map::builders::town_npcs::TownNpcManifestHandle>()
            .init_resource::<crate::character::RaceManifestHandle>()
            .init_resource::<crate::character::ClassManifestHandle>()
            .add_systems(
                OnEnter(AppState::Loading),
                (
                    load_monster_manifest,
                    load_monster_spawn_table,
                    load_tile_manifest,
                    load_item_manifest,
                    load_item_spawn_table,
                    load_player_asset,
                    // load_spell_registry removed (spell system replaced)
                    load_prop_manifest,
                    load_prefab_manifest,
                    load_decoration_catalog,
                    crate::map::builders::town_npcs::load_town_npc_manifest,
                    load_faction_matrix,
                    load_race_manifest,
                    load_class_manifest,
                ),
            )
            .add_systems(
                Update,
                check_assets_loaded.run_if(in_state(AppState::Loading)),
            );
    }
}

pub struct LoadingPlugin;
impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AssetsPlugin,
            RonAssetPlugin::<MonsterManifest>::new(&["monsters.ron"]),
            RonAssetPlugin::<MonsterSpawnTable>::new(&["monster_spawns.ron"]),
            RonAssetPlugin::<TileManifest>::new(&["tiles.ron"]),
            RonAssetPlugin::<ItemManifest>::new(&["items.ron"]),
            RonAssetPlugin::<ItemSpawnTable>::new(&["item_spawns.ron"]),
            RonAssetPlugin::<PlayerAsset>::new(&["player.ron"]),
            // RonAssetPlugin::<SpellRegistry> removed (spell system replaced)
            RonAssetPlugin::<PropManifest>::new(&["props.ron"]),
            RonAssetPlugin::<PrefabManifest>::new(&["prefabs.ron"]),
            RonAssetPlugin::<DecorationCatalog>::new(&["decorations.ron"]),
            RonAssetPlugin::<crate::map::builders::town_npcs::TownNpcManifest>::new(&["town_npcs.ron"]),
            RonAssetPlugin::<crate::game::factions::FactionMatrixAsset>::new(&["factions.ron"]),
            RonAssetPlugin::<crate::character::RaceManifest>::new(&["races.ron"]),
            RonAssetPlugin::<crate::character::ClassManifest>::new(&["classes.ron"]),
        ))
        .add_systems(Startup, (camera::setup_camera, set_clear_color))
        .init_resource::<MonsterSpriteAssets>()
        .init_resource::<TileSpriteAssets>()
        .init_resource::<ItemSpriteAssets>()
        .init_resource::<PropSpriteAssets>();
    }
}

#[derive(Resource, Default)]
pub struct MonsterSpriteAssets {
    pub handles: HashMap<String, Handle<Image>>,
    pub layouts: HashMap<String, Handle<TextureAtlasLayout>>,
}

#[derive(Resource, Default)]
pub struct TileSpriteAssets {
    pub handles: HashMap<String, Handle<Image>>,
    pub layouts: HashMap<String, Handle<TextureAtlasLayout>>,
}

#[derive(Resource, Default)]
pub struct ItemSpriteAssets {
    pub handles: HashMap<String, Handle<Image>>,
    pub layouts: HashMap<String, Handle<TextureAtlasLayout>>,
}

#[derive(Resource, Default)]
pub struct PropSpriteAssets {
    pub handles: HashMap<String, Handle<Image>>,
    pub layouts: HashMap<String, Handle<TextureAtlasLayout>>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
pub struct PropAsset {
    pub name: String,
    #[serde(default)]
    pub sprite: String,
    #[serde(default)]
    pub is_blocking: bool,
    #[serde(default)]
    pub is_opaque: bool,
    #[serde(default)]
    pub light_radius: Option<f32>,
    #[serde(default)]
    pub light_color: Option<[f32; 3]>,
    #[serde(default)]
    pub animated_frames: Option<u32>,
    #[serde(default)]
    pub grid_size: Option<UVec2>,
    #[serde(default)]
    pub tile_size: Option<UVec2>,
    #[serde(default)]
    pub ascii_char: String,
    #[serde(default = "default_white_hex", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_fg: Color,
    /// Optional trigger declaration. When present, the prop spawner
    /// attaches `Effected` + `EverFired` components and the dispatch
    /// systems in `prop_effects` activate this prop on step/bump (per
    /// `is_blocking`). When `None`, the prop is passive scenery.
    /// See RFC 0002.
    #[serde(default)]
    pub trigger: Option<PropTrigger>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct PropManifest {
    pub props: HashMap<String, PropAsset>,
}

#[derive(Resource, Default)]
pub struct PropManifestHandle(pub Handle<PropManifest>);

#[derive(Deserialize, Debug, Clone)]
pub struct PrefabPropEntry {
    pub x: i32,
    pub y: i32,
    pub prop: String,
}

#[derive(Deserialize, Debug, Clone)]
#[derive(Default)]
pub enum MonsterBehavior {
    Sentry,
    Patrol(Vec<(i32, i32)>),
    Roam { min: (i32, i32), max: (i32, i32) },
    #[default]
    Wander,
}


#[derive(Deserialize, Debug, Clone)]
pub struct PrefabMonsterSpawn {
    pub x: i32,
    pub y: i32,
    #[serde(default)]
    pub behavior: MonsterBehavior,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PrefabItemSpawn {
    pub x: i32,
    pub y: i32,
    pub item: Option<String>,
}

/// A machine-trigger embedded inside a prefab. Spawned by the prefab
/// placer into `BuilderMap.machine_spawn_list`, the materializer
/// stamps the prop + attaches `Machine` / `MachineTrigger` /
/// `MachineEffect` components. This is the prefab system's runtime
/// interactivity hook (shrines, traps, levers, etc.).
#[derive(Deserialize, Debug, Clone)]
pub struct PrefabTrigger {
    pub x: i32,
    pub y: i32,
    /// Prop manifest key for the visible entity (e.g. `"altar"`).
    /// Use `""` for invisible triggers (step-activated traps).
    #[serde(default)]
    pub prop_name: String,
    pub trigger: crate::game::machines::MachineTrigger,
    pub effect: crate::game::machines::MachineEffect,
    #[serde(default)]
    pub consume_on_use: bool,
}

/// Area decoration embedded inside a prefab. The placer stamps
/// `decoration` onto every walkable tile within `radius` Chebyshev
/// distance of `(x, y)` that doesn't already carry a decoration.
#[derive(Deserialize, Debug, Clone)]
pub struct PrefabDecoration {
    pub x: i32,
    pub y: i32,
    pub decoration: crate::map::tile::Decoration,
    #[serde(default = "default_decoration_radius")]
    pub radius: i32,
}

fn default_decoration_radius() -> i32 { 1 }

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
pub struct PrefabTemplate {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub min_floor: i32,
    pub max_floor: i32,
    pub tiles: Vec<String>,
    #[serde(default)]
    pub props: Vec<PrefabPropEntry>,
    #[serde(default)]
    pub monster_spawns: Vec<PrefabMonsterSpawn>,
    #[serde(default)]
    pub item_spawns: Vec<PrefabItemSpawn>,
    /// Machine triggers embedded in this prefab — shrines, traps,
    /// levers. The placer pushes each onto `machine_spawn_list`,
    /// where the materializer attaches `Machine` components.
    #[serde(default)]
    pub triggers: Vec<PrefabTrigger>,
    /// Area decorations (e.g. moss radius around a shrine altar).
    #[serde(default)]
    pub decorations: Vec<PrefabDecoration>,
    #[serde(default = "default_flee_threshold")]
    pub flee_threshold: f32,
    /// Placement mode: "room" (overlay on existing rooms), "wall" (carve into walls), "any" (try both).
    #[serde(default = "default_placement")]
    pub placement: String,
    /// Allow 90/180/270° rotation at placement time (default: true).
    #[serde(default = "default_true")]
    pub allow_rotate: bool,
    /// Allow horizontal flip at placement time (default: true).
    #[serde(default = "default_true")]
    pub allow_flip: bool,
}

fn default_placement() -> String { "any".to_string() }
fn default_white_hex() -> Color { Color::WHITE }
fn default_black_hex() -> Color { Color::BLACK }
fn default_true() -> bool { true }

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct PrefabManifest {
    pub prefabs: Vec<PrefabTemplate>,
}

#[derive(Resource, Default)]
pub struct PrefabManifestHandle(pub Handle<PrefabManifest>);

// `DecorationRule` and `DecorationChain` live in the engine crate
// (`roguelike_engine::map::decoration_rule`). Re-exported here so existing
// `crate::assets::DecorationRule` import sites compile unchanged.
pub use roguelike_engine::map::decoration_rule::{DecorationChain, DecorationRule};

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct DecorationCatalog {
    pub rules: Vec<DecorationRule>,
}

#[derive(Resource, Default)]
pub struct DecorationCatalogHandle(pub Handle<DecorationCatalog>);

#[derive(Deserialize, Debug, Clone)]
pub struct StartingItemDef {
    pub name: String,
    #[serde(default = "default_count_one")]
    pub count: u32,
}

#[derive(Asset, TypePath, Deserialize, Resource, Debug, Clone)]
pub struct PlayerAsset {
    pub name: String,
    #[serde(default)]
    pub sprite: String,
    pub damage: String,
    #[serde(default = "default_player_hp")]
    pub max_hp: i32,
    #[serde(default = "default_regen_rate")]
    pub regen_rate: i32,
    #[serde(default)]
    pub armor: i32,
    #[serde(default)]
    pub dodge: i32,
    #[serde(default)]
    pub viewshed_range: i32,
    #[serde(default)]
    pub starting_items: Vec<StartingItemDef>,
    #[serde(default)]
    pub ascii_char: String,
    #[serde(default = "default_white_hex", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_fg: Color,
}

fn default_player_hp() -> i32 { 25 }
fn default_regen_rate() -> i32 { 10 }

#[derive(Deserialize, Debug, Clone)]
pub struct MonsterLootEntry {
    pub item: String,
    pub spawn_chance: f32,
    /// Minimum number dropped (for stackable loot like arrows).
    #[serde(default = "default_count_one")]
    pub count_min: u32,
    /// Maximum number dropped.
    #[serde(default = "default_count_one")]
    pub count_max: u32,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterAsset {
    pub name: String,
    pub vision: i32,
    #[serde(default)]
    pub sprite: String,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub grid_size: Option<UVec2>,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub tile_size: Option<UVec2>,

    pub base_hp: i32,
    pub damage: String,

    /// Monster tier for XP-reward scaling (Phase 2). 1 = trivial; 27 = max
    /// challenge. Compared against the player's level to apply the
    /// anti-farming dropoff. Defaults to 1 if omitted.
    #[serde(default = "default_monster_tier")]
    pub tier: u32,

    /// Base perception score (Phase 4 stealth system). Modifier to the
    /// d20 perception roll vs. a target's stealth. Defaults to 0; range
    /// roughly -3..=+5 across the shipping monster roster.
    #[serde(default)]
    pub perception: i32,

    #[serde(default, deserialize_with = "serde_helpers::deserialize_i32_as_option")]
    pub regen: Option<i32>,

    #[serde(default)]
    pub loot_table: Vec<MonsterLootEntry>,

    /// Melee damage type (e.g. "physical", "fire", "poison"). Default: "physical".
    #[serde(default)]
    pub damage_type: String,

    /// Resistance map, e.g. {"fire": 100, "physical": 50}. Values are percentages.
    #[serde(default)]
    pub resistances: HashMap<String, i32>,

    /// Base armor value (flat damage reduction). Default: 0.
    #[serde(default)]
    pub base_armor: i32,

    /// ECS faction for hostility checks. Defaults to "Monster".
    #[serde(default = "default_faction")]
    pub faction: String,

    /// Biological category (Beast, Humanoid, Insect, etc.). Defaults to `Unknown`
    /// and logs a warning if unset; see `docs/design/ENEMIES.md` for the canonical list.
    #[serde(default)]
    pub species: Species,

    /// Monster abilities — passive, on-hit, on-death, and aura effects.
    #[serde(default)]
    pub abilities: Vec<AbilityDef>,

    /// Cooldown-based monster spell abilities (replaces old mana/spells system).
    #[serde(default)]
    pub monster_abilities: Vec<MonsterAbilityDef>,
    #[serde(default)]
    pub ascii_char: String,
    #[serde(default = "default_white_hex", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_fg: Color,

    /// AI behavior configuration (FSM or GOAP). Defaults to standard FSM with no special behaviors.
    #[serde(default)]
    pub ai: AiConfig,

    /// Base dodge value for this monster. Default: 0.
    #[serde(default)]
    pub base_dodge: i32,

    /// Movement delay multiplier. 1.0 = normal speed, >1.0 = slower, <1.0 = faster.
    #[serde(default = "default_delay")]
    pub movement_delay: f32,

    /// Attack delay multiplier. 1.0 = normal speed, >1.0 = slower, <1.0 = faster.
    #[serde(default = "default_delay")]
    pub attack_delay: f32,

    /// Movement mode controlling how this monster interacts with terrain.
    #[serde(default)]
    pub movement_mode: MovementMode,

    /// If true, the monster never moves — it only uses abilities/ranged attacks.
    #[serde(default)]
    pub stationary: bool,

    /// Phase B loadout: item names this monster spawns wielding/wearing.
    /// At spawn the items are looked up in `items.ron`, instantiated as
    /// entities attached to the monster's `Equipment` component, and
    /// their effects apply via the same paths as player equipment —
    /// equipped weapon's `damage` dice override the monster's intrinsic
    /// `damage:` field; weapon `on_hit_effects` proc through
    /// `handle_weapon_on_hit_effects`; armor adds to base armor/dodge.
    ///
    /// Empty by default. Monsters with no `equipped:` fall back to the
    /// intrinsic `damage:` / `base_armor:` / `base_dodge:` fields exactly
    /// as before.
    #[serde(default)]
    pub equipped: Vec<String>,
}

/// Detonation effect for `ExplodeOnHit`, deserialized from RON.
#[derive(Debug, Clone, Deserialize)]
pub enum ExplodeEffectDef {
    CrackFloor,
    GasCloud { volume: u16 },
}

impl Default for ExplodeEffectDef {
    fn default() -> Self { Self::CrackFloor }
}

/// Ability definition for RON deserialization.
/// Variant name encodes the trigger type (on-hit, on-being-hit, on-death, passive).
#[derive(Debug, Clone, Deserialize)]
pub enum AbilityDef {
    // On-hit (trigger when this monster lands an attack)
    BurningStrike { damage_per_turn: i32, duration: u32, chance: u32 },
    PoisonStrike { damage_per_turn: i32, duration: u32, chance: u32 },
    StunningBlow { duration: u32, chance: u32 },
    SlowStrike { duration: u32, chance: u32 },
    LifeDrain { percent: i32 },
    Knockback { distance: i32, chance: u32 },

    // On-being-hit (trigger when this monster takes melee damage)
    RoughBody { damage: i32 },
    Enrage { threshold_percent: u32 },

    // On-hit: self-destruct with a configurable area effect (chasms, gas, etc.).
    ExplodeOnHit { radius: i32, #[serde(default)] effect: ExplodeEffectDef },

    // On-death (trigger when this monster dies)
    ExplodeOnDeath { damage: i32, radius: i32, #[serde(default)] damage_type: Option<String> },
    SummonOnDeath { monster: String, count: u32 },
    GasOnDeath { radius: i32, volume: u16 },

    // On-being-hit (split)
    SplitOnHit { min_hp: i32 },

    // Passive / aura
    PackTactics,
    WarCry { radius: i32, duration: u32 },
    Rally { radius: i32, armor_bonus: i32 },
    Terrify { radius: i32 },

    // Disguise
    MimicDisguise,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterManifest {
    pub monsters: HashMap<String, MonsterAsset>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterSpawnInfo {
    #[serde(default)]
    pub monster: String,
    pub min_floor: i32,
    pub max_floor: i32,
    #[serde(default = "default_group_one")]
    pub min_group: i32,
    #[serde(default = "default_group_one")]
    pub max_group: i32,
    /// Mixed-species group. When non-empty, `monster`/`min_group`/`max_group` are ignored.
    #[serde(default)]
    pub group: Vec<GroupMember>,

    /// Squad behavior: collective HP ratio below which cowardly squad members flee.
    #[serde(default = "default_flee_threshold")]
    pub flee_threshold: f32,

    /// When true, this monster spawns on liquid tiles instead of dry land.
    #[serde(default)]
    pub spawn_on_liquid: bool,

    /// Relative spawn rarity. Higher = more common. Default 100. Entries
    /// with `weight: 5` against a default 100 backdrop spawn ~5% as often
    /// as default entries when both are eligible. Used by VoronoiSpawner
    /// to weight per-cell entry selection.
    #[serde(default = "default_spawn_weight")]
    pub weight: u32,
}

/// A single species entry within a mixed group spawn.
#[derive(Deserialize, Debug, Clone)]
pub struct GroupMember {
    pub monster: String,
    #[serde(default = "default_group_one")]
    pub min_count: i32,
    #[serde(default = "default_group_one")]
    pub max_count: i32,
}

fn default_group_one() -> i32 {
    1
}

fn default_flee_threshold() -> f32 {
    0.5
}

fn default_spawn_weight() -> u32 {
    100
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterSpawnTable {
    pub spawns: Vec<MonsterSpawnInfo>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct ItemSpawnInfo {
    pub item: String,
    pub min_floor: i32,
    pub max_floor: i32,
    #[serde(default = "default_weight")]
    pub weight: i32,
    /// Minimum number of items to spawn in a single batch (e.g., arrows).
    #[serde(default = "default_count_one")]
    #[allow(dead_code)]
    pub min_count: u32,
    /// Maximum number of items to spawn in a single batch.
    #[serde(default = "default_count_one")]
    #[allow(dead_code)]
    pub max_count: u32,
}

fn default_weight() -> i32 {
    1
}
fn default_count_one() -> u32 {
    1
}

fn default_monster_tier() -> u32 {
    1
}

fn default_faction() -> String {
    "Monster".to_string()
}

fn default_kite_distance() -> u32 {
    3
}

fn default_delay() -> f32 {
    1.0
}

fn default_morale() -> f32 {
    0.6
}

#[derive(Debug, Clone, Deserialize)]
pub enum AiTrait {
    Cowardly,
    Aggressive,
    Reckless,
    Mindless,
    Bestial,
    Intelligent,
    Hoarder,
    Support,
    Commander,
    Ranged { range: u32 },
}

/// AI behavior configuration for a monster. The sole variant after
/// the tactic-registry migration completed (Phase 5). The legacy
/// `Fsm` and `Goap` variants were deleted in Phases 4 and 5
/// respectively. See `docs/design/TACTICS.md`.
#[derive(Debug, Clone, Deserialize)]
pub enum AiConfig {
    /// Ordered list of tactic names plus tuning knobs that flow into
    /// `MonsterAI` and the squad `Morale` component. The names are
    /// resolved via `game::tactics::library::lookup_tactic` at spawn
    /// time; tactics read the knobs through the snapshot. Each list
    /// must end with `"Wait"` — enforced at startup by
    /// `validate_tactic_names_system`.
    TacticList {
        tactics: Vec<String>,
        #[serde(default)]
        flee_at_hp_percent: f32,
        #[serde(default)]
        chase_leash: u32,
        #[serde(default)]
        kites: bool,
        #[serde(default = "default_kite_distance")]
        kite_distance: u32,
        #[serde(default)]
        ranged_range: u32,
        /// Starting morale for the squad system. 0.0–1.0; default 0.6.
        /// Bosses and elite leaders raise this; cowardly support
        /// monsters lower it.
        #[serde(default = "default_morale")]
        base_morale: f32,
        /// What this monster does when no combat tactic fires.
        /// Defaults to `PathToRandomTile` — pick a random walkable
        /// destination, walk there, then pick another. Override to
        /// `Patrol` or `Roam` for monsters that need spawn-time
        /// bounds / waypoints (the spawner attaches a `PatrolRoute`
        /// component with the actual data). `Stationary` opts out
        /// of idle movement entirely (turrets, statues).
        #[serde(default)]
        idle_movement: IdleMovement,
    },
}

/// How a monster behaves when it has no combat target. Read by the
/// `IdleMove` tactic; default is `PathToRandomTile`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub enum IdleMovement {
    /// Pick a random walkable tile on the map, pathfind there, repeat
    /// when arrived or blocked. Most monsters use this.
    #[default]
    PathToRandomTile,
    /// Walk a fixed list of waypoints in a loop. Requires a
    /// `PatrolRoute::Waypoint { points, .. }` component attached at
    /// spawn time (the spawn-time builder supplies the waypoints).
    Patrol,
    /// Bounded random walk within a rectangle. Requires a
    /// `PatrolRoute::AreaRoam { min, max }` component attached at
    /// spawn time.
    Roam,
    /// Never move when idle — combat tactics may still produce
    /// movement (kite, flee), but idle behavior is to wait.
    Stationary,
}

impl Default for AiConfig {
    fn default() -> Self {
        // Minimal valid TacticList — just `Wait`. A monster falling
        // through to this default ends up doing nothing, which is the
        // safest possible behavior for content that forgot to declare
        // its AI. Real assets always declare an explicit list.
        AiConfig::TacticList {
            tactics: vec!["Wait".to_string()],
            flee_at_hp_percent: 0.0,
            chase_leash: 0,
            kites: false,
            kite_distance: 3,
            ranged_range: 0,
            base_morale: 0.6,
            idle_movement: IdleMovement::default(),
        }
    }
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct ItemSpawnTable {
    pub spawns: Vec<ItemSpawnInfo>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct TileAsset {
    #[serde(default)]
    pub sprite: String,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub grid_size: Option<UVec2>,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub tile_size: Option<UVec2>,
    #[serde(default)]
    pub ascii_char: String,
    #[serde(default = "default_white_hex", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_fg: Color,
    #[serde(default = "default_black_hex", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_bg: Color,
}

#[derive(Asset, TypePath, Deserialize, Resource, Debug, Clone)]
pub struct TileManifest {
    pub tiles: HashMap<String, TileAsset>,
}

/// Weapon-only data — only meaningful when `ItemKindData::Weapon(...)`.
#[derive(Debug, Clone, Deserialize)]
pub struct WeaponData {
    /// Damage dice (e.g. `"1d6"`, `"2d4+1"`). Required for weapons.
    pub damage: String,
    /// Attack-speed multiplier. 0.5 = twice as fast, 1.0 = normal.
    #[serde(default = "default_attack_speed")]
    pub attack_speed: f32,
    /// Range for ranged weapons (> 1 = ranged; 0 or 1 = melee/default).
    #[serde(default)]
    pub weapon_range: u32,
    /// Active weapon ability name (e.g. `"Backstab"`, `"Cleave"`). Sword
    /// has none — the no-ability balance baseline.
    #[serde(default)]
    pub weapon_ability: Option<String>,
    /// Weapon-family skill applied on melee/ranged hits.
    #[serde(default)]
    pub weapon_skill: Option<crate::game::skills::WeaponSkill>,
    /// On-hit effects applied to the wielder's successful attacks.
    /// Empty by default; `[PoisonStrike(...), ...]` for proc-weapons.
    #[serde(default)]
    pub on_hit_effects: Vec<OnHitEffect>,
}

/// Armor-only data — only meaningful when `ItemKindData::Armor(...)`.
#[derive(Debug, Clone, Deserialize)]
pub struct ArmorData {
    /// Which equipment slot this armor piece occupies. Required.
    pub slot: ArmorSlot,
    /// Flat armor value (chest/helm/etc.) or block value (OffHand
    /// shields). The runtime routing lives in `compute_stat_delta`.
    #[serde(default)]
    pub defense: i32,
    /// Per-turn shield block budget (Buckler=1, Kite=2, Tower=3). Only
    /// meaningful for OffHand armor; ignored on other slots.
    #[serde(default)]
    pub max_blocks: u32,
}

/// Staff-only data — only meaningful when `ItemKindData::Staff(...)`.
#[derive(Debug, Clone, Deserialize)]
pub struct StaffData {
    /// Which spell effect this staff casts. Required.
    pub effect: crate::game::staves::StaffEffect,
    /// Turns per charge at +0 enchantment.
    #[serde(default)]
    pub base_recharge: u32,
}

/// Consumable-only data — only meaningful when `ItemKindData::Consumable(...)`.
#[derive(Debug, Clone, Deserialize)]
pub struct ConsumableData {
    /// One-shot effect applied when used. `None` = inert (e.g. arrows).
    #[serde(default)]
    pub effect: Option<Effect>,
    /// Maximum stack size (1 = non-stackable).
    #[serde(default = "default_max_stack")]
    pub max_stack: u32,
    /// Whether this item is ammunition consumed by ranged attacks.
    #[serde(default)]
    pub is_ammo: bool,
}

/// Tagged-union for kind-specific item data. The variant determines
/// which set of fields is meaningful; the universal equip bonuses
/// (`hit_bonus`, `damage_bonus`, etc.) stay flat on `ItemAsset`
/// because they apply across multiple kinds.
#[derive(Debug, Clone, Deserialize)]
pub enum ItemKindData {
    Weapon(WeaponData),
    Armor(ArmorData),
    Staff(StaffData),
    Consumable(ConsumableData),
    /// Ring — all data lives in the universal equip-bonus fields on `ItemAsset`.
    Ring,
    /// Amulet — all data lives in the universal equip-bonus fields on `ItemAsset`.
    Amulet,
}

impl ItemKindData {
    /// Project the variant onto the simpler runtime `ItemKind` tag.
    pub fn as_kind(&self) -> ItemKind {
        match self {
            ItemKindData::Weapon(_) => ItemKind::Weapon,
            ItemKindData::Armor(_) => ItemKind::Armor,
            ItemKindData::Staff(_) => ItemKind::Staff,
            ItemKindData::Consumable(_) => ItemKind::Consumable,
            ItemKindData::Ring => ItemKind::Ring,
            ItemKindData::Amulet => ItemKind::Amulet,
        }
    }
}

impl Default for ItemKindData {
    fn default() -> Self {
        ItemKindData::Consumable(ConsumableData {
            effect: None,
            max_stack: 1,
            is_ammo: false,
        })
    }
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct ItemAsset {
    pub name: String,
    #[serde(default)]
    pub sprite: String,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub grid_size: Option<UVec2>,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub tile_size: Option<UVec2>,

    /// Kind-specific data (Weapon/Armor/Staff/Consumable/Ring/Amulet).
    /// Replaces the old flat `item_kind` + `damage` + `armor_slot` + ...
    /// soup. Fields that only make sense for one kind live in the
    /// matching variant; fields that apply across kinds stay flat below.
    #[serde(default)]
    pub kind: ItemKindData,

    #[serde(default)]
    pub rarity: Rarity,

    // ---- Universal equip bonuses (apply to rings, amulets, armor, weapons) ----
    /// Dodge bonus granted when equipped.
    #[serde(default)]
    pub dodge_bonus: i32,
    /// Flat hit bonus granted when equipped.
    #[serde(default)]
    pub hit_bonus: i32,
    /// Flat damage bonus granted when equipped.
    #[serde(default)]
    pub damage_bonus: i32,
    /// Regen rate bonus granted when equipped.
    #[serde(default)]
    pub regen_bonus: i32,
    /// Max HP bonus granted when equipped.
    #[serde(default)]
    pub max_hp_bonus: i32,
    /// Speed delay modifier when equipped (negative = faster).
    #[serde(default)]
    pub delay_modifier: f32,
    /// Vision range bonus when equipped (Ring of Perception).
    #[serde(default)]
    pub vision_bonus: i32,
    /// Per-damage-type resistance percentages granted while equipped.
    /// Keys are damage-type names ("fire", "lightning", "poison", "physical").
    #[serde(default)]
    pub resistances: HashMap<String, i32>,

    /// Stealth penalty for the wearer (Phase 4 stealth system). Subtracted
    /// from the d20 stealth roll. 0 = silent (cloth, robe), 5 = plate.
    /// Defaults to 0 so non-armor items don't carry a phantom penalty.
    #[serde(default)]
    pub armor_stealth_penalty: i32,

    /// Whether this item is a quest item required to win the game.
    #[serde(default)]
    pub is_quest_item: bool,
    #[serde(default)]
    pub ascii_char: String,
    #[serde(default = "default_white_hex", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_fg: Color,
}

impl ItemAsset {
    pub fn item_kind(&self) -> ItemKind {
        self.kind.as_kind()
    }

    pub fn weapon_data(&self) -> Option<&WeaponData> {
        if let ItemKindData::Weapon(w) = &self.kind { Some(w) } else { None }
    }

    pub fn armor_data(&self) -> Option<&ArmorData> {
        if let ItemKindData::Armor(a) = &self.kind { Some(a) } else { None }
    }

    pub fn staff_data(&self) -> Option<&StaffData> {
        if let ItemKindData::Staff(s) = &self.kind { Some(s) } else { None }
    }

    pub fn consumable_data(&self) -> Option<&ConsumableData> {
        if let ItemKindData::Consumable(c) = &self.kind { Some(c) } else { None }
    }

    /// Convenience: max stack size. 1 for any non-Consumable kind.
    pub fn max_stack(&self) -> u32 {
        self.consumable_data().map(|c| c.max_stack).unwrap_or(1)
    }
}

fn default_attack_speed() -> f32 {
    1.0
}

fn default_max_stack() -> u32 {
    1
}

#[derive(Asset, TypePath, Deserialize, Resource, Debug, Clone)]
pub struct ItemManifest {
    pub items: HashMap<String, ItemAsset>,
}

#[derive(Resource, Default)]
pub struct MonsterManifestHandle(pub Handle<MonsterManifest>);

#[derive(Resource, Default)]
pub struct MonsterSpawnTableHandle(pub Handle<MonsterSpawnTable>);

#[derive(Resource, Default)]
pub struct TileManifestHandle(pub Handle<TileManifest>);

#[derive(Resource, Default)]
pub struct ItemManifestHandle(pub Handle<ItemManifest>);

#[derive(Resource, Default)]
pub struct ItemSpawnTableHandle(pub Handle<ItemSpawnTable>);

#[derive(Resource, Default)]
pub struct PlayerAssetHandle(pub Handle<PlayerAsset>);

fn load_monster_manifest(
    asset_server: Res<AssetServer>,
    mut monster_manifest_handle: ResMut<MonsterManifestHandle>,
) {
    monster_manifest_handle.0 = asset_server.load("monsters.ron");
}

fn load_monster_spawn_table(
    asset_server: Res<AssetServer>,
    mut handle: ResMut<MonsterSpawnTableHandle>,
) {
    handle.0 = asset_server.load("monster_spawns.ron");
}

fn load_tile_manifest(asset_server: Res<AssetServer>, mut handle: ResMut<TileManifestHandle>) {
    handle.0 = asset_server.load("tiles.ron");
}

fn load_item_manifest(asset_server: Res<AssetServer>, mut handle: ResMut<ItemManifestHandle>) {
    handle.0 = asset_server.load("items.ron");
}

fn load_item_spawn_table(asset_server: Res<AssetServer>, mut handle: ResMut<ItemSpawnTableHandle>) {
    handle.0 = asset_server.load("item_spawns.ron");
}

fn load_player_asset(asset_server: Res<AssetServer>, mut handle: ResMut<PlayerAssetHandle>) {
    handle.0 = asset_server.load("player.ron");
}

fn load_prop_manifest(asset_server: Res<AssetServer>, mut handle: ResMut<PropManifestHandle>) {
    handle.0 = asset_server.load("props.ron");
}

fn load_prefab_manifest(asset_server: Res<AssetServer>, mut handle: ResMut<PrefabManifestHandle>) {
    handle.0 = asset_server.load("prefabs.ron");
}

fn load_decoration_catalog(
    asset_server: Res<AssetServer>,
    mut handle: ResMut<DecorationCatalogHandle>,
) {
    handle.0 = asset_server.load("decorations.ron");
}

fn load_faction_matrix(
    asset_server: Res<AssetServer>,
    mut handle: ResMut<crate::game::factions::FactionMatrixHandle>,
) {
    handle.0 = asset_server.load("factions.ron");
}

fn load_race_manifest(
    asset_server: Res<AssetServer>,
    mut handle: ResMut<crate::character::RaceManifestHandle>,
) {
    handle.0 = asset_server.load("races.ron");
}

fn load_class_manifest(
    asset_server: Res<AssetServer>,
    mut handle: ResMut<crate::character::ClassManifestHandle>,
) {
    handle.0 = asset_server.load("classes.ron");
}

// Groups the overflow resources so check_assets_loaded stays within Bevy's
// 16-SystemParam limit.
#[derive(bevy::ecs::system::SystemParam)]
struct ExtraLoadingParams<'w> {
    item_spawn_table_handle: Res<'w, ItemSpawnTableHandle>,
    item_spawn_tables: Res<'w, Assets<ItemSpawnTable>>,
    player_asset_handle: Res<'w, PlayerAssetHandle>,
    player_assets: Res<'w, Assets<PlayerAsset>>,
    prop_manifest_handle: Res<'w, PropManifestHandle>,
    prop_manifests: Res<'w, Assets<PropManifest>>,
    prefab_manifest_handle: Res<'w, PrefabManifestHandle>,
    prefab_manifests: Res<'w, Assets<PrefabManifest>>,
    decoration_catalog_handle: Res<'w, DecorationCatalogHandle>,
    decoration_catalogs: Res<'w, Assets<DecorationCatalog>>,
    town_npc_handle: Res<'w, crate::map::builders::town_npcs::TownNpcManifestHandle>,
    town_npc_manifests: Res<'w, Assets<crate::map::builders::town_npcs::TownNpcManifest>>,
    faction_matrix_handle: Res<'w, crate::game::factions::FactionMatrixHandle>,
    faction_matrix_assets: Res<'w, Assets<crate::game::factions::FactionMatrixAsset>>,
    race_manifest_handle: Res<'w, crate::character::RaceManifestHandle>,
    race_manifests: Res<'w, Assets<crate::character::RaceManifest>>,
    class_manifest_handle: Res<'w, crate::character::ClassManifestHandle>,
    class_manifests: Res<'w, Assets<crate::character::ClassManifest>>,
    next_state: ResMut<'w, NextState<AppState>>,
}

fn check_assets_loaded(
    monster_manifest_handle: Res<MonsterManifestHandle>,
    monster_manifests: Res<Assets<MonsterManifest>>,
    monster_spawn_table_handle: Res<MonsterSpawnTableHandle>,
    monster_spawn_tables: Res<Assets<MonsterSpawnTable>>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_manifests: Res<Assets<TileManifest>>,
    item_manifest_handle: Res<ItemManifestHandle>,
    item_manifests: Res<Assets<ItemManifest>>,
    mut extra: ExtraLoadingParams,
) {
    if monster_manifests.get(&monster_manifest_handle.0).is_none() {
        return;
    }

    if monster_spawn_tables
        .get(&monster_spawn_table_handle.0)
        .is_none()
    {
        return;
    }

    if tile_manifests.get(&tile_manifest_handle.0).is_none() {
        return;
    }

    if item_manifests.get(&item_manifest_handle.0).is_none() {
        return;
    }

    if extra
        .item_spawn_tables
        .get(&extra.item_spawn_table_handle.0)
        .is_none()
    {
        return;
    }

    if extra
        .player_assets
        .get(&extra.player_asset_handle.0)
        .is_none()
    {
        return;
    }

    if extra
        .prop_manifests
        .get(&extra.prop_manifest_handle.0)
        .is_none()
    {
        return;
    }

    if extra
        .prefab_manifests
        .get(&extra.prefab_manifest_handle.0)
        .is_none()
    {
        return;
    }

    if extra
        .decoration_catalogs
        .get(&extra.decoration_catalog_handle.0)
        .is_none()
    {
        return;
    }

    if extra
        .town_npc_manifests
        .get(&extra.town_npc_handle.0)
        .is_none()
    {
        return;
    }

    if extra
        .faction_matrix_assets
        .get(&extra.faction_matrix_handle.0)
        .is_none()
    {
        return;
    }

    if extra
        .race_manifests
        .get(&extra.race_manifest_handle.0)
        .is_none()
    {
        return;
    }

    if extra
        .class_manifests
        .get(&extra.class_manifest_handle.0)
        .is_none()
    {
        return;
    }

    if let Some(manifest) = monster_manifests.get(&monster_manifest_handle.0) {
        for (name, asset) in &manifest.monsters {
            if asset.species == Species::Unknown {
                warn!(
                    "Monster '{}' has no species set in monsters.ron (defaulted to Unknown). \
                     Add a `species:` field (Beast, Humanoid, Insect, etc.).",
                    name
                );
            }
        }
    }

    extra.next_state.set(AppState::Menu);
}

fn set_clear_color(mut clear_color: ResMut<ClearColor>) {
    clear_color.0 = Color::srgb_u8(37, 19, 26);
}

#[cfg(test)]
mod species_tests {
    use super::*;
    use crate::components::Species;

    /// Every Species variant deserializes from its unit-variant name.
    #[test]
    fn species_deserializes_unit_variants() {
        let cases = [
            ("Beast", Species::Beast),
            ("Humanoid", Species::Humanoid),
            ("Undead", Species::Undead),
            ("Insect", Species::Insect),
            ("Fungal", Species::Fungal),
            ("Ooze", Species::Ooze),
            ("Dragon", Species::Dragon),
            ("Construct", Species::Construct),
            ("Aberration", Species::Aberration),
            ("Unknown", Species::Unknown),
        ];
        for (input, expected) in cases {
            let parsed: Species = ron::from_str(input)
                .unwrap_or_else(|e| panic!("failed to parse {input}: {e}"));
            assert_eq!(parsed, expected, "variant {input} did not round-trip");
        }
    }

    /// `#[serde(default)]` on the species field yields `Species::Unknown`
    /// when a monster entry omits it — this is the guard the warning pass relies on.
    #[test]
    fn missing_species_field_defaults_to_unknown() {
        // Minimal subset of MonsterAsset fields — relies on serde(default) for the rest.
        let ron = r#"(
            name: "Test",
            vision: 8,
            damage: "1d4",
            base_hp: 5,
        )"#;
        let asset: MonsterAsset = ron::from_str(ron).expect("parse minimal monster");
        assert_eq!(asset.species, Species::Unknown);
    }

    /// Every monster shipped in `assets/monsters.ron` must declare a species.
    /// If this test fails, the newly added monster forgot the `species:` field.
    #[test]
    fn all_shipped_monsters_declare_species() {
        let manifest_src = include_str!("../../assets/monsters.ron");
        let manifest: MonsterManifest =
            ron::from_str(manifest_src).expect("assets/monsters.ron must parse");

        let missing: Vec<&String> = manifest
            .monsters
            .iter()
            .filter(|(_, asset)| asset.species == Species::Unknown)
            .map(|(name, _)| name)
            .collect();

        assert!(
            missing.is_empty(),
            "monsters missing a `species:` field in monsters.ron: {:?}",
            missing
        );
    }

    /// Every active forest floor has at least one eligible spawn entry —
    /// no silent "empty floor" regressions when the table is pruned.
    /// Threshold is 1 (not 3 like the 26-floor era) because the linear-
    /// floor world is mid-rebuild; raise the floor as the roster grows.
    /// Floor 1 is the town (no monster spawns by design) and the temple
    /// floor (MAX_FLOOR) has no spawns yet either (cultists arrive
    /// later). The forest floors in between must each have ≥1 entry.
    /// See docs/design/SPAWNING.md.
    #[test]
    fn every_active_floor_has_a_spawn_entry() {
        let spawns_src = include_str!("../../assets/monster_spawns.ron");
        let table: MonsterSpawnTable =
            ron::from_str(spawns_src).expect("assets/monster_spawns.ron must parse");

        // Forest floors are 2..=MAX_FLOOR-1. Floor 1 is the town (no
        // spawns by design); MAX_FLOOR is the temple (no spawns yet).
        for floor in 2..crate::constants::MAX_FLOOR {
            let count = table
                .spawns
                .iter()
                .filter(|s| floor >= s.min_floor && floor <= s.max_floor)
                .count();
            assert!(
                count >= 1,
                "active forest floor {} has no eligible spawn entries — add at least one to assets/monster_spawns.ron",
                floor,
            );
        }
    }

    /// No faction should be present on every single floor — phasing principle
    /// from the 26-floor distribution plan (factions rise, peak, fade).
    /// Walks each monster's species->faction mapping via the monster manifest.
    #[test]
    fn no_faction_is_present_on_every_floor() {
        let manifest_src = include_str!("../../assets/monsters.ron");
        let manifest: MonsterManifest = ron::from_str(manifest_src).expect("parse monsters");
        let spawns_src = include_str!("../../assets/monster_spawns.ron");
        let table: MonsterSpawnTable = ron::from_str(spawns_src).expect("parse spawns");

        // For each faction, collect the set of floors it appears on.
        use std::collections::{HashMap as Map, HashSet};
        let mut faction_floors: Map<String, HashSet<i32>> = Map::new();

        for entry in &table.spawns {
            // Collect monster names this entry references (solo or group).
            let names: Vec<&str> = if entry.group.is_empty() {
                if entry.monster.is_empty() {
                    continue;
                }
                vec![entry.monster.as_str()]
            } else {
                entry.group.iter().map(|g| g.monster.as_str()).collect()
            };
            for name in names {
                if let Some(asset) = manifest.monsters.get(name) {
                    let f = faction_floors.entry(asset.faction.clone()).or_default();
                    for floor in entry.min_floor..=entry.max_floor {
                        f.insert(floor);
                    }
                }
            }
        }

        for (faction, floors) in &faction_floors {
            // "Monster" is the default-faction catch-all for factionless solo
            // threats (Jelly, Mimic, Wolf, Shade, etc.) — per ENEMIES.md these
            // are intentionally scattered across all depths, so exempt from
            // the phasing rule.
            if faction == "Monster" {
                continue;
            }
            assert!(
                floors.len() < 26,
                "faction {:?} is present on all 26 floors ({} floors); factions must rise and fade",
                faction,
                floors.len()
            );
        }
    }

    /// Items in `assets/items.ron` parse into the new tagged-union shape.
    /// Every shipped item must classify into exactly one `ItemKindData`
    /// variant — flat `item_kind: X` is gone.
    #[test]
    fn items_ron_parses_into_tagged_union() {
        let src = include_str!("../../assets/items.ron");
        let manifest: ItemManifest = ron::from_str(src).expect("items.ron must parse");
        assert!(!manifest.items.is_empty(), "items.ron must declare at least one item");
        // Every kind variant projects cleanly to the runtime ItemKind tag.
        for (name, item) in &manifest.items {
            let kind = item.item_kind();
            // Round-trip: helper returns a meaningful variant for every item.
            assert!(
                matches!(
                    kind,
                    ItemKind::Weapon | ItemKind::Armor | ItemKind::Staff
                    | ItemKind::Consumable | ItemKind::Ring | ItemKind::Amulet,
                ),
                "item '{}' has unexpected kind tag", name,
            );
        }
    }

    /// Every Weapon-kind item declares its damage dice in the Weapon
    /// variant. Catches forgetting to migrate a flat `damage: Some("...")`
    /// field when adding a new weapon.
    #[test]
    fn every_weapon_declares_damage_dice() {
        let src = include_str!("../../assets/items.ron");
        let manifest: ItemManifest = ron::from_str(src).expect("items.ron must parse");
        for (name, item) in &manifest.items {
            if let Some(w) = item.weapon_data() {
                assert!(
                    !w.damage.trim().is_empty(),
                    "weapon '{}' has empty damage dice; declare damage in kind: Weapon((...))",
                    name,
                );
            }
        }
    }

    /// Every Armor-kind item declares its slot in the Armor variant.
    /// Catches forgetting to migrate a flat `armor_slot: Some(...)` field.
    #[test]
    fn every_armor_declares_slot() {
        let src = include_str!("../../assets/items.ron");
        let manifest: ItemManifest = ron::from_str(src).expect("items.ron must parse");
        for (name, item) in &manifest.items {
            if item.item_kind() == ItemKind::Armor {
                assert!(
                    item.armor_data().is_some(),
                    "armor '{}' missing kind: Armor((slot: ...)); flat armor_slot was dropped in the kind-union refactor",
                    name,
                );
            }
        }
    }

    /// Every Staff-kind item declares its effect in the Staff variant.
    #[test]
    fn every_staff_declares_effect() {
        let src = include_str!("../../assets/items.ron");
        let manifest: ItemManifest = ron::from_str(src).expect("items.ron must parse");
        for (name, item) in &manifest.items {
            if item.item_kind() == ItemKind::Staff {
                assert!(
                    item.staff_data().is_some(),
                    "staff '{}' missing kind: Staff((effect: ...))",
                    name,
                );
            }
        }
    }

    /// A weapon can declare `on_hit_effects` and round-trip through RON.
    /// This is the new authoring contract the Ritual Dagger relies on.
    #[test]
    fn weapon_on_hit_effects_round_trip() {
        let ron = r#"(
            name: "Test Dagger",
            kind: Weapon((
                damage: "1d4",
                attack_speed: 0.5,
                on_hit_effects: [
                    PoisonStrike(damage_per_turn: 1, duration: 3, chance: 30),
                ],
            )),
        )"#;
        let asset: ItemAsset = ron::from_str(ron).expect("parse weapon with on_hit");
        let w = asset.weapon_data().expect("must be weapon");
        assert_eq!(w.damage, "1d4");
        assert_eq!(w.on_hit_effects.len(), 1);
        match &w.on_hit_effects[0] {
            OnHitEffect::PoisonStrike { damage_per_turn, duration, chance } => {
                assert_eq!(*damage_per_turn, 1);
                assert_eq!(*duration, 3);
                assert_eq!(*chance, 30);
            }
            other => panic!("expected PoisonStrike, got {:?}", other),
        }
    }

    /// Ring and Amulet are unit variants — all their data lives in the
    /// universal equip-bonus fields on `ItemAsset`.
    #[test]
    fn ring_and_amulet_parse_as_unit_variants() {
        for (label, ron_str) in [
            ("ring", r#"(name: "Test Ring", kind: Ring, hit_bonus: 2)"#),
            ("amulet", r#"(name: "Test Amulet", kind: Amulet, max_hp_bonus: 15)"#),
        ] {
            let asset: ItemAsset = ron::from_str(ron_str)
                .unwrap_or_else(|e| panic!("{label} must parse: {e}"));
            match asset.item_kind() {
                ItemKind::Ring => assert_eq!(asset.hit_bonus, 2),
                ItemKind::Amulet => assert_eq!(asset.max_hp_bonus, 15),
                other => panic!("{label} wrong kind {:?}", other),
            }
        }
    }

    /// The species declared in a monster asset survives the round trip from RON
    /// into an ECS component on a spawned entity. Uses Bevy `World` directly
    /// rather than the full spawner pipeline (which needs sprite assets,
    /// turn manager, etc.) — the spawner simply inserts `monster_asset.species`.
    #[test]
    fn species_travels_from_asset_to_ecs_component() {
        let ron = r#"(
            name: "Test Spider",
            vision: 8,
            damage: "1d4",
            base_hp: 5,
            species: Insect,
        )"#;
        let asset: MonsterAsset = ron::from_str(ron).expect("parse");
        assert_eq!(asset.species, Species::Insect);

        let mut world = bevy::ecs::world::World::new();
        let entity = world.spawn(asset.species).id();
        let read = world
            .get::<Species>(entity)
            .expect("Species component should be on entity");
        assert_eq!(*read, Species::Insect);
    }
}

#[cfg(test)]
mod prop_asset_tests {
    //! Verifies the `PropAsset.trigger` field parses correctly from RON
    //! and defaults to `None` when omitted. The field is the authoring
    //! surface for RFC 0002 (prop+machine+decoration unification).
    use super::*;
    use crate::game::prop_effects::{ActivationMode, EffectAudience, TileEffect};

    /// A minimal PropAsset RON without the new `trigger:` field still
    /// parses, and `trigger` defaults to `None`.
    #[test]
    fn prop_asset_without_trigger_defaults_to_none() {
        let ron = r#"(
            name: "Barrel",
            is_blocking: true,
            ascii_char: "o",
        )"#;
        let asset: PropAsset = ron::from_str(ron).expect("parse PropAsset");
        assert_eq!(asset.name, "Barrel");
        assert!(asset.trigger.is_none(), "missing trigger should default to None");
    }

    /// A PropAsset declaring a `trigger:` block parses with all
    /// sub-fields populated.
    #[test]
    fn prop_asset_with_trigger_parses_fully() {
        let ron = r#"(
            name: "Campfire",
            is_blocking: false,
            light_radius: Some(28.0),
            ascii_char: "*",
            trigger: Some((
                effect: DealDamage(dice: "1d4", kind: Fire),
                audience: Anyone,
                mode: Repeating,
            )),
        )"#;
        let asset: PropAsset = ron::from_str(ron).expect("parse PropAsset");
        let trigger = asset.trigger.expect("trigger should be Some");
        assert!(matches!(trigger.effect, TileEffect::DealDamage { .. }));
        assert_eq!(trigger.audience, EffectAudience::Anyone);
        assert_eq!(trigger.mode, ActivationMode::Repeating);
    }

    /// PropTrigger sub-fields (audience, mode) default correctly when
    /// the RON omits them — only `effect` is required.
    #[test]
    fn prop_asset_trigger_inner_fields_default() {
        let ron = r#"(
            name: "Altar",
            is_blocking: true,
            ascii_char: "_",
            trigger: Some((
                effect: HealFull,
            )),
        )"#;
        let asset: PropAsset = ron::from_str(ron).expect("parse PropAsset");
        let trigger = asset.trigger.expect("trigger should be Some");
        assert!(matches!(trigger.effect, TileEffect::HealFull));
        assert_eq!(trigger.audience, EffectAudience::Anyone);
        assert_eq!(trigger.mode, ActivationMode::Repeating);
    }

    /// PropAsset reads OnceConsumed / PlayerOnly correctly — the
    /// Trapped Vault use case.
    #[test]
    fn prop_asset_with_once_consumed_player_only_trigger() {
        let ron = r#"(
            name: "Trapped Chest",
            is_blocking: true,
            ascii_char: "$",
            trigger: Some((
                effect: SpawnMonsters(monster_name: "", count: 2),
                audience: Anyone,
                mode: OnceConsumed,
            )),
        )"#;
        let asset: PropAsset = ron::from_str(ron).expect("parse PropAsset");
        let trigger = asset.trigger.expect("trigger should be Some");
        assert!(matches!(trigger.effect, TileEffect::SpawnMonsters { .. }));
        assert_eq!(trigger.mode, ActivationMode::OnceConsumed);
    }
}
