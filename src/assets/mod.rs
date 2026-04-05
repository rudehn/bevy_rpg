use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::Deserialize;
use std::collections::HashMap;

use crate::components::MovementMode;
use crate::game::effects::Effect;
use crate::game::items::{ArmorSlot, ItemKind, Rarity};
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
                    load_faction_matrix,
                ),
            )
            .add_systems(
                Update,
                (
                    load_monster_sprites,
                    load_tile_sprites,
                    load_item_sprites,
                    load_prop_sprites,
                    check_assets_loaded,
                )
                    .run_if(in_state(AppState::Loading)),
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
            RonAssetPlugin::<crate::game::factions::FactionMatrixAsset>::new(&["factions.ron"]),
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
    /// Combat role to resolve via faction table (e.g. "melee_guard", "ranged", "caster", "leader", "brute").
    pub role: String,
    #[serde(default)]
    pub behavior: MonsterBehavior,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PrefabItemSpawn {
    pub x: i32,
    pub y: i32,
    pub item: Option<String>,
}

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
    #[serde(default)]
    pub on_leader_death: String,
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

#[derive(Deserialize, Debug, Clone)]
pub struct DecorationChain {
    pub decoration: crate::map::tile::Decoration,
    pub chance: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DecorationRule {
    #[allow(dead_code)]
    pub name: String,
    pub min_floor: i32,
    pub max_floor: i32,
    pub min_seeds: i32,
    pub max_seeds: i32,
    pub decoration: crate::map::tile::Decoration,
    pub requires_terrain: Vec<crate::map::tile::TerrainType>,
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
}

/// Infer the combat role of a monster from its asset data (replaces the old
/// explicit `role` field). Used by prefab placement for faction-role resolution.
pub fn infer_role(asset: &MonsterAsset) -> &'static str {
    // Check if any ability implies leadership
    if asset.abilities.iter().any(|a| matches!(a, AbilityDef::WarCry { .. } | AbilityDef::Rally { .. })) {
        return "leader";
    }
    // Check for ranged from AI config
    let has_ranged = match &asset.ai {
        AiConfig::Fsm { ranged_range, .. } => *ranged_range > 0,
        AiConfig::Goap { traits, .. } => traits.iter().any(|t| matches!(t, AiTrait::Ranged { .. })),
    };
    if !asset.monster_abilities.is_empty() && !has_ranged { return "caster"; }
    if has_ranged { return "ranged"; }
    if asset.base_armor >= 2 { return "brute"; }
    "melee_guard"
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

    // On-death (trigger when this monster dies)
    ExplodeOnDeath { damage: i32, radius: i32, #[serde(default)] damage_type: Option<String> },
    SummonOnDeath { monster: String, count: u32 },

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

    /// Squad behavior: what happens when the leader dies ("scatter", "enrage", or "" for nothing).
    #[serde(default)]
    pub on_leader_death: String,

    /// Squad behavior: collective HP ratio below which cowardly squad members flee.
    #[serde(default = "default_flee_threshold")]
    pub flee_threshold: f32,

    /// When true, this monster spawns on liquid tiles instead of dry land.
    #[serde(default)]
    pub spawn_on_liquid: bool,
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

/// AI behavior configuration for a monster. Either a standard FSM or GOAP planner.
#[derive(Debug, Clone, Deserialize)]
pub enum AiConfig {
    /// Standard 3-state FSM (Asleep/Hunting/Idle) with reactive behaviors.
    Fsm {
        #[serde(default)]
        flee_at_hp_percent: f32,
        #[serde(default)]
        erratic_chance: f32,
        #[serde(default)]
        chase_leash: u32,
        #[serde(default)]
        kites: bool,
        #[serde(default = "default_kite_distance")]
        kite_distance: u32,
        #[serde(default)]
        ranged_range: u32,
    },
    /// Goal-Oriented Action Planning with trait-based goals/actions.
    Goap {
        /// Behavioral traits that drive goal/action generation.
        traits: Vec<AiTrait>,
        /// Initial morale value (0.0-1.0).
        #[serde(default = "default_morale")]
        base_morale: f32,
    },
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig::Fsm {
            flee_at_hp_percent: 0.0,
            erratic_chance: 0.0,
            chase_leash: 0,
            kites: false,
            kite_distance: 3,
            ranged_range: 0,
        }
    }
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct ItemSpawnTable {
    pub spawns: Vec<ItemSpawnInfo>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct TileAsset {
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

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct ItemAsset {
    pub name: String,
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
    pub item_kind: ItemKind,
    #[serde(default)]
    pub armor_slot: Option<ArmorSlot>,
    #[serde(default)]
    pub damage: Option<String>,
    #[serde(default)]
    pub defense: i32,
    #[serde(default)]
    pub rarity: Rarity,
    #[serde(default)]
    pub effect: Option<Effect>,
    /// Range for ranged weapons (> 1 = ranged; 0 or 1 = melee/default).
    #[serde(default)]
    pub weapon_range: u32,
    /// Attack speed multiplier (0.5 = twice as fast, 1.0 = normal). Defaults to 1.0.
    #[serde(default = "default_attack_speed")]
    pub attack_speed: f32,
    /// Staff effect type (only for Staff items).
    #[serde(default)]
    pub staff_effect: Option<crate::game::staves::StaffEffect>,
    /// Base recharge rate for staves (turns per charge at +0 enchantment).
    #[serde(default)]
    pub base_recharge: u32,
    /// Maximum number of items that can share one inventory slot (1 = not stackable).
    #[serde(default = "default_max_stack")]
    pub max_stack: u32,
    /// Whether this item is ammunition (consumed by ranged attacks).
    #[serde(default)]
    pub is_ammo: bool,
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
    /// Speed delay modifier when equipped (negative = faster, positive = slower).
    #[serde(default)]
    pub delay_modifier: f32,
    /// Active weapon ability name (e.g. "Backstab", "Riposte").
    #[serde(default)]
    pub weapon_ability: Option<String>,
    /// Whether this item is a quest item required to win the game.
    #[serde(default)]
    pub is_quest_item: bool,
    #[serde(default)]
    pub ascii_char: String,
    #[serde(default = "default_white_hex", deserialize_with = "serde_helpers::deserialize_hex_color")]
    pub ascii_fg: Color,
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

/// Collects sprite entries that need loading: `(texture_path, tile_size, grid_size)`.
/// Used to build a deduped list before inserting into the sprite asset maps.
struct SpriteEntry {
    key: String,
    tile_size: UVec2,
    grid_size: UVec2,
}

/// Shared sprite loading logic. For each manifest entry, loads the image texture
/// and creates a `TextureAtlasLayout` if not already present in `existing_keys`.
fn load_sprite_entries(
    entries: impl Iterator<Item = (String, UVec2, UVec2)>,
    existing_keys: &HashMap<String, Handle<Image>>,
) -> Vec<SpriteEntry> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (sprite_path, tile_size, grid_size) in entries {
        let (texture_path, _) = parse_sprite_path(&sprite_path);
        let key = texture_path.to_string();
        if !existing_keys.contains_key(&key) && seen.insert(key.clone()) {
            result.push(SpriteEntry { key, tile_size, grid_size });
        }
    }
    result
}

fn apply_sprite_entries(
    entries: Vec<SpriteEntry>,
    handles: &mut HashMap<String, Handle<Image>>,
    layouts: &mut HashMap<String, Handle<TextureAtlasLayout>>,
    asset_server: &AssetServer,
    atlas_layouts: &mut Assets<TextureAtlasLayout>,
) {
    for entry in entries {
        handles.insert(entry.key.clone(), asset_server.load::<Image>(entry.key.clone()));
        layouts.insert(
            entry.key,
            atlas_layouts.add(TextureAtlasLayout::from_grid(
                entry.tile_size,
                entry.grid_size.x,
                entry.grid_size.y,
                None,
                None,
            )),
        );
    }
}

fn load_monster_sprites(
    asset_server: Res<AssetServer>,
    manifest_handle: Res<MonsterManifestHandle>,
    manifests: Res<Assets<MonsterManifest>>,
    mut sprites: ResMut<MonsterSpriteAssets>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    if let Some(manifest) = manifests.get(&manifest_handle.0) {
        let new = load_sprite_entries(
            manifest.monsters.values().map(|a| (
                a.sprite.clone(),
                a.tile_size.unwrap_or(UVec2::new(32, 32)),
                a.grid_size.unwrap_or(UVec2::new(1, 1)),
            )),
            &sprites.handles,
        );
        let s = &mut *sprites;
        apply_sprite_entries(new, &mut s.handles, &mut s.layouts, &asset_server, &mut layouts);
    }
}

fn load_tile_sprites(
    asset_server: Res<AssetServer>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_manifests: Res<Assets<TileManifest>>,
    player_asset_handle: Res<PlayerAssetHandle>,
    player_assets: Res<Assets<PlayerAsset>>,
    mut sprites: ResMut<TileSpriteAssets>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // Collect all entries from both tile manifest and player asset before mutating.
    let mut all_entries: Vec<(String, UVec2, UVec2)> = Vec::new();
    if let Some(manifest) = tile_manifests.get(&tile_manifest_handle.0) {
        all_entries.extend(manifest.tiles.values().map(|a| (
            a.sprite.clone(),
            a.tile_size.unwrap_or(UVec2::new(16, 16)),
            a.grid_size.unwrap_or(UVec2::new(1, 1)),
        )));
    }
    // Player sprite loaded into tile assets (player.ron may load after tile manifest).
    if let Some(player_asset) = player_assets.get(&player_asset_handle.0) {
        all_entries.push((player_asset.sprite.clone(), UVec2::new(32, 32), UVec2::new(1, 1)));
    }
    let new = load_sprite_entries(all_entries.into_iter(), &sprites.handles);
    let s = &mut *sprites;
    apply_sprite_entries(new, &mut s.handles, &mut s.layouts, &asset_server, &mut layouts);
}

fn load_item_sprites(
    asset_server: Res<AssetServer>,
    manifest_handle: Res<ItemManifestHandle>,
    manifests: Res<Assets<ItemManifest>>,
    mut sprites: ResMut<ItemSpriteAssets>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    if let Some(manifest) = manifests.get(&manifest_handle.0) {
        let new = load_sprite_entries(
            manifest.items.values().map(|a| (
                a.sprite.clone(),
                a.tile_size.unwrap_or(UVec2::new(32, 32)),
                a.grid_size.unwrap_or(UVec2::new(1, 1)),
            )),
            &sprites.handles,
        );
        let s = &mut *sprites;
        apply_sprite_entries(new, &mut s.handles, &mut s.layouts, &asset_server, &mut layouts);
    }
}

fn load_prop_sprites(
    asset_server: Res<AssetServer>,
    manifest_handle: Res<PropManifestHandle>,
    manifests: Res<Assets<PropManifest>>,
    mut sprites: ResMut<PropSpriteAssets>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    if let Some(manifest) = manifests.get(&manifest_handle.0) {
        let new = load_sprite_entries(
            manifest.props.values().map(|a| (
                a.sprite.clone(),
                a.tile_size.unwrap_or(UVec2::new(16, 16)),
                a.grid_size.unwrap_or(UVec2::new(4, 1)),
            )),
            &sprites.handles,
        );
        let s = &mut *sprites;
        apply_sprite_entries(new, &mut s.handles, &mut s.layouts, &asset_server, &mut layouts);
    }
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
    prop_sprite_assets: Res<'w, PropSpriteAssets>,
    prefab_manifest_handle: Res<'w, PrefabManifestHandle>,
    prefab_manifests: Res<'w, Assets<PrefabManifest>>,
    decoration_catalog_handle: Res<'w, DecorationCatalogHandle>,
    decoration_catalogs: Res<'w, Assets<DecorationCatalog>>,
    faction_matrix_handle: Res<'w, crate::game::factions::FactionMatrixHandle>,
    faction_matrix_assets: Res<'w, Assets<crate::game::factions::FactionMatrixAsset>>,
    next_state: ResMut<'w, NextState<AppState>>,
}

fn check_assets_loaded(
    asset_server: Res<AssetServer>,
    monster_manifest_handle: Res<MonsterManifestHandle>,
    monster_manifests: Res<Assets<MonsterManifest>>,
    monster_spawn_table_handle: Res<MonsterSpawnTableHandle>,
    monster_spawn_tables: Res<Assets<MonsterSpawnTable>>,
    monster_sprite_assets: Res<MonsterSpriteAssets>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_manifests: Res<Assets<TileManifest>>,
    tile_sprite_assets: Res<TileSpriteAssets>,
    item_manifest_handle: Res<ItemManifestHandle>,
    item_manifests: Res<Assets<ItemManifest>>,
    item_sprite_assets: Res<ItemSpriteAssets>,
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
        .faction_matrix_assets
        .get(&extra.faction_matrix_handle.0)
        .is_none()
    {
        return;
    }

    if monster_sprite_assets.handles.is_empty()
        || tile_sprite_assets.handles.is_empty()
        || item_sprite_assets.handles.is_empty()
    {
        return;
    }

    for handle in monster_sprite_assets.handles.values() {
        if !asset_server.is_loaded_with_dependencies(handle) {
            return;
        }
    }

    for handle in tile_sprite_assets.handles.values() {
        if !asset_server.is_loaded_with_dependencies(handle) {
            return;
        }
    }

    for handle in item_sprite_assets.handles.values() {
        if !asset_server.is_loaded_with_dependencies(handle) {
            return;
        }
    }

    for handle in extra.prop_sprite_assets.handles.values() {
        if !asset_server.is_loaded_with_dependencies(handle) {
            return;
        }
    }

    extra.next_state.set(AppState::Menu);
}

fn set_clear_color(mut clear_color: ResMut<ClearColor>) {
    clear_color.0 = Color::srgb_u8(37, 19, 26);
}
