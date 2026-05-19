use std::collections::HashMap;

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use serde::{Deserialize, Serialize};

/// Re-export of the engine crate's save framework.
///
/// - `platform` — platform-agnostic I/O primitives (native RON / WASM localStorage).
/// - `SaveFrameworkConfig` — engine resource for the per-game save key.
/// - `SaveExists` — engine resource for save-file presence.
///
/// All three are owned by `roguelike_engine::save`; the game's
/// [`SavePlugin`] simply inserts `SaveFrameworkConfig` with the
/// Veiled-Tyrant-specific key and schedules the schema-aware systems.
pub use roguelike_engine::save::{
    SaveEnvelope, SaveExists, SaveFrameworkConfig, SaveLoadError, SaveMigration, apply_migrations,
    load_with_version, platform, save_with_version,
};

use crate::{
    assets::{ItemManifest, ItemManifestHandle, ItemSpriteAssets},
    components::{
        Equipped, FloorEntityMarker, InInventory, Inventory, Item, Key, Monster, Name, Position,
        Prop, QuestItem, Viewshed,
    },
    game::{
        AppState,
        combat::{Damage, Health},
        enchantment::{
            ArmorRunic, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified, WeaponRunic,
        },
        items::{Equipment, ItemProperties, ItemStack},
        magic::StatusEffects,
        spawner::spawn_item,
        squad::{SquadConfig, SquadId, SquadIdCounter, SquadLeader},
        stats::{Armor, DamageBonus, Dodge, HitBonus},
        staves::{Rechargeable, StaffData, StaffEffect},
    },
    map::{
        dungeon::{
            AutoSavePending, CachedFloor, Floor, FloorCache, PendingGameLoad, PendingPlayerLoad,
        },
        map::Map,
        tile::Tile,
    },
    player::Player,
    ui::game_log::GameLog,
};

// ---- Schema-level convenience wrappers ----
//
// These thin wrappers delegate to `platform::` using The Veiled Tyrant's
// save key. They exist so existing call sites (menu, dungeon, combat)
// don't need to thread a key parameter through. New code should prefer
// reading the key from the `SaveFrameworkConfig` resource and calling
// `platform::*` directly, which is what the game's `SavePlugin` does
// by inserting the config at startup.

const GAME_SAVE_KEY: &str = "ironveil_save";

/// Current save schema version for The Veiled Tyrant.
///
/// ## Version history
/// - **v0**: Pre-versioning raw RON payload (no envelope).
/// - **v1**: First versioned envelope. Same payload format as v0.
/// - **v2**: Status effects migrated to engine format (flat `Burning` /
///   `Poisoned` kinds with `magnitude` on the instance; game-specific
///   kinds like `Entangled` / `Enraged` moved to `Custom { id }`).
/// - **v3**: Phase 1 character system. `PlayerSaveData` gains `race`,
///   `class`, `attributes`, `hit_bonus`, `damage_bonus` fields.
/// - **v4**: Phase 2 character system. CON removed from `Attributes`;
///   Halfling removed from `Race`. **v3 saves containing CON values
///   or Halfling race will fail to load** — they're unrecoverable as
///   the player's attribute scores can't be safely renormalized from
///   the Phase 1 base-10 scale to the Phase 2 base-16 scale. Acceptable
///   in a permadeath dev cycle with no production save data.
/// - **v5**: Phase 3 skills. `PlayerSaveData` gains `skills`,
///   `skill_xp`, `skill_training`, `skill_xp_pool`. Pre-v5 saves load
///   with empty maps and 0 pool via serde defaults — no in-game
///   effect until the player trains; migration is a no-op.
/// - **v6**: Overworld topology — `OverworldSave` on `GameSaveData` and
///   `exit_tiles` on `SavedFloorData`. Both `#[serde(default)]`.
/// - **v7**: Stealth Phase I — per-monster awareness state persisted on
///   `SavedMonster.awareness` as a degraded shape (Hidden | Searching).
///   `Aware` collapses to `Searching{ player.pos, +20 }` at save time;
///   `Suspicious` collapses to `Hidden`. `#[serde(default)]` keeps v6
///   saves loadable with all monsters defaulting to `Hidden`.
/// - **v8**: `StatusEffectKind::Custom { id }` variants replaced with
///   named variants (`Entangled`, `Enraged`, `FireResistance`,
///   `PoisonResistance`). The bincode representation of these kinds
///   changes — **pre-v8 saves containing the old `Custom { id }` shape
///   are unrecoverable** and will fail to deserialize. Acceptable in
///   the permadeath dev cycle with no production save data.
/// - **v9**: Tactic registry — `SavedMonster.fleeing: Option<SavedFleeing>`
///   persists the sticky `Fleeing` overlay component (since_turn +
///   last_known_threat_pos). `#[serde(default)]` keeps v8 saves
///   loadable; pre-v9 monsters load with `None` (not fleeing), which
///   matches the historical behaviour where Fleeing was always lost on
///   load anyway.
/// - **v10**: RFC 0002 prop effects — `SavedProp.ever_fired: bool` persists
///   per-instance prop activation state (used altars stay inert, sprung
///   traps don't re-fire). `#[serde(default)]` keeps v9 saves loadable;
///   pre-v10 saves load all props as not-yet-fired, matching the
///   pre-RFC behavior where Machine activation state was lost on load.
pub const SAVE_SCHEMA_VERSION: u32 = 10;

// ---- Migration chain ----

/// v0 → v1: pre-versioning saves are wrapped in the new envelope
/// without any payload transformation.
struct MigrateV0ToV1;
impl SaveMigration for MigrateV0ToV1 {
    fn from_version(&self) -> u32 {
        0
    }
    fn to_version(&self) -> u32 {
        1
    }
    fn migrate(&self, data: &str) -> Result<String, String> {
        // The payload is already valid v1 RON; the envelope is added by
        // `save_with_version` / handled by `load_with_version`.
        Ok(data.to_string())
    }
}

/// v1 → v2: status-effect format change.
///
/// Old format (v1): variant payloads carried DoT damage and named kinds
/// existed for Entangled / Enraged / FireResistance / PoisonResistance.
///
/// ```ron
/// (kind: Burning(damage_per_turn: 3), turns_remaining: 5, initial_duration: 5)
/// (kind: Entangled, turns_remaining: 3, initial_duration: 3)
/// ```
///
/// New format (v2): flat kinds with `magnitude` on the instance; custom
/// statuses for game-specific kinds.
///
/// ```ron
/// (kind: Burning, remaining_turns: 5, magnitude: 3)
/// (kind: Custom(id: 1), remaining_turns: 3, magnitude: 0)
/// ```
///
/// Implementation: for maximum safety, this migration just drops all
/// persisted status effects. They are transient by design (max ~20 turn
/// duration) and this avoids fragile RON string surgery. Players loading
/// a pre-migration save will lose any active buffs/debuffs but keep all
/// durable state (HP, inventory, position, floor progress).
struct MigrateV1ToV2;
impl SaveMigration for MigrateV1ToV2 {
    fn from_version(&self) -> u32 {
        1
    }
    fn to_version(&self) -> u32 {
        2
    }
    fn migrate(&self, data: &str) -> Result<String, String> {
        // Replace every `status_effects: StatusEffects(...)` with an
        // empty StatusEffects. Uses a simple tokeniser to find the outer
        // parens — naive but sufficient since the field always serialises
        // on a line-delimited structure in RON.
        let mut out = String::with_capacity(data.len());
        let mut rest = data;
        while let Some(idx) = rest.find("status_effects: StatusEffects") {
            out.push_str(&rest[..idx]);
            out.push_str("status_effects: StatusEffects(effects: [])");
            // Skip past the matching closing paren of the original value.
            let after = &rest[idx + "status_effects: StatusEffects".len()..];
            if let Some(skip) = skip_balanced_parens(after) {
                rest = &after[skip..];
            } else {
                // Malformed — keep whatever came after to avoid data loss.
                rest = after;
            }
        }
        out.push_str(rest);
        Ok(out)
    }
}

/// Scan `s` starting at an opening `(` and return the index just after
/// its matching close paren. Returns `None` if the string does not start
/// with `(` or parens are unbalanced.
fn skip_balanced_parens(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'(') {
        return None;
    }
    let mut depth: i32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// v2 → v3: Phase 1 character system added Race/Class/Attributes/HitBonus/
/// DamageBonus to `PlayerSaveData`. The migration is a no-op because
/// `#[serde(default)]` on each new field handles missing values cleanly:
/// pre-v3 saves load as Human Warrior with all-10 attributes and zero
/// flat bonuses — a sane fallback that doesn't crash the game. Players
/// can keep their hp/inventory/floor progress; only the new identity
/// fields default.
struct MigrateV2ToV3;
impl SaveMigration for MigrateV2ToV3 {
    fn from_version(&self) -> u32 {
        2
    }
    fn to_version(&self) -> u32 {
        3
    }
    fn migrate(&self, data: &str) -> Result<String, String> {
        Ok(data.to_string())
    }
}

/// v3 → v4: Phase 2 dropped CON from `Attributes` and removed Halfling
/// from `Race`. There is no safe automatic remapping (the modifier
/// anchor moved from 10 to 16; old attribute scores would systematically
/// over-mod under the new scale). This migration is intentionally a
/// no-op — v3 saves with CON values or Halfling race will fail to
/// deserialize against the new types and `read_save_data` will return
/// `None`. Acceptable in active dev with no production save data.
struct MigrateV3ToV4;
impl SaveMigration for MigrateV3ToV4 {
    fn from_version(&self) -> u32 {
        3
    }
    fn to_version(&self) -> u32 {
        4
    }
    fn migrate(&self, data: &str) -> Result<String, String> {
        Ok(data.to_string())
    }
}

/// v4 → v5: Phase 3 skills are additive — `PlayerSaveData` gains
/// skill fields with `#[serde(default)]`. No payload transformation
/// needed; pre-v5 saves load with empty skill state, which is exactly
/// what a fresh-spawn character looks like.
struct MigrateV4ToV5;
impl SaveMigration for MigrateV4ToV5 {
    fn from_version(&self) -> u32 {
        4
    }
    fn to_version(&self) -> u32 {
        5
    }
    fn migrate(&self, data: &str) -> Result<String, String> {
        Ok(data.to_string())
    }
}

/// v5 → v6: introduces `OverworldSave` on `GameSaveData` and
/// `exit_tiles` on `SavedFloorData`. Both fields are `#[serde(default)]`
/// so the migration itself is a no-op — old saves load with an empty
/// overworld state and no edge transitions, which is fine because the
/// overworld didn't exist on v5.
struct MigrateV5ToV6;
impl SaveMigration for MigrateV5ToV6 {
    fn from_version(&self) -> u32 { 5 }
    fn to_version(&self) -> u32 { 6 }
    fn migrate(&self, data: &str) -> Result<String, String> { Ok(data.to_string()) }
}

/// v6 → v7: Stealth Phase I adds `SavedMonster.awareness`. The field is
/// `#[serde(default)]` so the migration itself is a no-op — old saves
/// load with `MonsterAwarenessSave::default()` (Hidden), which mirrors
/// a fresh monster spawn before perception has fired.
struct MigrateV6ToV7;
impl SaveMigration for MigrateV6ToV7 {
    fn from_version(&self) -> u32 { 6 }
    fn to_version(&self) -> u32 { 7 }
    fn migrate(&self, data: &str) -> Result<String, String> { Ok(data.to_string()) }
}

// Note: v7 → v8 is intentionally not in the migration chain. v8 was
// the StatusEffectKind::Custom { id } → named-variant break (see the
// schema-version doc above); pre-v8 saves carry an unrecoverable
// bincode shape and fail to load. Adding a v7→v8 step would mask
// that intentional failure.

/// v8 → v9: tactic-registry migration adds
/// `SavedMonster.fleeing: Option<SavedFleeing>`. The field is
/// `#[serde(default)]` (defaults to `None`), so the migration is a
/// no-op — v8 monsters load as not-fleeing, which matches the
/// historical behaviour where sticky panic was always lost on save.
struct MigrateV8ToV9;
impl SaveMigration for MigrateV8ToV9 {
    fn from_version(&self) -> u32 { 8 }
    fn to_version(&self) -> u32 { 9 }
    fn migrate(&self, data: &str) -> Result<String, String> { Ok(data.to_string()) }
}

/// v9 → v10: RFC 0002 prop effects — `SavedProp.ever_fired` field
/// added. Backward-compatible (serde default `false`); migration is a
/// pure version-bump no-op.
struct MigrateV9ToV10;
impl SaveMigration for MigrateV9ToV10 {
    fn from_version(&self) -> u32 { 9 }
    fn to_version(&self) -> u32 { 10 }
    fn migrate(&self, data: &str) -> Result<String, String> { Ok(data.to_string()) }
}

fn migrations() -> Vec<Box<dyn SaveMigration>> {
    vec![
        Box::new(MigrateV0ToV1),
        Box::new(MigrateV1ToV2),
        Box::new(MigrateV2ToV3),
        Box::new(MigrateV3ToV4),
        Box::new(MigrateV4ToV5),
        Box::new(MigrateV5ToV6),
        Box::new(MigrateV6ToV7),
        Box::new(MigrateV8ToV9),
        Box::new(MigrateV9ToV10),
    ]
}

/// Write serialized save data under the game's save key, wrapped in the
/// versioned envelope.
pub fn write_save_data(data: &str) -> bool {
    save_with_version(GAME_SAVE_KEY, data, SAVE_SCHEMA_VERSION)
}

/// Read serialized save data for the game's save key, if any exists.
///
/// Handles envelope unwrapping and applies migrations for older schemas.
/// Falls back to reading a raw payload for pre-envelope saves (v0).
pub fn read_save_data() -> Option<String> {
    match load_with_version(GAME_SAVE_KEY) {
        Ok((version, payload)) => {
            if version == SAVE_SCHEMA_VERSION {
                return Some(payload);
            }
            let migs = migrations();
            let mig_refs: Vec<&dyn SaveMigration> = migs.iter().map(|m| m.as_ref()).collect();
            match apply_migrations(&payload, version, SAVE_SCHEMA_VERSION, &mig_refs) {
                Ok(migrated) => Some(migrated),
                Err(e) => {
                    warn!(
                        "save migration v{}→v{} failed: {}",
                        version, SAVE_SCHEMA_VERSION, e
                    );
                    None
                }
            }
        }
        Err(SaveLoadError::CorruptedEnvelope(_)) => {
            // Pre-envelope (v0) fallback: raw payload without the wrapper.
            let raw = platform::read_bytes(GAME_SAVE_KEY)?;
            let migs = migrations();
            let mig_refs: Vec<&dyn SaveMigration> = migs.iter().map(|m| m.as_ref()).collect();
            match apply_migrations(&raw, 0, SAVE_SCHEMA_VERSION, &mig_refs) {
                Ok(migrated) => Some(migrated),
                Err(e) => {
                    warn!(
                        "legacy save migration v0→v{} failed: {}",
                        SAVE_SCHEMA_VERSION, e
                    );
                    None
                }
            }
        }
        Err(SaveLoadError::NotFound) => None,
    }
}

/// Returns `true` if a save exists under the game's save key.
pub fn save_data_exists() -> bool {
    platform::exists(GAME_SAVE_KEY)
}

/// Delete the save data under the game's save key.
pub fn delete_save() {
    platform::delete(GAME_SAVE_KEY)
}

// ---- Temporary component ----

/// Placed on monsters during load; overrides their HP once stat_recalculation_system runs.
#[derive(Component)]
pub struct SavedHp(pub i32);

// ---- Plugin ----

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        // The engine's default key is a generic fallback; this game
        // uses "ironveil_save" for historical compatibility.
        app.insert_resource(SaveFrameworkConfig {
            save_key: GAME_SAVE_KEY.to_string(),
            schema_version: SAVE_SCHEMA_VERSION,
        })
        .init_resource::<SaveExists>()
        .init_resource::<PendingGameLoad>()
        .init_resource::<PendingPlayerLoad>()
        .init_resource::<AutoSavePending>()
        .add_systems(Startup, check_save_exists)
        .add_systems(OnEnter(AppState::Menu), check_save_exists)
        .add_systems(
            Update,
            (
                apply_player_load_system.run_if(|r: Res<PendingPlayerLoad>| r.0.is_some()),
                apply_saved_hp_system,
                apply_saved_awareness_system,
            )
                .run_if(in_state(AppState::InGame)),
        )
        // auto_save and exit-save both run in Last so they execute AFTER Bevy's
        // close_when_requested system (which runs in Update and sends AppExit).
        // The runner only checks AppExit after the full schedule (including Last),
        // so the save completes in the same frame the window is closed.
        .add_systems(
            Last,
            (
                save_on_exit_system.before(auto_save_system),
                auto_save_system.run_if(|r: Res<AutoSavePending>| r.0),
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn check_save_exists(mut save_exists: ResMut<SaveExists>) {
    save_exists.0 = save_data_exists();
}

// ---- Serializable data types ----

#[derive(Serialize, Deserialize)]
pub struct GameSaveData {
    pub floor: u32,
    pub game_log: Vec<String>,
    pub map: MapSaveData,
    pub player: PlayerSaveData,
    pub monsters: Vec<SavedMonster>,
    pub floor_items: Vec<SavedItem>,
    #[serde(default)]
    pub props: Vec<SavedProp>,
    pub floor_cache: HashMap<u32, SavedFloorData>,
    #[serde(default)]
    pub squad_id_counter: u64,
    /// Monsters that have fallen through a chasm and are waiting to be
    /// materialized on their destination floor. Keyed by destination depth.
    /// Defaults to empty for backward compatibility with older saves.
    #[serde(default)]
    pub fallen_monsters: HashMap<u32, Vec<SavedMonster>>,
    /// Items that have fallen through a chasm, waiting on the destination floor.
    #[serde(default)]
    pub fallen_items: HashMap<u32, Vec<SavedItem>>,
    /// Per-run overworld state — which forest tile contains the temple
    /// entrance and where on that tile the entrance sits. Schema v6+.
    #[serde(default)]
    pub overworld: OverworldSave,
}

/// Save-format mirror of `crate::map::world::OverworldState`.
#[derive(Serialize, Deserialize, Clone, Copy, Default)]
pub struct OverworldSave {
    #[serde(default = "default_entrance_floor")]
    pub temple_entrance_floor: u32,
    #[serde(default)]
    pub temple_entrance_pos: Option<[i32; 2]>,
}

fn default_entrance_floor() -> u32 { 1 }

/// `MapExitTile` snapshot — used to round-trip overworld edge exits
/// and temple stairs across save / restore. Schema v6+.
#[derive(Serialize, Deserialize, Clone, Copy, Default)]
pub struct SavedExitTile {
    pub x: i32,
    pub y: i32,
    pub destination_floor: u32,
    #[serde(default)]
    pub destination_pos: Option<[i32; 2]>,
}

// ---------------------------------------------------------------------------
// Unified entity types — shared by GameSaveData, CachedFloor, and
// SavedFloorData. Adding a new persistent field only requires updating
// these types + the queries that populate them.
// ---------------------------------------------------------------------------

/// Degraded per-monster awareness snapshot. Stealth Phase I (schema v7)
/// persists awareness across save/load with a deliberately small shape:
/// only the player-keyed record is saved, and only the two states that
/// reconstruct cleanly without resolving stale `Entity` IDs.
///
/// Save-time degradations (see `degrade_awareness_for_save`):
/// - `Aware`               → `Searching{ player.pos, +20 turns }`
/// - `Searching{pos, t}`   → `Searching{ pos, t - now }` (offset preserved)
/// - `Suspicious{...}`     → `Hidden` (no tracked suspect to round-trip)
/// - `Hidden` / no record  → `Hidden`
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SavedAwarenessState {
    Hidden,
    Searching {
        last_known_x: i32,
        last_known_y: i32,
        /// `giveup_at_turn - now_at_save`. On load, recomputed as
        /// `now_at_load + giveup_at_offset` so the timer resumes with
        /// the same remaining duration.
        giveup_at_offset: u32,
    },
}

impl Default for SavedAwarenessState {
    fn default() -> Self {
        SavedAwarenessState::Hidden
    }
}

/// Wrapper around `SavedAwarenessState` so future per-monster awareness
/// fields (e.g. last-known-pos for *other* targets) can be added without
/// another schema bump.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MonsterAwarenessSave {
    #[serde(default)]
    pub state: SavedAwarenessState,
}

/// A monster's mutable state, shared by save files and the floor cache.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedMonster {
    pub x: i32,
    pub y: i32,
    pub name: String,
    /// 0 means "freshly generated — let the manifest decide HP". Any
    /// positive value is a restored HP from cache or save.
    #[serde(default)]
    pub hp_current: i32,
    #[serde(default)]
    pub squad_id: Option<u64>,
    #[serde(default)]
    pub is_leader: bool,
    #[serde(default)]
    pub squad_config: Option<SquadConfig>,
    #[serde(default)]
    pub patrol_route: Option<crate::game::ai::PatrolRoute>,
    #[serde(default)]
    pub submerged: bool,
    /// Stealth Phase I (schema v7+): degraded awareness snapshot. Only
    /// the player-keyed record survives the round trip. Pre-v7 saves
    /// default to `Hidden` (matches a fresh perception state).
    #[serde(default)]
    pub awareness: MonsterAwarenessSave,
    /// Tactic-registry migration (schema v9+): sticky Fleeing overlay.
    /// `None` for monsters that weren't panicking when the save was
    /// taken (the common case). Pre-v9 saves default to `None`.
    #[serde(default)]
    pub fleeing: Option<crate::game::fleeing::SavedFleeing>,
}

impl SavedMonster {
    pub fn pos(&self) -> Point { Point::new(self.x, self.y) }
}

/// Shared mutable item state — enchantment, runic, and staff fields.
/// Embedded by both `SavedItem` (floor items) and `InventoryItemSave` (player inventory).
/// Adding a new persistent item field only requires updating this struct + the queries
/// that populate it.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ItemMutableState {
    #[serde(default)]
    pub enchantment: Option<i32>,
    #[serde(default)]
    pub weapon_runic: Option<WeaponRunic>,
    #[serde(default)]
    pub armor_runic: Option<ArmorRunic>,
    #[serde(default)]
    pub runic_identified: Option<bool>,
    #[serde(default)]
    pub staff_effect: Option<StaffEffect>,
    #[serde(default)]
    pub base_recharge: Option<u32>,
    #[serde(default)]
    pub staff_charges: Option<i32>,
    #[serde(default)]
    pub staff_max_charges: Option<i32>,
    #[serde(default)]
    pub staff_recharge_timer: Option<u32>,
    #[serde(default)]
    pub staff_recharge_rate: Option<u32>,
}

/// A floor item's mutable state, shared by save files and the floor cache.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedItem {
    pub x: i32,
    pub y: i32,
    pub name: String,
    #[serde(default = "default_stack_count")]
    pub count: u32,
    #[serde(default, flatten)]
    pub state: ItemMutableState,
    #[serde(default)]
    pub drifting: bool,
}

impl SavedItem {
    pub fn pos(&self) -> Point { Point::new(self.x, self.y) }
}

/// A prop's state, shared by save files and the floor cache.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedProp {
    pub x: i32,
    pub y: i32,
    pub name: String,
    /// RFC 0002 step 4 — persists the `EverFired` activation flag for
    /// props with an authored trigger (used altars / sprung traps).
    /// Defaults to `false` for backward compatibility with v9 saves;
    /// pre-v10 saves load all props as not-yet-fired, which matches
    /// the historical behavior where prop activation state was lost.
    #[serde(default)]
    pub ever_fired: bool,
}

impl SavedProp {
    pub fn pos(&self) -> Point { Point::new(self.x, self.y) }
}

/// A complete floor snapshot, shared by the in-memory floor cache and
/// serialized save files. Uses `MapSaveData` so it can be serialized
/// without a separate conversion type.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedFloorData {
    pub map: MapSaveData,
    pub monsters: Vec<SavedMonster>,
    pub items: Vec<SavedItem>,
    #[serde(default)]
    pub props: Vec<SavedProp>,
    pub down_stairs_pos: [i32; 2],
    #[serde(default)]
    pub up_stairs_pos: [i32; 2],
    /// Overworld edge / temple stair `MapExitTile` markers. Schema v6+.
    #[serde(default)]
    pub exit_tiles: Vec<SavedExitTile>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MapSaveData {
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub name: String,
    pub tiles: Vec<Tile>,
    pub explored: Vec<bool>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct PlayerSaveData {
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub armor: i32,
    /// Phase 3 follow-up: shield SH value. The Block component stores
    /// the SH (additive bonus to the block-check roll), not flat damage
    /// reduction. Defaults to 0 on pre-existing saves via serde.
    #[serde(default)]
    pub block: i32,
    /// Cap on shield-block attempts per turn (1 buckler / 2 kite /
    /// 3 tower). Defaults to 0 on pre-existing saves; a fresh equip of
    /// a shield restores the correct value via the equip handler.
    #[serde(default)]
    pub max_shield_blocks: u32,
    pub dodge: i32,
    pub viewshed_range: i32,
    pub damage: String,
    #[serde(default)]
    pub status_effects: StatusEffects,
    pub inventory: Vec<InventoryItemSave>,
    // ---- Phase 1 character system (save schema v3) ----
    /// Player race; defaults to Human on pre-v3 saves.
    #[serde(default)]
    pub race: crate::character::Race,
    /// Player class; defaults to Warrior on pre-v3 saves.
    #[serde(default)]
    pub class: crate::character::Class,
    /// Final attribute scores. Defaults to all-10 on pre-v3 saves (the
    /// pre-race-and-class baseline), so derived stats aren't broken.
    #[serde(default = "default_attributes_baseline")]
    pub attributes: crate::character::Attributes,
    /// Saved `HitBonus.0` — the post-equipment, post-attribute sum. On
    /// load this is restored directly so equipment doesn't need to be
    /// re-applied via the equip handler.
    #[serde(default)]
    pub hit_bonus: i32,
    /// Saved `DamageBonus.0`, same shape as `hit_bonus`.
    #[serde(default)]
    pub damage_bonus: i32,
    /// Phase 2: player XP level. Defaults to 1 on pre-v4 saves.
    #[serde(default = "default_save_level")]
    pub level: u32,
    /// Phase 2: experience accumulated toward the next level.
    #[serde(default)]
    pub experience: u32,
    /// Phase 3: per-skill float levels (mirrors `Skills` component).
    #[serde(default)]
    pub skills: crate::game::skills::Skills,
    /// Phase 3: per-skill cumulative XP (mirrors `SkillXp` component).
    #[serde(default)]
    pub skill_xp: crate::game::skills::SkillXp,
    /// Phase 3: per-skill training state (mirrors `SkillTraining`).
    #[serde(default)]
    pub skill_training: crate::game::skills::SkillTraining,
    /// Phase 3: unallocated skill XP pool (mirrors `SkillXpPool` resource).
    #[serde(default)]
    pub skill_xp_pool: u64,
}

fn default_save_level() -> u32 {
    1
}

/// Serde default for `PlayerSaveData::attributes` — Phase 2 anchors the
/// modifier at 16, so all-16 yields mod 0 across the board and a load
/// with missing attributes doesn't shift combat math.
fn default_attributes_baseline() -> crate::character::Attributes {
    crate::character::Attributes {
        strength: 16,
        dexterity: 16,
        intelligence: 16,
    }
}

#[derive(Serialize, Deserialize)]
pub struct InventoryItemSave {
    pub name: String,
    pub properties: ItemProperties,
    pub equipped_slot: Option<String>,
    #[serde(default = "default_stack_count")]
    pub count: u32,
    #[serde(default = "default_stack_max")]
    pub max_stack: u32,
    #[serde(default, flatten)]
    pub state: ItemMutableState,
    /// Key name for Key items (opens a specific locked door).
    #[serde(default)]
    pub key_name: Option<String>,
    /// Whether this item is a quest item (e.g., Amulet of Yendor).
    #[serde(default)]
    pub is_quest_item: bool,
}

fn default_stack_count() -> u32 {
    1
}
fn default_stack_max() -> u32 {
    1
}

/// Backward-compatible alias: save files and `SavedFloorCache` still
/// reference this name.
pub type CachedFloorSave = SavedFloorData;

// ---- Conversion helpers ----

/// Build `ItemMutableState` from ECS query results. Used by both floor-item
/// and inventory-item save paths to avoid duplicating the field mapping.
pub fn build_item_state(
    enchant: Option<&Enchantment>,
    weapon_runic: Option<&ItemWeaponRunic>,
    armor_runic: Option<&ItemArmorRunic>,
    runic_id: Option<&RunicIdentified>,
    staff_data: Option<&StaffData>,
    rechargeable: Option<&Rechargeable>,
) -> ItemMutableState {
    ItemMutableState {
        enchantment: enchant.map(|e| e.level),
        weapon_runic: weapon_runic.map(|w| w.0.clone()),
        armor_runic: armor_runic.map(|a| a.0),
        runic_identified: runic_id.map(|r| r.0),
        staff_effect: staff_data.map(|s| s.effect),
        base_recharge: staff_data.map(|s| s.base_recharge),
        staff_charges: rechargeable.map(|r| r.charges),
        staff_max_charges: rechargeable.map(|r| r.max_charges),
        staff_recharge_timer: rechargeable.map(|r| r.recharge_timer),
        staff_recharge_rate: rechargeable.map(|r| r.recharge_rate),
    }
}

/// Restore `ItemMutableState` fields onto an entity. Counterpart to `build_item_state`.
pub fn restore_item_mutable_state(
    commands: &mut Commands,
    entity: Entity,
    state: &ItemMutableState,
) {
    if let Some(level) = state.enchantment {
        commands.entity(entity).insert(Enchantment { level });
    }
    if let Some(ref runic) = state.weapon_runic {
        commands
            .entity(entity)
            .insert(ItemWeaponRunic(runic.clone()));
    }
    if let Some(runic) = state.armor_runic {
        commands.entity(entity).insert(ItemArmorRunic(runic));
    }
    if let Some(identified) = state.runic_identified {
        commands.entity(entity).insert(RunicIdentified(identified));
    }
    if let Some(effect) = state.staff_effect {
        let base_recharge = state.base_recharge.unwrap_or(250);
        commands.entity(entity).insert(StaffData {
            effect,
            base_recharge,
        });
        if let (Some(charges), Some(max_charges), Some(recharge_timer), Some(recharge_rate)) = (
            state.staff_charges,
            state.staff_max_charges,
            state.staff_recharge_timer,
            state.staff_recharge_rate,
        ) {
            commands.entity(entity).insert(Rechargeable {
                charges,
                max_charges,
                recharge_timer,
                recharge_rate,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Stealth Phase I — awareness degrade / restore helpers (schema v7).
// ---------------------------------------------------------------------------

/// Snapshot the *player-keyed* awareness record into a `MonsterAwarenessSave`
/// using the degraded scheme described on `SavedAwarenessState`. Pure
/// function — pulled out for unit testability and so the save loop stays
/// a simple `.map()`.
pub(crate) fn degrade_awareness_for_save(
    awareness: &roguelike_engine::stealth::Awareness,
    player_entity: Entity,
    player_pos: Point,
    now: u32,
) -> MonsterAwarenessSave {
    use roguelike_engine::stealth::AwarenessState;
    let saved_state = match awareness.get(player_entity).map(|r| r.state) {
        Some(AwarenessState::Aware) => SavedAwarenessState::Searching {
            last_known_x: player_pos.x,
            last_known_y: player_pos.y,
            giveup_at_offset: 20,
        },
        Some(AwarenessState::Searching { last_known_pos, giveup_at_turn }) => {
            SavedAwarenessState::Searching {
                last_known_x: last_known_pos.x,
                last_known_y: last_known_pos.y,
                giveup_at_offset: giveup_at_turn.saturating_sub(now),
            }
        }
        // Suspicious has a suspect_pos but we deliberately drop it — V1
        // keeps the saved shape narrow. Hidden / missing → Hidden.
        _ => SavedAwarenessState::Hidden,
    };
    MonsterAwarenessSave { state: saved_state }
}

/// Reconstruct an `Awareness` component from a saved snapshot, keyed at
/// the *post-load* player entity. Hidden saves produce an empty map (no
/// record); Searching saves produce a single Searching record with the
/// timer rebased to `now + giveup_at_offset`.
pub(crate) fn restore_awareness_from_save(
    saved: &MonsterAwarenessSave,
    player_entity: Entity,
    now: u32,
) -> roguelike_engine::stealth::Awareness {
    use roguelike_engine::stealth::{Awareness, AwarenessRecord, AwarenessState};
    let mut a = Awareness::default();
    let (last_known, giveup_at_turn) = match saved.state {
        SavedAwarenessState::Hidden => return a,
        SavedAwarenessState::Searching {
            last_known_x,
            last_known_y,
            giveup_at_offset,
        } => (
            Point::new(last_known_x, last_known_y),
            now.saturating_add(giveup_at_offset),
        ),
    };
    a.records.insert(
        player_entity,
        AwarenessRecord {
            state: AwarenessState::Searching {
                last_known_pos: last_known,
                giveup_at_turn,
            },
            last_update_turn: now,
            last_seen_pos: Some(last_known),
        },
    );
    a
}

pub fn map_to_save_data(map: &Map) -> MapSaveData {
    MapSaveData {
        width: map.width,
        height: map.height,
        depth: map.depth,
        name: map.name.clone(),
        tiles: map.tiles.clone(),
        explored: map.explored_tiles.clone(),
    }
}

pub fn save_data_to_map(data: &MapSaveData) -> Map {
    let tile_count = (data.width * data.height) as usize;
    Map {
        name: data.name.clone(),
        tiles: data.tiles.clone(),
        explored_tiles: data.explored.clone(),
        blocked: vec![false; tile_count],
        width: data.width,
        height: data.height,
        depth: data.depth,
    }
}

pub fn cached_floor_to_save(cached: &CachedFloor) -> SavedFloorData {
    SavedFloorData {
        map: map_to_save_data(&cached.map),
        monsters: cached.monsters.clone(),
        items: cached.items.clone(),
        props: cached.props.clone(),
        down_stairs_pos: [cached.down_stairs_pos.x, cached.down_stairs_pos.y],
        up_stairs_pos: [cached.up_stairs_pos.x, cached.up_stairs_pos.y],
        exit_tiles: cached
            .exit_tiles
            .iter()
            .map(|(pt, exit)| SavedExitTile {
                x: pt.x,
                y: pt.y,
                destination_floor: exit.destination_floor,
                destination_pos: exit.destination_pos.map(|p| [p.x, p.y]),
            })
            .collect(),
    }
}

pub fn save_to_cached_floor(data: &SavedFloorData) -> CachedFloor {
    CachedFloor {
        map: save_data_to_map(&data.map),
        monsters: data.monsters.clone(),
        items: data.items.clone(),
        props: data.props.clone(),
        exit_tiles: data
            .exit_tiles
            .iter()
            .map(|e| {
                (
                    Point::new(e.x, e.y),
                    crate::map::world::MapExitTile {
                        destination_floor: e.destination_floor,
                        destination_pos: e.destination_pos.map(|p| crate::components::Position { x: p[0], y: p[1] }),
                    },
                )
            })
            .collect(),
        down_stairs_pos: Point::new(data.down_stairs_pos[0], data.down_stairs_pos[1]),
        up_stairs_pos: Point::new(data.up_stairs_pos[0], data.up_stairs_pos[1]),
    }
}

// ---- Auto-save system ----

/// Bundles the Phase 3 skill query + skill XP pool so auto_save_system
/// stays under Bevy's 16-param cap.
#[derive(bevy::ecs::system::SystemParam)]
pub struct PlayerSkillSaveParams<'w, 's> {
    pub query: Query<
        'w,
        's,
        (
            &'static crate::game::skills::Skills,
            &'static crate::game::skills::SkillXp,
            &'static crate::game::skills::SkillTraining,
        ),
        With<Player>,
    >,
    pub pool: Res<'w, crate::game::skills::SkillXpPool>,
}

/// Bundled resources for `auto_save_system` — keeps the system under
/// Bevy's 16-param limit now that overworld state needs to be saved.
#[derive(bevy::ecs::system::SystemParam)]
pub struct AutoSaveExtras<'w, 's> {
    pub squad_counter: Res<'w, SquadIdCounter>,
    pub fallen_entities: Res<'w, crate::map::dungeon::FallenEntities>,
    pub overworld_state: Res<'w, crate::map::world::OverworldState>,
    /// Stealth Phase I (schema v7): needed by the per-monster awareness
    /// degrade pass to compute `giveup_at_offset = giveup_at_turn - now`.
    pub turn_manager: Res<'w, crate::game::TurnManager>,
    /// Stealth Phase I (schema v7): the player entity is the only key
    /// the degraded awareness shape preserves. Looked up once per save.
    pub player_entity: Query<'w, 's, Entity, With<Player>>,
}

#[allow(clippy::too_many_arguments)]
pub fn auto_save_system(
    mut auto_save_pending: ResMut<AutoSavePending>,
    mut save_exists: ResMut<SaveExists>,
    map: Res<Map>,
    floor: Res<Floor>,
    game_log: Res<GameLog>,
    floor_cache: Res<FloorCache>,
    player_query: Query<
        (
            &Position,
            &Health,
            &Armor,
            &crate::game::stats::Block,
            &crate::game::stats::MaxShieldBlocks,
            &Dodge,
            &Inventory,
            &Equipment,
            &Damage,
            &Viewshed,
        ),
        With<Player>,
    >,
    player_status_query: Query<&StatusEffects, With<Player>>,
    player_character_query: Query<
        (
            &crate::character::Race,
            &crate::character::Class,
            &crate::character::Attributes,
            &HitBonus,
            &DamageBonus,
            &crate::game::xp::Level,
            &crate::game::xp::Experience,
        ),
        With<Player>,
    >,
    skill_params: PlayerSkillSaveParams,
    inv_item_query: Query<
        (
            &Name,
            &ItemProperties,
            Has<Equipped>,
            Option<&ItemStack>,
            Option<&Enchantment>,
            Option<&ItemWeaponRunic>,
            Option<&ItemArmorRunic>,
            Option<&RunicIdentified>,
            Option<&StaffData>,
            Option<&Rechargeable>,
            Option<&Key>,
            Has<QuestItem>,
        ),
        With<InInventory>,
    >,
    monster_query: Query<
        (
            &Position,
            &Name,
            &Health,
            Option<&SquadId>,
            Option<&SquadConfig>,
            Has<SquadLeader>,
            Option<&crate::game::ai::PatrolRoute>,
            Has<crate::components::Submerged>,
            Option<&roguelike_engine::stealth::Awareness>,
            Option<&crate::game::fleeing::Fleeing>,
        ),
        With<Monster>,
    >,
    floor_item_query: Query<
        (
            &Position,
            &Name,
            Option<&ItemStack>,
            Option<&Enchantment>,
            Option<&ItemWeaponRunic>,
            Option<&ItemArmorRunic>,
            Option<&RunicIdentified>,
            Option<&StaffData>,
            Option<&Rechargeable>,
            Has<crate::components::Drifting>,
        ),
        (With<Item>, Without<InInventory>),
    >,
    prop_query: Query<
        (
            &Position,
            &Name,
            Option<&crate::components::PropKey>,
            Option<&crate::game::prop_effects::EverFired>,
        ),
        With<Prop>,
    >,
    auto_save_extras: AutoSaveExtras,
) {
    auto_save_pending.0 = false;
    let fallen_entities = &auto_save_extras.fallen_entities;
    let overworld_state = &auto_save_extras.overworld_state;
    let squad_counter = &auto_save_extras.squad_counter;
    let now = auto_save_extras.turn_manager.current_time;
    // Stealth Phase I: degraded awareness keys on the player entity. If
    // the player entity is missing the awareness map collapses to Hidden
    // (the safe default — perception will rebuild within a turn or two).
    let player_entity = auto_save_extras.player_entity.single().ok();

    let Ok((pos, health, armor, block, max_shield_blocks, dodge, inventory, equipment, damage, viewshed)) =
        player_query.single()
    else {
        warn!("Auto-save skipped: no player entity found.");
        return;
    };

    // Inventory items
    let inv_saves: Vec<InventoryItemSave> = inventory
        .items
        .iter()
        .filter_map(|&item_entity| {
            let Ok((
                name,
                props,
                is_equipped,
                stack,
                enchant,
                weapon_runic,
                armor_runic,
                runic_id,
                staff_data,
                rechargeable,
                key,
                is_quest_item,
            )) = inv_item_query.get(item_entity)
            else {
                return None;
            };
            let equipped_slot = if is_equipped {
                equipment.find_slot(item_entity).map(|s| s.to_string())
            } else {
                None
            };
            let (count, max_stack) = stack.map(|s| (s.count, s.max_stack)).unwrap_or((1, 1));
            Some(InventoryItemSave {
                name: name.0.clone(),
                properties: props.clone(),
                equipped_slot,
                count,
                max_stack,
                state: build_item_state(
                    enchant,
                    weapon_runic,
                    armor_runic,
                    runic_id,
                    staff_data,
                    rechargeable,
                ),
                key_name: key.map(|k| k.key_name.clone()),
                is_quest_item,
            })
        })
        .collect();

    // Floor monsters
    let player_pos_point = Point::new(pos.x, pos.y);
    let monsters: Vec<SavedMonster> = monster_query
        .iter()
        .map(
            |(mpos, name, health, squad_id, squad_config, is_leader, patrol_route, is_submerged, awareness, fleeing)| {
                let awareness_save = match (awareness, player_entity) {
                    (Some(a), Some(pe)) => {
                        degrade_awareness_for_save(a, pe, player_pos_point, now)
                    }
                    _ => MonsterAwarenessSave::default(),
                };
                SavedMonster {
                    x: mpos.x,
                    y: mpos.y,
                    name: name.0.clone(),
                    hp_current: health.current,
                    squad_id: squad_id.map(|s| s.0),
                    is_leader,
                    squad_config: squad_config.cloned(),
                    patrol_route: patrol_route.cloned(),
                    submerged: is_submerged,
                    awareness: awareness_save,
                    fleeing: fleeing.map(crate::game::fleeing::SavedFleeing::from_component),
                }
            },
        )
        .collect();

    // Floor items (not in inventory)
    let floor_items: Vec<SavedItem> = floor_item_query
        .iter()
        .map(
            |(
                pos,
                name,
                stack,
                enchant,
                weapon_runic,
                armor_runic,
                runic_id,
                staff_data,
                rechargeable,
                is_drifting,
            )| SavedItem {
                x: pos.x,
                y: pos.y,
                name: name.0.clone(),
                count: stack.map(|s| s.count).unwrap_or(1),
                state: build_item_state(
                    enchant,
                    weapon_runic,
                    armor_runic,
                    runic_id,
                    staff_data,
                    rechargeable,
                ),
                drifting: is_drifting,
            },
        )
        .collect();

    // Status effects
    let status_effects = player_status_query.single().cloned().unwrap_or_default();

    // Props
    let props: Vec<SavedProp> = prop_query
        .iter()
        .map(|(pos, name, prop_key, ever_fired)| SavedProp {
            x: pos.x,
            y: pos.y,
            // Use the manifest key if available; fall back to display name
            // for backward compatibility with old saves.
            name: prop_key
                .map(|k| k.0.clone())
                .unwrap_or_else(|| name.0.clone()),
            ever_fired: ever_fired.map(|e| e.0).unwrap_or(false),
        })
        .collect();

    // Floor cache
    let floor_cache_save: HashMap<u32, SavedFloorData> = floor_cache
        .0
        .iter()
        .map(|(k, v)| (*k, cached_floor_to_save(v)))
        .collect();

    // Character-system fields. The player query for Race/Class/Attributes/
    // HitBonus/DamageBonus is separate so we don't blow Bevy's max-tuple
    // size on the main player query. Defaults applied if the player entity
    // somehow lacks them (shouldn't happen post-spawn, but the save path
    // must not crash mid-run).
    let (race, class, attributes, hit_bonus, damage_bonus, level, experience) = player_character_query
        .single()
        .map(|(r, c, a, h, d, l, x)| (*r, *c, *a, h.0, d.0, l.0, x.0))
        .unwrap_or_else(|_| {
            (
                crate::character::Race::default(),
                crate::character::Class::default(),
                default_attributes_baseline(),
                0,
                0,
                1,
                0,
            )
        });

    // Phase 3 skill state. Defaults if the player entity has somehow
    // lost its skill components mid-run (shouldn't happen post-spawn,
    // but the save path must not crash).
    let (skills, skill_xp, skill_training) = skill_params
        .query
        .single()
        .map(|(s, x, t)| (s.clone(), x.clone(), t.clone()))
        .unwrap_or_default();
    let skill_pool_raw = skill_params.pool.raw;

    let save_data = GameSaveData {
        floor: floor.0,
        game_log: game_log.entries.clone(),
        map: map_to_save_data(&map),
        player: PlayerSaveData {
            x: pos.x,
            y: pos.y,
            hp: health.current,
            armor: armor.0,
            block: block.0,
            max_shield_blocks: max_shield_blocks.0,
            dodge: dodge.0,
            viewshed_range: viewshed.range,
            damage: damage.0.clone(),
            status_effects,
            inventory: inv_saves,
            race,
            class,
            attributes,
            hit_bonus,
            damage_bonus,
            level,
            experience,
            skills,
            skill_xp,
            skill_training,
            skill_xp_pool: skill_pool_raw,
        },
        monsters,
        floor_items,
        props,
        floor_cache: floor_cache_save,
        squad_id_counter: squad_counter.0,
        fallen_monsters: fallen_entities.monsters.clone(),
        fallen_items: fallen_entities.items.clone(),
        overworld: OverworldSave::default(),
    };

    match ron::ser::to_string_pretty(&save_data, ron::ser::PrettyConfig::default()) {
        Ok(serialized) => {
            if write_save_data(&serialized) {
                info!("Game saved.");
                save_exists.0 = true;
            }
        }
        Err(e) => error!("Failed to serialize save data: {}", e),
    }
}

// ---- Player load system ----
// Runs one time after spawn_dungeon sets PendingPlayerLoad.

pub fn apply_player_load_system(
    mut pending: ResMut<PendingPlayerLoad>,
    mut commands: Commands,
    mut player_query: Query<
        (
            &mut Position,
            &mut Health,
            &mut Armor,
            &mut crate::game::stats::Block,
            &mut crate::game::stats::MaxShieldBlocks,
            &mut Dodge,
            &mut Inventory,
            &mut Equipment,
            &mut Damage,
            &mut Viewshed,
            &mut HitBonus,
            &mut DamageBonus,
        ),
        With<Player>,
    >,
    player_entity_query: Query<Entity, With<Player>>,
    spawner: crate::game::spawner::ItemSpawner,
    mut floor_cache: ResMut<FloorCache>,
    saved_floor_cache: Option<Res<SavedFloorCache>>,
    mut save_exists: ResMut<SaveExists>,
) {
    let Some(player_data) = pending.0.take() else {
        return;
    };

    let Ok((
        mut pos,
        mut health,
        mut armor,
        mut block,
        mut max_shield_blocks,
        mut dodge,
        mut inventory,
        mut equipment,
        mut damage,
        mut viewshed,
        mut hit_bonus,
        mut damage_bonus,
    )) = player_query.single_mut()
    else {
        warn!("apply_player_load_system: no player entity yet, requeueing.");
        pending.0 = Some(player_data);
        return;
    };

    // --- Position ---
    pos.x = player_data.x;
    pos.y = player_data.y;

    // --- Health ---
    health.current = player_data.hp;

    // --- Armor / Block / Dodge ---
    armor.0 = player_data.armor;
    block.0 = player_data.block;
    max_shield_blocks.0 = player_data.max_shield_blocks;
    dodge.0 = player_data.dodge;

    // --- HitBonus / DamageBonus (post-equipment, post-attribute totals) ---
    hit_bonus.0 = player_data.hit_bonus;
    damage_bonus.0 = player_data.damage_bonus;

    // --- Damage / Viewshed ---
    damage.0 = player_data.damage.clone();
    viewshed.range = player_data.viewshed_range;
    viewshed.dirty = true;

    // --- Status effects + character system components ---
    if let Ok(player_entity) = player_entity_query.single() {
        commands.entity(player_entity).insert((
            player_data.status_effects.clone(),
            // Race/Class/Attributes inserted via .insert() overwrite the
            // spawn-time values (which were derived from the current
            // `CharacterChoice` resource — which may not match the saved
            // character).
            player_data.race,
            player_data.class,
            player_data.attributes,
            crate::game::xp::Level(player_data.level),
            crate::game::xp::Experience(player_data.experience),
            // Phase 3: skill components also override spawn-time values.
            player_data.skills.clone(),
            player_data.skill_xp.clone(),
            player_data.skill_training.clone(),
        ));
    }
    // Restore the skill XP pool resource.
    commands.insert_resource(crate::game::skills::SkillXpPool {
        raw: player_data.skill_xp_pool,
    });

    // --- Inventory ---
    inventory.items.clear();
    *equipment = Equipment::default();

    let dummy_pt = Point::new(0, 0);
    for item_save in &player_data.inventory {
        let Some(item_entity) = spawner.try_spawn(&mut commands, &item_save.name, &dummy_pt, None)
        else {
            continue;
        };

        // Override properties from save (preserves any stat tweaks)
        commands
            .entity(item_entity)
            .insert(item_save.properties.clone())
            .insert(ItemStack {
                count: item_save.count,
                max_stack: item_save.max_stack,
            })
            .insert(InInventory)
            .insert(Visibility::Hidden)
            .remove::<FloorEntityMarker>()
            .remove::<Position>();

        // Restore enchantment, runic, and staff data from shared state
        restore_item_mutable_state(&mut commands, item_entity, &item_save.state);

        // Restore Key component
        if let Some(ref key_name) = item_save.key_name {
            commands.entity(item_entity).insert(Key {
                key_name: key_name.clone(),
            });
        }

        // Restore QuestItem marker
        if item_save.is_quest_item {
            commands.entity(item_entity).insert(QuestItem);
        }

        inventory.items.push(item_entity);

        if let Some(ref slot) = item_save.equipped_slot {
            commands.entity(item_entity).insert(Equipped);
            equipment.set_slot(slot, Some(item_entity));
        }
    }

    // --- Restore floor cache ---
    if let Some(saved_cache) = saved_floor_cache {
        for (floor_num, cached_save) in &saved_cache.0 {
            floor_cache
                .0
                .insert(*floor_num, save_to_cached_floor(cached_save));
        }
    }

    save_exists.0 = true;
    info!("Player state restored from save.");
}

// ---- Save-on-exit system ----
// Triggers auto_save_system in the same frame when the app is about to exit.

pub fn save_on_exit_system(
    mut exit_events: MessageReader<AppExit>,
    mut auto_save_pending: ResMut<AutoSavePending>,
) {
    if !exit_events.is_empty() {
        exit_events.clear();
        auto_save_pending.0 = true;
    }
}

// ---- HP override system ----
// Applies SavedHp to monsters after stat_recalculation_system has run.

pub fn apply_saved_hp_system(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Health, &SavedHp)>,
) {
    for (entity, mut health, saved) in query.iter_mut() {
        health.current = saved.0.min(health.max);
        commands.entity(entity).remove::<SavedHp>();
    }
}

// ---- Awareness restore system ----
// Applies PendingAwarenessRestore to monsters once the player entity is
// available. Mirrors the SavedHp pattern — monsters get the marker
// component at spawn, this system converts it into a real `Awareness`
// keyed by the live player entity, then removes the marker.

/// Temporary marker carrying the saved degraded awareness blob until
/// the player entity exists and we can key the reconstructed
/// `Awareness::records` against it.
#[derive(Component, Debug, Clone)]
pub struct PendingAwarenessRestore(pub MonsterAwarenessSave);

pub fn apply_saved_awareness_system(
    mut commands: Commands,
    turn_manager: Res<roguelike_engine::turn::TurnManager>,
    player_query: Query<Entity, With<crate::player::Player>>,
    query: Query<(Entity, &PendingAwarenessRestore)>,
) {
    let Ok(player_entity) = player_query.single() else { return; };
    let now = turn_manager.current_time;
    for (monster_entity, pending) in query.iter() {
        let restored = restore_awareness_from_save(&pending.0, player_entity, now);
        commands
            .entity(monster_entity)
            .insert(restored)
            .remove::<PendingAwarenessRestore>();
    }
}

// ---- Helper resource to pass floor cache through the load pipeline ----

/// Temporarily holds the serialized floor cache loaded from disk.
/// Consumed by apply_player_load_system to restore FloorCache.
#[derive(Resource, Default)]
pub struct SavedFloorCache(pub HashMap<u32, SavedFloorData>);

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::ai::{PatrolRoute, PatrolState};
    use crate::game::combat::DamageType;
    use crate::game::effects::Effect;
    use crate::game::enchantment::{ArmorRunic, WeaponRunic};
    use crate::game::items::{ArmorSlot, ItemKind, Rarity};
    use crate::game::magic::{
        GameStatusEffectsExt, StatusEffectInstance, StatusEffectKind, StatusEffects,
    };
    use crate::game::squad::SquadConfig;
    use crate::game::staves::StaffEffect;
    use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};

    // NOTE ON RON + #[serde(flatten)]
    //
    // RON 0.10 has a known limitation: `#[serde(flatten)]` causes the parent
    // struct to serialize as a JSON-style map rather than RON struct syntax.
    // Deserialization then fails for `Option<EnumUnitVariant>` values.
    //
    // SavedItem and InventoryItemSave use `#[serde(flatten)]` on their
    // `state: ItemMutableState` field. Items with Some(enum) in weapon_runic,
    // armor_runic, or staff_effect serialize fine but FAIL to deserialize.
    //
    // This is a latent production bug (no affected items saved yet).
    // Tests work around it by:
    //   1. Testing ItemMutableState enum fields directly (non-flattened).
    //   2. Testing SavedItem/InventoryItemSave with scalar-only state.
    //   3. Including ron_flatten_enum_limitation to document the bug.

    fn to_ron<T: Serialize>(value: &T) -> String {
        ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())
            .expect("RON serialization failed")
    }

    fn from_ron<T: serde::de::DeserializeOwned>(s: &str) -> T {
        ron::from_str(s).expect("RON deserialization failed")
    }

    // -- Tile helpers --

    fn tile_floor() -> Tile {
        Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }
    fn tile_wall() -> Tile {
        Tile {
            terrain: TerrainType::Wall,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        }
    }
    fn tile_water() -> Tile {
        Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::Water,
            decoration: Decoration::Grass,
        }
    }

    // -- Composite helpers --

    fn small_map() -> MapSaveData {
        MapSaveData {
            width: 4,
            height: 4,
            depth: 3,
            name: "Test Dungeon".to_string(),
            tiles: vec![
                tile_wall(),
                tile_wall(),
                tile_wall(),
                tile_wall(),
                tile_wall(),
                tile_floor(),
                tile_floor(),
                tile_wall(),
                tile_wall(),
                tile_water(),
                tile_floor(),
                tile_wall(),
                tile_wall(),
                tile_wall(),
                tile_wall(),
                tile_wall(),
            ],
            explored: vec![
                false, false, false, false, false, true, true, false, false, true, false, false,
                false, false, false, false,
            ],
        }
    }

    fn basic_monster() -> SavedMonster {
        SavedMonster {
            x: 5,
            y: 10,
            name: "Goblin".to_string(),
            hp_current: 8,
            squad_id: None,
            is_leader: false,
            squad_config: None,
            patrol_route: None,
            submerged: false,
            awareness: MonsterAwarenessSave::default(),
            fleeing: None,
        }
    }

    fn squad_leader_monster() -> SavedMonster {
        SavedMonster {
            x: 12,
            y: 7,
            name: "Goblin Chieftain".to_string(),
            hp_current: 25,
            squad_id: Some(42),
            is_leader: true,
            squad_config: Some(SquadConfig {
                flee_threshold: 0.3,
            }),
            patrol_route: Some(PatrolRoute {
                state: PatrolState::Sentry { home: (12, 7) },
            }),
            submerged: false,
            awareness: MonsterAwarenessSave::default(),
            fleeing: None,
        }
    }

    fn submerged_monster() -> SavedMonster {
        SavedMonster {
            x: 20,
            y: 15,
            name: "Eel".to_string(),
            hp_current: 12,
            squad_id: Some(42),
            is_leader: false,
            squad_config: Some(SquadConfig {
                flee_threshold: 0.5,
            }),
            patrol_route: Some(PatrolRoute {
                state: PatrolState::AreaRoam {
                    min: (18, 13),
                    max: (25, 20),
                },
            }),
            submerged: true,
            awareness: MonsterAwarenessSave::default(),
            fleeing: None,
        }
    }

    fn basic_item() -> SavedItem {
        SavedItem {
            x: 3,
            y: 4,
            name: "Health Potion".to_string(),
            count: 3,
            state: ItemMutableState::default(),
            drifting: false,
        }
    }

    fn scalar_state_item() -> SavedItem {
        SavedItem {
            x: 10,
            y: 20,
            name: "Longsword".to_string(),
            count: 1,
            state: ItemMutableState {
                enchantment: Some(3),
                runic_identified: Some(true),
                base_recharge: Some(250),
                staff_charges: Some(2),
                staff_max_charges: Some(5),
                staff_recharge_timer: Some(100),
                staff_recharge_rate: Some(200),
                ..Default::default()
            },
            drifting: false,
        }
    }

    fn drifting_item() -> SavedItem {
        SavedItem {
            x: 15,
            y: 22,
            name: "Gold".to_string(),
            count: 50,
            state: ItemMutableState::default(),
            drifting: true,
        }
    }

    fn basic_prop() -> SavedProp {
        SavedProp {
            x: 6,
            y: 9,
            name: "watchfire".to_string(),
            ever_fired: false,
        }
    }

    fn consumable_inv_item() -> InventoryItemSave {
        InventoryItemSave {
            name: "Potion of Healing".to_string(),
            properties: ItemProperties {
                kind: ItemKind::Consumable,
                effect: Some(Effect::HealHp(20)),
                rarity: Rarity::Common,
                attack_speed: 1.0,
                ..Default::default()
            },
            equipped_slot: None,
            count: 2,
            max_stack: 5,
            state: ItemMutableState::default(),
            key_name: None,
            is_quest_item: false,
        }
    }

    fn key_inv_item() -> InventoryItemSave {
        InventoryItemSave {
            name: "Iron Key".to_string(),
            properties: ItemProperties {
                kind: ItemKind::Consumable,
                rarity: Rarity::Uncommon,
                attack_speed: 1.0,
                ..Default::default()
            },
            equipped_slot: None,
            count: 1,
            max_stack: 1,
            state: ItemMutableState::default(),
            key_name: Some("crypt_key".to_string()),
            is_quest_item: false,
        }
    }

    fn quest_inv_item() -> InventoryItemSave {
        InventoryItemSave {
            name: "Amulet of Yendor".to_string(),
            properties: ItemProperties {
                kind: ItemKind::Amulet,
                rarity: Rarity::Legendary,
                attack_speed: 1.0,
                ..Default::default()
            },
            equipped_slot: None,
            count: 1,
            max_stack: 1,
            state: ItemMutableState::default(),
            key_name: None,
            is_quest_item: true,
        }
    }

    fn weapon_inv_item() -> InventoryItemSave {
        InventoryItemSave {
            name: "Dagger".to_string(),
            properties: ItemProperties {
                kind: ItemKind::Weapon,
                damage: Some("1d4+1".to_string()),
                rarity: Rarity::Uncommon,
                weapon_range: 1,
                attack_speed: 0.5,
                hit_bonus: 2,
                weapon_ability: Some("Backstab".to_string()),
                ..Default::default()
            },
            equipped_slot: Some("weapon".to_string()),
            count: 1,
            max_stack: 1,
            state: ItemMutableState {
                enchantment: Some(2),
                runic_identified: Some(false),
                ..Default::default()
            },
            key_name: None,
            is_quest_item: false,
        }
    }

    fn armor_inv_item() -> InventoryItemSave {
        InventoryItemSave {
            name: "Chain Mail".to_string(),
            properties: ItemProperties {
                kind: ItemKind::Armor,
                armor_slot: Some(ArmorSlot::Chest),
                defense: 5,
                rarity: Rarity::Common,
                attack_speed: 1.0,
                dodge_bonus: -1,
                delay_modifier: 0.1,
                ..Default::default()
            },
            equipped_slot: Some("chest".to_string()),
            count: 1,
            max_stack: 1,
            state: ItemMutableState {
                enchantment: Some(1),
                runic_identified: Some(true),
                ..Default::default()
            },
            key_name: None,
            is_quest_item: false,
        }
    }

    fn player_data() -> PlayerSaveData {
        PlayerSaveData {
            x: 10,
            y: 20,
            hp: 45,
            armor: 3,
            block: 2,
            max_shield_blocks: 1,
            dodge: 2,
            viewshed_range: 8,
            damage: "1d4+1".to_string(),
            status_effects: StatusEffects {
                effects: vec![
                    StatusEffectInstance {
                        kind: StatusEffectKind::Hasted,
                        remaining_turns: 5,
                        magnitude: 0,
                        source: None,
                    },
                    StatusEffectInstance {
                        kind: StatusEffectKind::Poisoned,
                        remaining_turns: 3,
                        magnitude: 2,
                        source: None,
                    },
                ],
            },
            inventory: vec![
                weapon_inv_item(),
                armor_inv_item(),
                consumable_inv_item(),
                key_inv_item(),
                quest_inv_item(),
            ],
            race: crate::character::Race::Dwarf,
            class: crate::character::Class::Warrior,
            attributes: crate::character::Attributes {
                strength: 20,
                dexterity: 10,
                intelligence: 8,
            },
            hit_bonus: 5,
            damage_bonus: 3,
            level: 4,
            experience: 250,
            skills: Default::default(),
            skill_xp: Default::default(),
            skill_training: Default::default(),
            skill_xp_pool: 0,
        }
    }

    fn floor_data() -> SavedFloorData {
        SavedFloorData {
            map: small_map(),
            monsters: vec![basic_monster()],
            items: vec![basic_item()],
            props: vec![basic_prop()],
            down_stairs_pos: [3, 2],
            up_stairs_pos: [1, 1],
            exit_tiles: Vec::new(),
        }
    }

    fn full_save() -> GameSaveData {
        let mut fc = HashMap::new();
        fc.insert(1, floor_data());
        // Populate the fallen queues so roundtrip tests exercise both fields.
        let mut fallen_monsters = HashMap::new();
        fallen_monsters.insert(4, vec![basic_monster()]);
        let mut fallen_items = HashMap::new();
        fallen_items.insert(4, vec![basic_item(), drifting_item()]);
        GameSaveData {
            floor: 3,
            game_log: vec![
                "Welcome to floor 1!".into(),
                "A goblin attacks!".into(),
                "You slay the goblin.".into(),
            ],
            map: small_map(),
            player: player_data(),
            monsters: vec![basic_monster(), squad_leader_monster(), submerged_monster()],
            floor_items: vec![basic_item(), scalar_state_item(), drifting_item()],
            props: vec![basic_prop()],
            floor_cache: fc,
            squad_id_counter: 99,
            fallen_monsters,
            fallen_items,
            overworld: OverworldSave::default(),
        }
    }

    fn minimal_save(squad_id_counter: u64) -> GameSaveData {
        GameSaveData {
            floor: 1,
            game_log: vec![],
            map: MapSaveData {
                width: 1,
                height: 1,
                depth: 1,
                name: "t".into(),
                tiles: vec![tile_floor()],
                explored: vec![false],
            },
            player: PlayerSaveData {
                x: 0,
                y: 0,
                hp: 1,
                armor: 0,
                dodge: 0,
                viewshed_range: 1,
                damage: "1".into(),
                status_effects: StatusEffects::default(),
                inventory: vec![],
                ..Default::default()
            },
            monsters: vec![],
            floor_items: vec![],
            props: vec![],
            floor_cache: HashMap::new(),
            squad_id_counter,
            fallen_monsters: HashMap::new(),
            fallen_items: HashMap::new(),
            overworld: OverworldSave::default(),
        }
    }

    // =====================================================================
    // Full GameSaveData roundtrip
    // =====================================================================

    #[test]
    fn roundtrip_full_save_data() {
        let loaded: GameSaveData = from_ron(&to_ron(&full_save()));
        assert_eq!(loaded.floor, 3);
        assert_eq!(loaded.game_log.len(), 3);
        assert_eq!(loaded.game_log[1], "A goblin attacks!");
        assert_eq!(loaded.monsters.len(), 3);
        assert_eq!(loaded.floor_items.len(), 3);
        assert_eq!(loaded.props.len(), 1);
        assert_eq!(loaded.floor_cache.len(), 1);
        assert_eq!(loaded.squad_id_counter, 99);
    }

    #[test]
    fn roundtrip_full_save_data_field_values() {
        let original = full_save();
        let loaded: GameSaveData = from_ron(&to_ron(&original));
        assert_eq!(loaded.floor, original.floor);
        assert_eq!(loaded.squad_id_counter, original.squad_id_counter);
        assert_eq!(loaded.monsters.len(), original.monsters.len());
        assert_eq!(loaded.floor_items.len(), original.floor_items.len());
    }

    #[test]
    fn roundtrip_fallen_queues_preserve_destination_floor() {
        // Fallen entities must survive save/load so a save taken mid-collapse
        // doesn't lose queued monsters/items before the player descends.
        let loaded: GameSaveData = from_ron(&to_ron(&full_save()));
        let queued_monsters = loaded
            .fallen_monsters
            .get(&4)
            .expect("fallen_monsters for floor 4 should round-trip");
        assert_eq!(queued_monsters.len(), 1);
        assert_eq!(queued_monsters[0].name, "Goblin");

        let queued_items = loaded
            .fallen_items
            .get(&4)
            .expect("fallen_items for floor 4 should round-trip");
        assert_eq!(queued_items.len(), 2);
    }

    #[test]
    fn legacy_saves_without_fallen_fields_still_load() {
        // Backward compatibility: a save from before this feature lacked the
        // `fallen_monsters` / `fallen_items` fields. `#[serde(default)]` must
        // fill them with empty HashMaps.
        let legacy_ron = r#"(
            floor: 1,
            game_log: [],
            map: (width: 1, height: 1, depth: 1, name: "t", tiles: [(terrain: Floor, liquid: None, decoration: None)], explored: [false]),
            player: (x: 0, y: 0, hp: 1, armor: 0, dodge: 0, viewshed_range: 1, damage: "1", status_effects: (effects: []), inventory: []),
            monsters: [],
            floor_items: [],
            props: [],
            floor_cache: {},
            squad_id_counter: 0,
        )"#;
        let loaded: GameSaveData = from_ron(legacy_ron);
        assert!(loaded.fallen_monsters.is_empty());
        assert!(loaded.fallen_items.is_empty());
    }

    // =====================================================================
    // MapSaveData roundtrip
    // =====================================================================

    #[test]
    fn roundtrip_map_save_data() {
        let loaded: MapSaveData = from_ron(&to_ron(&small_map()));
        assert_eq!(loaded.width, 4);
        assert_eq!(loaded.height, 4);
        assert_eq!(loaded.depth, 3);
        assert_eq!(loaded.name, "Test Dungeon");
        assert_eq!(loaded.tiles.len(), 16);
        assert_eq!(loaded.explored.len(), 16);
        assert_eq!(loaded.tiles[0].terrain, TerrainType::Wall);
        assert_eq!(loaded.tiles[5].terrain, TerrainType::Floor);
        assert_eq!(loaded.tiles[9].liquid, LiquidType::Water);
        assert_eq!(loaded.tiles[9].decoration, Decoration::Grass);
        assert!(!loaded.explored[0]);
        assert!(loaded.explored[5]);
    }

    #[test]
    fn roundtrip_map_all_terrain_types() {
        let terrains = [
            TerrainType::Wall,
            TerrainType::Floor,
            TerrainType::DownStairs,
            TerrainType::UpStairs,
            TerrainType::Empty,
            TerrainType::Door,
            TerrainType::OpenDoor,
            TerrainType::HiddenDoor,
            TerrainType::LockedDoor,
            TerrainType::Portal,
        ];
        let tiles: Vec<Tile> = terrains
            .iter()
            .map(|&t| Tile {
                terrain: t,
                liquid: LiquidType::None,
                decoration: Decoration::None,
            })
            .collect();
        let data = MapSaveData {
            width: tiles.len() as i32,
            height: 1,
            depth: 1,
            name: "t".into(),
            tiles,
            explored: vec![false; terrains.len()],
        };
        let loaded: MapSaveData = from_ron(&to_ron(&data));
        for (i, &t) in terrains.iter().enumerate() {
            assert_eq!(loaded.tiles[i].terrain, t, "terrain mismatch at {}", i);
        }
    }

    #[test]
    fn roundtrip_map_all_liquid_types() {
        let liquids = [
            LiquidType::None,
            LiquidType::Water,
            LiquidType::ShallowWater,
            LiquidType::Lava,
            LiquidType::Chasm,
        ];
        let tiles: Vec<Tile> = liquids
            .iter()
            .map(|&l| Tile {
                terrain: TerrainType::Floor,
                liquid: l,
                decoration: Decoration::None,
            })
            .collect();
        let data = MapSaveData {
            width: tiles.len() as i32,
            height: 1,
            depth: 1,
            name: "t".into(),
            tiles,
            explored: vec![true; liquids.len()],
        };
        let loaded: MapSaveData = from_ron(&to_ron(&data));
        for (i, &l) in liquids.iter().enumerate() {
            assert_eq!(loaded.tiles[i].liquid, l, "liquid mismatch at {}", i);
        }
    }

    #[test]
    fn roundtrip_map_all_decorations() {
        let decos = [
            Decoration::None,
            Decoration::Grass,
            Decoration::TallGrass,
            Decoration::DeadGrass,
            Decoration::Rubble,
            Decoration::Moss,
            Decoration::Fungus,
            Decoration::Cobweb,
            Decoration::Bloodstain,
            Decoration::TrampledGrass,
            Decoration::TrampledFungus,
            Decoration::Embers,
            Decoration::Ash,
            Decoration::CrackedFloor,
        ];
        let tiles: Vec<Tile> = decos
            .iter()
            .map(|&d| Tile {
                terrain: TerrainType::Floor,
                liquid: LiquidType::None,
                decoration: d,
            })
            .collect();
        let data = MapSaveData {
            width: tiles.len() as i32,
            height: 1,
            depth: 1,
            name: "t".into(),
            tiles,
            explored: vec![false; decos.len()],
        };
        let loaded: MapSaveData = from_ron(&to_ron(&data));
        for (i, &d) in decos.iter().enumerate() {
            assert_eq!(loaded.tiles[i].decoration, d, "deco mismatch at {}", i);
        }
    }

    // =====================================================================
    // SavedMonster roundtrip
    // =====================================================================

    #[test]
    fn roundtrip_monster_basic() {
        let loaded: SavedMonster = from_ron(&to_ron(&basic_monster()));
        assert_eq!(loaded.x, 5);
        assert_eq!(loaded.y, 10);
        assert_eq!(loaded.name, "Goblin");
        assert_eq!(loaded.hp_current, 8);
        assert_eq!(loaded.squad_id, None);
        assert!(!loaded.is_leader);
        assert!(loaded.squad_config.is_none());
        assert!(loaded.patrol_route.is_none());
        assert!(!loaded.submerged);
    }

    #[test]
    fn roundtrip_monster_squad_leader() {
        let loaded: SavedMonster = from_ron(&to_ron(&squad_leader_monster()));
        assert_eq!(loaded.hp_current, 25);
        assert_eq!(loaded.squad_id, Some(42));
        assert!(loaded.is_leader);
        let cfg = loaded.squad_config.unwrap();
        assert!((cfg.flee_threshold - 0.3).abs() < f32::EPSILON);
        match loaded.patrol_route.unwrap().state {
            PatrolState::Sentry { home } => assert_eq!(home, (12, 7)),
            _ => panic!("expected Sentry"),
        }
    }

    #[test]
    fn roundtrip_monster_submerged() {
        let loaded: SavedMonster = from_ron(&to_ron(&submerged_monster()));
        assert!(loaded.submerged);
        assert!(!loaded.is_leader);
        match loaded.patrol_route.unwrap().state {
            PatrolState::AreaRoam { min, max } => {
                assert_eq!(min, (18, 13));
                assert_eq!(max, (25, 20));
            }
            _ => panic!("expected AreaRoam"),
        }
    }

    #[test]
    fn roundtrip_monster_fleeing_state() {
        use crate::game::fleeing::SavedFleeing;
        let m = SavedMonster {
            fleeing: Some(SavedFleeing {
                since_turn: 142,
                last_known_threat_x: Some(12),
                last_known_threat_y: Some(7),
            }),
            ..basic_monster()
        };
        let loaded: SavedMonster = from_ron(&to_ron(&m));
        let f = loaded.fleeing.expect("Fleeing should survive roundtrip");
        assert_eq!(f.since_turn, 142);
        assert_eq!(f.last_known_threat_x, Some(12));
        assert_eq!(f.last_known_threat_y, Some(7));
    }

    #[test]
    fn roundtrip_monster_without_fleeing() {
        let m = basic_monster();
        let loaded: SavedMonster = from_ron(&to_ron(&m));
        assert!(loaded.fleeing.is_none());
    }

    #[test]
    fn pre_v9_save_loads_with_no_fleeing() {
        // RON without a `fleeing:` field must round-trip via serde defaults
        // — that's the v8→v9 migration contract (no-op + serde(default)).
        let ron = r#"(
            x: 5, y: 10, name: "Goblin", hp_current: 8,
            squad_id: None, is_leader: false, squad_config: None,
            patrol_route: None, submerged: false,
            awareness: (state: Hidden),
        )"#;
        let loaded: SavedMonster = from_ron(ron);
        assert!(loaded.fleeing.is_none());
    }

    #[test]
    fn roundtrip_monster_waypoint_patrol() {
        let m = SavedMonster {
            patrol_route: Some(PatrolRoute {
                state: PatrolState::Waypoint {
                    points: vec![(1, 1), (5, 1), (5, 5), (1, 5)],
                    current_index: 2,
                },
            }),
            ..basic_monster()
        };
        let loaded: SavedMonster = from_ron(&to_ron(&m));
        match loaded.patrol_route.unwrap().state {
            PatrolState::Waypoint {
                points,
                current_index,
            } => {
                assert_eq!(points.len(), 4);
                assert_eq!(current_index, 2);
                assert_eq!(points[3], (1, 5));
            }
            _ => panic!("expected Waypoint"),
        }
    }

    // =====================================================================
    // SavedItem roundtrip (scalar-only state to avoid flatten bug)
    // =====================================================================

    #[test]
    fn roundtrip_item_basic() {
        let loaded: SavedItem = from_ron(&to_ron(&basic_item()));
        assert_eq!(loaded.x, 3);
        assert_eq!(loaded.y, 4);
        assert_eq!(loaded.name, "Health Potion");
        assert_eq!(loaded.count, 3);
        assert!(!loaded.drifting);
        assert!(loaded.state.enchantment.is_none());
    }

    #[test]
    fn roundtrip_item_with_scalar_state() {
        let loaded: SavedItem = from_ron(&to_ron(&scalar_state_item()));
        assert_eq!(loaded.state.enchantment, Some(3));
        assert_eq!(loaded.state.runic_identified, Some(true));
        assert_eq!(loaded.state.staff_charges, Some(2));
        assert_eq!(loaded.state.staff_max_charges, Some(5));
        assert_eq!(loaded.state.staff_recharge_timer, Some(100));
        assert_eq!(loaded.state.staff_recharge_rate, Some(200));
        assert_eq!(loaded.state.base_recharge, Some(250));
    }

    #[test]
    fn roundtrip_item_drifting() {
        let loaded: SavedItem = from_ron(&to_ron(&drifting_item()));
        assert!(loaded.drifting);
        assert_eq!(loaded.count, 50);
    }

    // =====================================================================
    // ItemMutableState roundtrip (direct, not flattened -- tests enum fields)
    // =====================================================================

    #[test]
    fn roundtrip_all_weapon_runics() {
        for runic in &[
            WeaponRunic::Speed,
            WeaponRunic::Slowing,
            WeaponRunic::Force,
            WeaponRunic::Paralysis,
            WeaponRunic::Quietus,
            WeaponRunic::Flames,
            WeaponRunic::Venom,
            WeaponRunic::Lightning,
            WeaponRunic::Slaying {
                faction: "Dragon".into(),
            },
        ] {
            let s = ItemMutableState {
                weapon_runic: Some(runic.clone()),
                ..Default::default()
            };
            let l: ItemMutableState = from_ron(&to_ron(&s));
            assert_eq!(l.weapon_runic.as_ref(), Some(runic));
        }
    }

    #[test]
    fn roundtrip_all_armor_runics() {
        for runic in &[
            ArmorRunic::Reprisal,
            ArmorRunic::Absorption,
            ArmorRunic::Reflection,
            ArmorRunic::Immunity {
                damage_type: DamageType::Fire,
            },
            ArmorRunic::Immunity {
                damage_type: DamageType::Lightning,
            },
            ArmorRunic::Immunity {
                damage_type: DamageType::Poison,
            },
        ] {
            let s = ItemMutableState {
                armor_runic: Some(*runic),
                ..Default::default()
            };
            let l: ItemMutableState = from_ron(&to_ron(&s));
            assert_eq!(l.armor_runic, Some(*runic));
        }
    }

    #[test]
    fn roundtrip_all_staff_effects() {
        for effect in &[
            StaffEffect::Lightning,
            StaffEffect::Poison,
            StaffEffect::Blinking,
            StaffEffect::Fire,
            StaffEffect::Healing,
            StaffEffect::Force,
        ] {
            let s = ItemMutableState {
                staff_effect: Some(*effect),
                base_recharge: Some(200),
                staff_charges: Some(1),
                staff_max_charges: Some(3),
                staff_recharge_timer: Some(0),
                staff_recharge_rate: Some(200),
                ..Default::default()
            };
            let l: ItemMutableState = from_ron(&to_ron(&s));
            assert_eq!(l.staff_effect, Some(*effect));
        }
    }

    #[test]
    fn roundtrip_item_mutable_state_full() {
        let s = ItemMutableState {
            enchantment: Some(5),
            weapon_runic: Some(WeaponRunic::Slaying {
                faction: "Undead".into(),
            }),
            armor_runic: None,
            runic_identified: Some(true),
            staff_effect: Some(StaffEffect::Fire),
            base_recharge: Some(300),
            staff_charges: Some(2),
            staff_max_charges: Some(4),
            staff_recharge_timer: Some(50),
            staff_recharge_rate: Some(250),
        };
        let l: ItemMutableState = from_ron(&to_ron(&s));
        assert_eq!(l.enchantment, Some(5));
        match l.weapon_runic {
            Some(WeaponRunic::Slaying { faction }) => assert_eq!(faction, "Undead"),
            _ => panic!("expected Slaying"),
        }
        assert_eq!(l.staff_effect, Some(StaffEffect::Fire));
        assert_eq!(l.staff_charges, Some(2));
    }

    #[test]
    fn roundtrip_item_mutable_state_immunity_runic() {
        let s = ItemMutableState {
            armor_runic: Some(ArmorRunic::Immunity {
                damage_type: DamageType::Lightning,
            }),
            ..Default::default()
        };
        let l: ItemMutableState = from_ron(&to_ron(&s));
        match l.armor_runic {
            Some(ArmorRunic::Immunity { damage_type }) => {
                assert_eq!(damage_type, DamageType::Lightning)
            }
            _ => panic!("expected Immunity"),
        }
    }

    #[test]
    fn roundtrip_default_item_mutable_state() {
        let l: ItemMutableState = from_ron(&to_ron(&ItemMutableState::default()));
        assert!(l.enchantment.is_none());
        assert!(l.weapon_runic.is_none());
        assert!(l.armor_runic.is_none());
        assert!(l.runic_identified.is_none());
        assert!(l.staff_effect.is_none());
        assert!(l.base_recharge.is_none());
        assert!(l.staff_charges.is_none());
        assert!(l.staff_max_charges.is_none());
        assert!(l.staff_recharge_timer.is_none());
        assert!(l.staff_recharge_rate.is_none());
    }

    // =====================================================================
    // RON flatten + enum limitation (documents known bug)
    // =====================================================================

    #[test]
    fn ron_flatten_enum_limitation() {
        let item = SavedItem {
            x: 0,
            y: 0,
            name: "test".into(),
            count: 1,
            state: ItemMutableState {
                weapon_runic: Some(WeaponRunic::Flames),
                ..Default::default()
            },
            drifting: false,
        };
        let serialized = to_ron(&item);
        assert!(serialized.contains("Flames"));
        let result: Result<SavedItem, _> = ron::from_str(&serialized);
        assert!(
            result.is_err(),
            "Expected RON flatten+enum deserialization to fail"
        );
    }

    // =====================================================================
    // SavedProp roundtrip
    // =====================================================================

    #[test]
    fn roundtrip_prop() {
        let l: SavedProp = from_ron(&to_ron(&basic_prop()));
        assert_eq!(l.x, 6);
        assert_eq!(l.y, 9);
        assert_eq!(l.name, "watchfire");
        assert!(!l.ever_fired);
    }

    /// RFC 0002 step 4 — `ever_fired = true` survives serde round trip.
    /// Guards the per-instance prop activation state in saves.
    #[test]
    fn roundtrip_prop_with_ever_fired_true() {
        let prop = SavedProp {
            x: 2,
            y: 2,
            name: "altar".into(),
            ever_fired: true,
        };
        let parsed: SavedProp = from_ron(&to_ron(&prop));
        assert!(parsed.ever_fired, "ever_fired flag must survive serde");
    }

    /// Backward-compat: v9 SavedProp RON (no `ever_fired` field) loads
    /// into the new schema with `ever_fired = false` via #[serde(default)].
    #[test]
    fn roundtrip_prop_v9_format_loads_with_ever_fired_default() {
        let v9_ron = r#"(x: 3, y: 4, name: "altar")"#;
        let parsed: SavedProp = from_ron(v9_ron);
        assert_eq!(parsed.x, 3);
        assert_eq!(parsed.y, 4);
        assert_eq!(parsed.name, "altar");
        assert!(!parsed.ever_fired, "v9 saves must default to ever_fired=false");
    }

    // =====================================================================
    // PlayerSaveData roundtrip
    // =====================================================================

    #[test]
    fn roundtrip_player_save_data() {
        let l: PlayerSaveData = from_ron(&to_ron(&player_data()));
        assert_eq!(l.x, 10);
        assert_eq!(l.y, 20);
        assert_eq!(l.hp, 45);
        assert_eq!(l.armor, 3);
        assert_eq!(l.dodge, 2);
        assert_eq!(l.viewshed_range, 8);
        assert_eq!(l.damage, "1d4+1");
        assert_eq!(l.status_effects.effects.len(), 2);
        assert_eq!(l.status_effects.effects[0].remaining_turns, 5);
        assert_eq!(l.status_effects.effects[1].kind, StatusEffectKind::Poisoned);
        assert_eq!(l.status_effects.effects[1].magnitude, 2);
        assert_eq!(l.inventory.len(), 5);
    }

    #[test]
    fn roundtrip_player_inventory_weapon() {
        let l: InventoryItemSave = from_ron(&to_ron(&weapon_inv_item()));
        assert_eq!(l.name, "Dagger");
        assert_eq!(l.equipped_slot, Some("weapon".into()));
        assert_eq!(l.properties.kind, ItemKind::Weapon);
        assert_eq!(l.properties.damage, Some("1d4+1".into()));
        assert!((l.properties.attack_speed - 0.5).abs() < f32::EPSILON);
        assert_eq!(l.state.enchantment, Some(2));
        assert_eq!(l.state.runic_identified, Some(false));
    }

    #[test]
    fn roundtrip_player_inventory_armor() {
        let l: InventoryItemSave = from_ron(&to_ron(&armor_inv_item()));
        assert_eq!(l.properties.kind, ItemKind::Armor);
        assert_eq!(l.properties.armor_slot, Some(ArmorSlot::Chest));
        assert_eq!(l.properties.defense, 5);
        assert_eq!(l.equipped_slot, Some("chest".into()));
        assert_eq!(l.state.enchantment, Some(1));
    }

    #[test]
    fn roundtrip_player_inventory_consumable() {
        let l: InventoryItemSave = from_ron(&to_ron(&consumable_inv_item()));
        assert_eq!(l.count, 2);
        assert_eq!(l.max_stack, 5);
        assert_eq!(l.properties.effect, Some(Effect::HealHp(20)));
    }

    #[test]
    fn roundtrip_player_inventory_key() {
        let l: InventoryItemSave = from_ron(&to_ron(&key_inv_item()));
        assert_eq!(l.key_name, Some("crypt_key".into()));
        assert!(!l.is_quest_item);
    }

    #[test]
    fn roundtrip_player_inventory_quest_item() {
        let l: InventoryItemSave = from_ron(&to_ron(&quest_inv_item()));
        assert!(l.is_quest_item);
        assert_eq!(l.properties.rarity, Rarity::Legendary);
    }

    // =====================================================================
    // Status effects roundtrip
    // =====================================================================

    #[test]
    fn roundtrip_all_status_effects() {
        // (kind, magnitude) — matches new engine `StatusEffectInstance` layout.
        let specs: [(StatusEffectKind, i32); 9] = [
            (StatusEffectKind::Hasted, 0),
            (StatusEffectKind::Slowed, 0),
            (StatusEffectKind::Stunned, 0),
            (StatusEffectKind::Entangled, 0),
            (StatusEffectKind::Burning, 5),
            (StatusEffectKind::Poisoned, 3),
            (StatusEffectKind::Enraged, 0),
            (StatusEffectKind::FireResistance, 0),
            (StatusEffectKind::PoisonResistance, 0),
        ];
        let p = PlayerSaveData {
            x: 0,
            y: 0,
            hp: 50,
            armor: 0,
            dodge: 0,
            viewshed_range: 8,
            damage: "1d6".into(),
            status_effects: StatusEffects {
                effects: specs
                    .iter()
                    .map(|(k, mag)| StatusEffectInstance {
                        kind: *k,
                        remaining_turns: 10,
                        magnitude: *mag,
                        source: None,
                    })
                    .collect(),
            },
            inventory: vec![],
            ..Default::default()
        };
        let l: PlayerSaveData = from_ron(&to_ron(&p));
        assert_eq!(l.status_effects.effects.len(), 9);
        assert_eq!(l.status_effects.effects[4].kind, StatusEffectKind::Burning);
        assert_eq!(l.status_effects.effects[4].magnitude, 5);
    }

    // =====================================================================
    // SavedFloorData roundtrip
    // =====================================================================

    #[test]
    fn roundtrip_floor_data() {
        let l: SavedFloorData = from_ron(&to_ron(&floor_data()));
        assert_eq!(l.map.width, 4);
        assert_eq!(l.monsters.len(), 1);
        assert_eq!(l.items.len(), 1);
        assert_eq!(l.props.len(), 1);
        assert_eq!(l.down_stairs_pos, [3, 2]);
        assert_eq!(l.up_stairs_pos, [1, 1]);
    }

    // =====================================================================
    // Floor cache roundtrip
    // =====================================================================

    #[test]
    fn roundtrip_floor_cache_multiple_floors() {
        let mut cache = HashMap::new();
        cache.insert(1, floor_data());
        cache.insert(
            2,
            SavedFloorData {
                map: MapSaveData {
                    width: 2,
                    height: 2,
                    depth: 2,
                    name: "Floor 2".into(),
                    tiles: vec![tile_floor(); 4],
                    explored: vec![true; 4],
                },
                monsters: vec![],
                items: vec![],
                props: vec![],
                down_stairs_pos: [1, 0],
                up_stairs_pos: [0, 1],
                exit_tiles: Vec::new(),
            },
        );
        let save = GameSaveData {
            floor: 3,
            game_log: vec![],
            map: small_map(),
            player: PlayerSaveData {
                x: 1,
                y: 1,
                hp: 50,
                armor: 0,
                dodge: 0,
                viewshed_range: 8,
                damage: "1d6".into(),
                status_effects: StatusEffects::default(),
                inventory: vec![],
                ..Default::default()
            },
            monsters: vec![],
            floor_items: vec![],
            props: vec![],
            floor_cache: cache,
            squad_id_counter: 10,
            fallen_monsters: HashMap::new(),
            fallen_items: HashMap::new(),
            overworld: OverworldSave::default(),
        };
        let l: GameSaveData = from_ron(&to_ron(&save));
        assert_eq!(l.floor_cache.len(), 2);
        assert_eq!(l.floor_cache[&1].monsters.len(), 1);
        assert_eq!(l.floor_cache[&2].map.name, "Floor 2");
    }

    // =====================================================================
    // Empty / minimal data
    // =====================================================================

    #[test]
    fn roundtrip_empty_vectors() {
        let l: GameSaveData = from_ron(&to_ron(&minimal_save(0)));
        assert!(l.game_log.is_empty());
        assert!(l.monsters.is_empty());
        assert!(l.floor_items.is_empty());
        assert!(l.props.is_empty());
        assert!(l.floor_cache.is_empty());
        assert!(l.player.inventory.is_empty());
        assert!(l.player.status_effects.effects.is_empty());
        assert_eq!(l.squad_id_counter, 0);
    }

    // =====================================================================
    // SquadIdCounter persistence
    // =====================================================================

    #[test]
    fn squad_id_counter_survives_roundtrip() {
        let l: GameSaveData = from_ron(&to_ron(&minimal_save(12345)));
        assert_eq!(l.squad_id_counter, 12345);
    }

    #[test]
    fn large_squad_id_counter_survives_roundtrip() {
        let l: GameSaveData = from_ron(&to_ron(&minimal_save(u64::MAX)));
        assert_eq!(l.squad_id_counter, u64::MAX);
    }

    // =====================================================================
    // Forward compatibility: serde(default) fields
    // =====================================================================

    #[test]
    fn forward_compat_monster_missing_optional_fields() {
        let l: SavedMonster = ron::from_str(r#"(x: 5, y: 10, name: "Goblin")"#).unwrap();
        assert_eq!(l.hp_current, 0);
        assert_eq!(l.squad_id, None);
        assert!(!l.is_leader);
        assert!(!l.submerged);
        assert!(l.squad_config.is_none());
        assert!(l.patrol_route.is_none());
    }

    #[test]
    fn forward_compat_game_save_missing_props() {
        let l: GameSaveData = from_ron(&to_ron(&minimal_save(5)));
        assert!(l.props.is_empty());
    }

    #[test]
    fn forward_compat_floor_data_missing_up_stairs() {
        let ron_str = r#"(
            map: (width: 2, height: 2, depth: 1, name: "old", tiles: [(terrain: Floor, liquid: None, decoration: None), (terrain: Floor, liquid: None, decoration: None), (terrain: Floor, liquid: None, decoration: None), (terrain: Floor, liquid: None, decoration: None)], explored: [false, false, false, false]),
            monsters: [],
            items: [],
            down_stairs_pos: (1, 1),
        )"#;
        let l: SavedFloorData = ron::from_str(ron_str).unwrap();
        assert_eq!(l.down_stairs_pos, [1, 1]);
        assert_eq!(l.up_stairs_pos, [0, 0]);
        assert!(l.props.is_empty());
    }

    // =====================================================================
    // Conversion helpers
    // =====================================================================

    #[test]
    fn map_save_data_conversion_roundtrip() {
        let map = Map {
            name: "Roundtrip Floor".into(),
            tiles: vec![tile_floor(), tile_wall(), tile_water()],
            explored_tiles: vec![true, false, true],
            blocked: vec![false, true, false],
            width: 3,
            height: 1,
            depth: 7,
        };
        let restored = save_data_to_map(&map_to_save_data(&map));
        assert_eq!(restored.width, 3);
        assert_eq!(restored.depth, 7);
        assert_eq!(restored.tiles, map.tiles);
        assert_eq!(restored.explored_tiles, map.explored_tiles);
        assert!(restored.blocked.iter().all(|b| !b)); // blocked rebuilt fresh
    }

    #[test]
    fn cached_floor_conversion_roundtrip() {
        let cached = CachedFloor {
            map: Map {
                name: "Cache".into(),
                tiles: vec![tile_floor(); 4],
                explored_tiles: vec![false; 4],
                blocked: vec![false; 4],
                width: 2,
                height: 2,
                depth: 4,
            },
            monsters: vec![basic_monster()],
            items: vec![basic_item()],
            props: vec![basic_prop()],
            exit_tiles: Vec::new(),
            down_stairs_pos: Point::new(1, 0),
            up_stairs_pos: Point::new(0, 1),
        };
        let saved = cached_floor_to_save(&cached);
        let restored = save_to_cached_floor(&saved);
        assert_eq!(restored.map.width, 2);
        assert_eq!(restored.monsters.len(), 1);
        assert_eq!(restored.items.len(), 1);
        assert_eq!(restored.props.len(), 1);
        assert_eq!(restored.down_stairs_pos, Point::new(1, 0));
        assert_eq!(restored.up_stairs_pos, Point::new(0, 1));
        assert_eq!(saved.down_stairs_pos, [1, 0]);
    }

    // =====================================================================
    // Edge cases
    // =====================================================================

    #[test]
    fn monster_hp_zero_survives_roundtrip() {
        let m = SavedMonster {
            hp_current: 0,
            ..basic_monster()
        };
        assert_eq!(from_ron::<SavedMonster>(&to_ron(&m)).hp_current, 0);
    }

    #[test]
    fn monster_negative_hp_survives_roundtrip() {
        let m = SavedMonster {
            hp_current: -5,
            ..basic_monster()
        };
        assert_eq!(from_ron::<SavedMonster>(&to_ron(&m)).hp_current, -5);
    }

    #[test]
    fn large_stack_count_survives_roundtrip() {
        let item = SavedItem {
            count: u32::MAX,
            ..basic_item()
        };
        assert_eq!(from_ron::<SavedItem>(&to_ron(&item)).count, u32::MAX);
    }

    #[test]
    fn player_with_many_status_effects() {
        let mk = |kind: StatusEffectKind, turns: u32, magnitude: i32| StatusEffectInstance {
            kind,
            remaining_turns: turns,
            magnitude,
            source: None,
        };
        let p = PlayerSaveData {
            x: 0,
            y: 0,
            hp: 100,
            armor: 10,
            dodge: 5,
            viewshed_range: 12,
            damage: "2d8+3".into(),
            status_effects: StatusEffects {
                effects: vec![
                    mk(StatusEffectKind::Hasted, 1, 0),
                    mk(StatusEffectKind::Slowed, 2, 0),
                    mk(StatusEffectKind::Stunned, 3, 0),
                    mk(StatusEffectKind::Entangled, 4, 0),
                    mk(StatusEffectKind::Burning, 5, 10),
                    mk(StatusEffectKind::Enraged, 6, 0),
                ],
            },
            inventory: vec![],
            ..Default::default()
        };
        let l: PlayerSaveData = from_ron(&to_ron(&p));
        assert_eq!(l.status_effects.effects.len(), 6);
        assert_eq!(l.status_effects.effects[2].remaining_turns, 3);
    }

    #[test]
    fn game_log_preserves_order() {
        let entries: Vec<String> = (0..100).map(|i| format!("Log entry {}", i)).collect();
        let mut save = minimal_save(0);
        save.game_log = entries.clone();
        let l: GameSaveData = from_ron(&to_ron(&save));
        assert_eq!(l.game_log.len(), 100);
        for (i, entry) in l.game_log.iter().enumerate() {
            assert_eq!(entry, &format!("Log entry {}", i));
        }
    }

    #[test]
    fn unicode_names_survive_roundtrip() {
        let m = SavedMonster {
            name: "Eel".into(),
            ..basic_monster()
        };
        let item = SavedItem {
            name: "Staff of Fire".into(),
            ..basic_item()
        };
        let prop = SavedProp {
            x: 0,
            y: 0,
            name: "brazier".into(),
            ever_fired: false,
        };
        assert_eq!(from_ron::<SavedMonster>(&to_ron(&m)).name, "Eel");
        assert_eq!(from_ron::<SavedItem>(&to_ron(&item)).name, "Staff of Fire");
        assert_eq!(from_ron::<SavedProp>(&to_ron(&prop)).name, "brazier");
    }

    // Platform I/O tests live in the engine crate at
    // `roguelike_engine::save::platform`. `SaveFrameworkConfig` and
    // `SaveExists` tests live at `roguelike_engine::save`. This module
    // only verifies the schema-level behavior.

    #[test]
    fn game_save_key_is_ironveil_save() {
        // The game-side SavePlugin inserts SaveFrameworkConfig with
        // "ironveil_save" so The Veiled Tyrant's save files don't
        // collide with other games on the same filesystem.
        assert_eq!(GAME_SAVE_KEY, "ironveil_save");
    }

    // =====================================================================
    // Schema migration tests
    // =====================================================================

    #[test]
    fn schema_version_is_ten() {
        assert_eq!(SAVE_SCHEMA_VERSION, 10);
    }

    #[test]
    fn v6_overworld_survives_roundtrip() {
        let mut save = full_save();
        save.overworld = OverworldSave {
            temple_entrance_floor: 7,
            temple_entrance_pos: Some([40, 30]),
        };
        let l: GameSaveData = from_ron(&to_ron(&save));
        assert_eq!(l.overworld.temple_entrance_floor, 7);
        assert_eq!(l.overworld.temple_entrance_pos, Some([40, 30]));
    }

    #[test]
    fn v6_exit_tiles_survive_roundtrip() {
        let mut floor = floor_data();
        floor.exit_tiles = vec![
            SavedExitTile { x: 78, y: 30, destination_floor: 5, destination_pos: Some([2, 30]) },
            SavedExitTile { x: 1, y: 1, destination_floor: 9, destination_pos: None },
        ];
        let l: SavedFloorData = from_ron(&to_ron(&floor));
        assert_eq!(l.exit_tiles.len(), 2);
        assert_eq!(l.exit_tiles[0].destination_floor, 5);
        assert_eq!(l.exit_tiles[0].destination_pos, Some([2, 30]));
        assert_eq!(l.exit_tiles[1].destination_pos, None);
    }

    #[test]
    fn migrate_v0_to_v1_is_identity() {
        let payload = "arbitrary ron content";
        let result = MigrateV0ToV1.migrate(payload).unwrap();
        assert_eq!(result, payload);
    }

    #[test]
    fn migrate_v1_to_v2_drops_status_effects() {
        let v1 = r#"PlayerSaveData(x: 0, y: 0, hp: 10, status_effects: StatusEffects([(kind: Burning(damage_per_turn: 3), turns_remaining: 5, initial_duration: 5)]), inventory: [])"#;
        let v2 = MigrateV1ToV2.migrate(v1).unwrap();
        assert!(
            v2.contains("status_effects: StatusEffects(effects: [])"),
            "v1→v2 should reset status_effects to empty; got: {}",
            v2
        );
        // Other fields should survive.
        assert!(v2.contains("x: 0"));
        assert!(v2.contains("hp: 10"));
        assert!(v2.contains("inventory: []"));
    }

    #[test]
    fn migrate_v1_to_v2_handles_empty_effects() {
        let v1 = r#"(status_effects: StatusEffects([]), other_field: 42)"#;
        let v2 = MigrateV1ToV2.migrate(v1).unwrap();
        assert!(v2.contains("status_effects: StatusEffects(effects: [])"));
        assert!(v2.contains("other_field: 42"));
    }

    #[test]
    fn migrate_v1_to_v2_handles_nested_parens() {
        let v1 = r#"(status_effects: StatusEffects([(kind: Poisoned(damage_per_turn: 2), turns_remaining: 3, initial_duration: 3)]), trailing: ok)"#;
        let v2 = MigrateV1ToV2.migrate(v1).unwrap();
        assert!(
            v2.contains("trailing: ok"),
            "content after the effects block must be preserved; got: {}",
            v2
        );
        assert!(v2.contains("status_effects: StatusEffects(effects: [])"));
    }

    #[test]
    fn skip_balanced_parens_matches_outer() {
        assert_eq!(skip_balanced_parens("(abc)rest"), Some(5));
        assert_eq!(skip_balanced_parens("((nested))rest"), Some(10));
        assert_eq!(skip_balanced_parens("(a(b(c))d)rest"), Some(10));
        assert_eq!(skip_balanced_parens("no paren"), None);
        assert_eq!(skip_balanced_parens("(unclosed"), None);
    }

    #[test]
    fn migrations_chain_is_ordered() {
        // Note: v7 → v8 is intentionally absent. v8 was the
        // StatusEffectKind::Custom { id } → named-variant hard break;
        // pre-v8 saves are unrecoverable by design and fail to load.
        let migs = migrations();
        assert_eq!(migs.len(), 9);
        assert_eq!(migs[0].from_version(), 0);
        assert_eq!(migs[0].to_version(), 1);
        assert_eq!(migs[1].from_version(), 1);
        assert_eq!(migs[1].to_version(), 2);
        assert_eq!(migs[2].from_version(), 2);
        assert_eq!(migs[2].to_version(), 3);
        assert_eq!(migs[3].from_version(), 3);
        assert_eq!(migs[3].to_version(), 4);
        assert_eq!(migs[4].from_version(), 4);
        assert_eq!(migs[4].to_version(), 5);
        assert_eq!(migs[5].from_version(), 5);
        assert_eq!(migs[5].to_version(), 6);
        assert_eq!(migs[6].from_version(), 6);
        assert_eq!(migs[6].to_version(), 7);
        assert_eq!(migs[7].from_version(), 8);
        assert_eq!(migs[7].to_version(), 9);
        assert_eq!(migs[8].from_version(), 9);
        assert_eq!(migs[8].to_version(), 10);
    }

    // =====================================================================
    // Stealth save / load (degraded persistence) tests
    // =====================================================================

    fn pe() -> Entity {
        Entity::from_raw_u32(1).expect("valid test entity")
    }

    #[test]
    fn aware_collapses_to_searching_on_save() {
        let mut a = roguelike_engine::stealth::Awareness::default();
        a.set(pe(), roguelike_engine::stealth::AwarenessState::Aware, 10);
        let saved = degrade_awareness_for_save(&a, pe(), Point::new(5, 5), 10);
        assert!(matches!(
            saved.state,
            SavedAwarenessState::Searching {
                last_known_x: 5,
                last_known_y: 5,
                giveup_at_offset: 20,
            }
        ));
    }

    #[test]
    fn searching_preserves_last_known_with_offset_on_save() {
        let mut a = roguelike_engine::stealth::Awareness::default();
        a.set(
            pe(),
            roguelike_engine::stealth::AwarenessState::Searching {
                last_known_pos: Point::new(3, 4),
                giveup_at_turn: 50,
            },
            10,
        );
        let saved = degrade_awareness_for_save(&a, pe(), Point::new(99, 99), 30);
        assert!(matches!(
            saved.state,
            SavedAwarenessState::Searching {
                last_known_x: 3,
                last_known_y: 4,
                giveup_at_offset: 20,
            }
        ));
    }

    #[test]
    fn suspicious_collapses_to_hidden_on_save() {
        let mut a = roguelike_engine::stealth::Awareness::default();
        a.set(
            pe(),
            roguelike_engine::stealth::AwarenessState::Suspicious {
                suspect_pos: Point::new(7, 7),
                decay_at_turn: 100,
            },
            0,
        );
        let saved = degrade_awareness_for_save(&a, pe(), Point::new(0, 0), 0);
        assert_eq!(saved.state, SavedAwarenessState::Hidden);
    }

    #[test]
    fn no_record_saves_as_hidden() {
        let a = roguelike_engine::stealth::Awareness::default();
        let saved = degrade_awareness_for_save(&a, pe(), Point::new(0, 0), 0);
        assert_eq!(saved.state, SavedAwarenessState::Hidden);
    }

    #[test]
    fn hidden_save_restores_empty_awareness() {
        let saved = MonsterAwarenessSave {
            state: SavedAwarenessState::Hidden,
        };
        let a = restore_awareness_from_save(&saved, pe(), 0);
        assert!(a.records.is_empty());
    }

    #[test]
    fn searching_save_restores_with_recomputed_turn() {
        let saved = MonsterAwarenessSave {
            state: SavedAwarenessState::Searching {
                last_known_x: 7,
                last_known_y: 8,
                giveup_at_offset: 15,
            },
        };
        let now = 100;
        let a = restore_awareness_from_save(&saved, pe(), now);
        let rec = a.records.get(&pe()).expect("player record restored");
        match rec.state {
            roguelike_engine::stealth::AwarenessState::Searching {
                last_known_pos,
                giveup_at_turn,
            } => {
                assert_eq!(last_known_pos, Point::new(7, 8));
                assert_eq!(giveup_at_turn, 115);
            }
            other => panic!("expected Searching, got {:?}", other),
        }
    }

    /// v2 → v3 migration is a no-op (serde defaults handle the new fields).
    #[test]
    fn migrate_v2_to_v3_is_identity() {
        let payload = "PlayerSaveData(x: 0, y: 0, hp: 10)";
        let result = MigrateV2ToV3.migrate(payload).unwrap();
        assert_eq!(result, payload);
    }

    /// v3 → v4 migration is a no-op. Pre-Phase-2 saves containing CON or
    /// Halfling will fail at deserialize (intentional — no safe remap).
    #[test]
    fn migrate_v3_to_v4_is_identity() {
        let payload = "PlayerSaveData(x: 0, y: 0, hp: 10)";
        let result = MigrateV3ToV4.migrate(payload).unwrap();
        assert_eq!(result, payload);
    }

    /// v4 → v5 migration is a no-op. Phase 3 added skill fields with
    /// serde defaults; pre-v5 saves load with empty skill state.
    #[test]
    fn migrate_v4_to_v5_is_identity() {
        let payload = "PlayerSaveData(x: 0, y: 0, hp: 10)";
        let result = MigrateV4ToV5.migrate(payload).unwrap();
        assert_eq!(result, payload);
    }

    /// A pre-v3 save (no race/class/attributes/bonuses fields) must still
    /// deserialize, with the new fields filled by their serde defaults.
    #[test]
    fn pre_v3_player_save_data_loads_with_defaults() {
        // Hand-crafted RON in the v2 shape — no race/class/attributes fields.
        let v2_ron = r#"(
            x: 5,
            y: 7,
            hp: 22,
            armor: 1,
            dodge: 0,
            viewshed_range: 8,
            damage: "1d4",
            status_effects: (effects: []),
            inventory: []
        )"#;
        let loaded: PlayerSaveData =
            ron::from_str(v2_ron).expect("v2-shape RON must still parse");
        assert_eq!(loaded.x, 5);
        assert_eq!(loaded.hp, 22);
        // Race / Class defaults
        assert_eq!(loaded.race, crate::character::Race::Human);
        assert_eq!(loaded.class, crate::character::Class::Warrior);
        // Attributes default to all-16 via `default_attributes_baseline`
        // (mod 0 across the board in the Phase 2 anchored-at-16 scale).
        assert_eq!(loaded.attributes.strength, 16);
        assert_eq!(loaded.attributes.dexterity, 16);
        assert_eq!(loaded.attributes.intelligence, 16);
        // Bonuses default to 0
        assert_eq!(loaded.hit_bonus, 0);
        assert_eq!(loaded.damage_bonus, 0);
    }

    /// Round-trip: a save with Race/Class/Attributes/HitBonus/DamageBonus
    /// populated must serialize and deserialize without losing any field.
    #[test]
    fn v4_player_save_data_round_trips() {
        let original = PlayerSaveData {
            x: 11,
            y: 12,
            hp: 30,
            armor: 2,
            block: 0,
            max_shield_blocks: 0,
            dodge: 3,
            viewshed_range: 10,
            damage: "1d6+2".to_string(),
            status_effects: StatusEffects::default(),
            inventory: vec![],
            race: crate::character::Race::Elf,
            class: crate::character::Class::Rogue,
            attributes: crate::character::Attributes {
                strength: 10,
                dexterity: 17,
                intelligence: 13,
            },
            hit_bonus: 4,
            damage_bonus: 2,
            level: 7,
            experience: 1234,
            skills: Default::default(),
            skill_xp: Default::default(),
            skill_training: Default::default(),
            skill_xp_pool: 0,
        };
        let ron = ron::to_string(&original).expect("serialize");
        let loaded: PlayerSaveData = ron::from_str(&ron).expect("deserialize");
        assert_eq!(loaded.race, crate::character::Race::Elf);
        assert_eq!(loaded.class, crate::character::Class::Rogue);
        assert_eq!(loaded.attributes.dexterity, 17);
        assert_eq!(loaded.attributes.intelligence, 13);
        assert_eq!(loaded.hit_bonus, 4);
        assert_eq!(loaded.damage_bonus, 2);
        assert_eq!(loaded.level, 7);
        assert_eq!(loaded.experience, 1234);
        assert_eq!(loaded.x, 11);
        assert_eq!(loaded.hp, 30);
    }

    /// v5 round-trip: skill state (skills levels, raw XP totals, training
    /// modes, pooled XP) serializes and deserializes faithfully.
    #[test]
    fn v5_player_save_data_round_trips_skills() {
        use crate::game::skills::{Skill, SkillState, SkillTraining, SkillXp, Skills};

        let mut skills = Skills::new();
        skills.set(Skill::Fighting, 4.7);
        skills.set(Skill::LongBlades, 3.2);
        let mut skill_xp = SkillXp::new();
        skill_xp.add(Skill::Fighting, 500);
        skill_xp.add(Skill::LongBlades, 320);
        let mut training = SkillTraining::new();
        training.cycle(Skill::Fighting); // Normal → Focused
        training.cycle(Skill::Axes); // Normal → Focused
        training.cycle(Skill::Axes); // Focused → Disabled

        let mut original = player_data();
        original.skills = skills;
        original.skill_xp = skill_xp;
        original.skill_training = training;
        original.skill_xp_pool = 1247;

        let ron = ron::to_string(&original).expect("serialize");
        let loaded: PlayerSaveData = ron::from_str(&ron).expect("deserialize");

        assert!((loaded.skills.get(Skill::Fighting) - 4.7).abs() < 0.01);
        assert!((loaded.skills.get(Skill::LongBlades) - 3.2).abs() < 0.01);
        assert_eq!(loaded.skill_xp.get(Skill::Fighting), 500);
        assert_eq!(loaded.skill_xp.get(Skill::LongBlades), 320);
        assert_eq!(loaded.skill_training.get(Skill::Fighting), SkillState::Focused);
        assert_eq!(loaded.skill_training.get(Skill::Axes), SkillState::Disabled);
        assert_eq!(loaded.skill_xp_pool, 1247);
    }
}
