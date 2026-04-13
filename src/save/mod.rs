//! Save framework scaffolding.
//!
//! The engine provides:
//!
//! - [`platform`] — platform-agnostic I/O primitives (native RON files,
//!   WASM `localStorage`) keyed by a caller-supplied string.
//! - [`SaveFrameworkConfig`] — a Bevy resource that names the save key
//!   a game uses; inspected by game-side save systems (typically
//!   `auto_save_system`) before calling the platform layer.
//! - [`SaveExists`] — a simple Bevy resource indicating whether a save
//!   file is currently present. Menus and main screens read this to
//!   enable/disable "Continue" buttons.
//! - [`SaveEnvelope`] — versioned wrapper around save data.
//! - [`save_with_version`] / [`load_with_version`] — version-aware
//!   save/load that embed schema version metadata.
//! - [`SaveMigration`] / [`apply_migrations`] — trait and runner for
//!   upgrading save data across schema versions.
//!
//! Games layer their own schema and serialization on top.

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

pub mod platform;

/// The default save key used if a game doesn't configure its own.
///
/// Games that care about branding should insert their own
/// [`SaveFrameworkConfig`] before `SavePlugin` runs.
pub const DEFAULT_SAVE_KEY: &str = "game_save";

/// Per-game configuration for the save framework.
///
/// Games override the default key by inserting
/// `SaveFrameworkConfig { save_key: "my_game_save".into(), schema_version: 1 }`
/// before the save plugin runs (or calling
/// `app.insert_resource(SaveFrameworkConfig { .. })`).
///
/// The default value uses [`DEFAULT_SAVE_KEY`] — games SHOULD override
/// this to a game-specific string so multiple games can coexist on the
/// same filesystem / `localStorage` without overwriting each other.
#[derive(Resource, Clone, Debug)]
pub struct SaveFrameworkConfig {
    pub save_key: String,
    /// The current schema version for this game's saves.
    pub schema_version: u32,
}

impl Default for SaveFrameworkConfig {
    fn default() -> Self {
        Self {
            save_key: DEFAULT_SAVE_KEY.to_string(),
            schema_version: 0,
        }
    }
}

/// Whether a save file currently exists for the configured save key.
///
/// Populated at startup and on entering the main menu by the game's
/// save plugin. Menus read it to enable or disable a "Continue" option.
#[derive(Resource, Default)]
pub struct SaveExists(pub bool);

/// Versioned wrapper around save data. When writing saves, games wrap
/// their serialized payload in an envelope that records the schema version.
/// On load, the version is checked and migrations applied if needed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveEnvelope {
    /// Schema version of the save data.
    pub schema_version: u32,
    /// Engine version string at the time of saving (informational).
    pub engine_version: String,
    /// The actual save payload (typically RON-serialized game state).
    pub payload: String,
}

/// Errors that can occur when loading a versioned save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveLoadError {
    /// No save file found for the key.
    NotFound,
    /// The save file exists but is not a valid envelope.
    /// The raw data is included so callers can attempt recovery.
    CorruptedEnvelope(String),
}

/// The envelope marker that distinguishes versioned saves from legacy ones.
const ENVELOPE_MARKER: &str = "roguelike_engine::SaveEnvelope";

/// Write a versioned save. Wraps `payload` in a [`SaveEnvelope`] and
/// writes it via the platform layer.
///
/// The envelope uses a simple text format with a magic header so that
/// unversioned (legacy) saves can be detected and treated as version 0.
pub fn save_with_version(key: &str, payload: &str, schema_version: u32) -> bool {
    let engine_version = env!("CARGO_PKG_VERSION");
    let data = format!(
        "{}\n{}\n{}\n{}",
        ENVELOPE_MARKER, schema_version, engine_version, payload
    );
    platform::write_bytes(key, &data)
}

/// Load a versioned save. Returns the schema version and payload.
///
/// If the save exists but does not have the envelope marker, it is
/// treated as a legacy (version 0) save — the entire contents are
/// returned as the payload.
pub fn load_with_version(key: &str) -> Result<(u32, String), SaveLoadError> {
    let raw = platform::read_bytes(key).ok_or(SaveLoadError::NotFound)?;

    if raw.starts_with(ENVELOPE_MARKER) {
        // Parse the envelope: marker\nversion\nengine_version\npayload...
        let mut lines = raw.splitn(4, '\n');
        let _marker = lines.next(); // Already verified via starts_with
        let version_str = lines
            .next()
            .ok_or_else(|| SaveLoadError::CorruptedEnvelope(raw.clone()))?;
        let _engine_version = lines
            .next()
            .ok_or_else(|| SaveLoadError::CorruptedEnvelope(raw.clone()))?;
        let payload = lines
            .next()
            .ok_or_else(|| SaveLoadError::CorruptedEnvelope(raw.clone()))?;

        let version: u32 = version_str
            .parse()
            .map_err(|_| SaveLoadError::CorruptedEnvelope(raw.clone()))?;

        Ok((version, payload.to_string()))
    } else {
        // Legacy save — treat as version 0
        Ok((0, raw))
    }
}

/// A single migration step that transforms save data from one schema
/// version to the next. Games implement this for each version bump.
pub trait SaveMigration {
    /// The version this migration upgrades FROM.
    fn from_version(&self) -> u32;
    /// The version this migration upgrades TO.
    fn to_version(&self) -> u32;
    /// Transform the payload data. Returns the migrated payload or an error.
    fn migrate(&self, data: &str) -> Result<String, String>;
}

/// Apply a chain of migrations to bring save data up to the target version.
///
/// Migrations are applied in order. Each migration's `from_version` must
/// match the current version, and `to_version` must be greater. Returns
/// the final migrated payload.
pub fn apply_migrations(
    payload: &str,
    from_version: u32,
    target_version: u32,
    migrations: &[&dyn SaveMigration],
) -> Result<String, String> {
    if from_version >= target_version {
        return Ok(payload.to_string());
    }

    let mut current = payload.to_string();
    let mut version = from_version;

    while version < target_version {
        let migration = migrations
            .iter()
            .find(|m| m.from_version() == version)
            .ok_or_else(|| {
                format!(
                    "No migration found from version {} (target: {})",
                    version, target_version
                )
            })?;

        current = migration.migrate(&current)?;
        version = migration.to_version();
    }

    Ok(current)
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_default_key() {
        let cfg = SaveFrameworkConfig::default();
        assert_eq!(cfg.save_key, DEFAULT_SAVE_KEY);
        assert_eq!(cfg.save_key, "game_save");
    }

    #[test]
    fn config_key_is_mutable() {
        let mut cfg = SaveFrameworkConfig::default();
        cfg.save_key = "my_rpg_save".into();
        assert_eq!(cfg.save_key, "my_rpg_save");
    }

    #[test]
    fn save_exists_default_is_false() {
        let e = SaveExists::default();
        assert!(!e.0);
    }

    #[test]
    fn config_with_version() {
        let cfg = SaveFrameworkConfig {
            save_key: "test".into(),
            schema_version: 5,
        };
        assert_eq!(cfg.schema_version, 5);
    }

    #[test]
    fn default_config_schema_version_is_zero() {
        let cfg = SaveFrameworkConfig::default();
        assert_eq!(cfg.schema_version, 0);
    }

    // --- unique key helper (mirrors platform::tests) ---

    fn unique_test_key() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("_engine_save_mod_{}_{}", std::process::id(), n)
    }

    // --- versioned save/load tests ---

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn versioned_save_roundtrip() {
        let key = unique_test_key();
        save_with_version(&key, "test payload data", 3);
        let (version, payload) = load_with_version(&key).unwrap();
        assert_eq!(version, 3);
        assert_eq!(payload, "test payload data");
        platform::delete(&key);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn legacy_save_detected_as_version_zero() {
        let key = unique_test_key();
        platform::write_bytes(&key, "some old save data");
        let (version, payload) = load_with_version(&key).unwrap();
        assert_eq!(version, 0);
        assert_eq!(payload, "some old save data");
        platform::delete(&key);
    }

    #[test]
    fn load_nonexistent_returns_not_found() {
        let result = load_with_version("nonexistent_key_xyz");
        assert_eq!(result, Err(SaveLoadError::NotFound));
    }

    // --- migration tests ---

    #[test]
    fn apply_migrations_no_op_when_at_target() {
        let result = apply_migrations("data", 3, 3, &[]);
        assert_eq!(result.unwrap(), "data");
    }

    #[test]
    fn apply_migrations_single_step() {
        struct V1ToV2;
        impl SaveMigration for V1ToV2 {
            fn from_version(&self) -> u32 {
                1
            }
            fn to_version(&self) -> u32 {
                2
            }
            fn migrate(&self, data: &str) -> Result<String, String> {
                Ok(format!("{}_migrated", data))
            }
        }
        let result = apply_migrations("data", 1, 2, &[&V1ToV2]);
        assert_eq!(result.unwrap(), "data_migrated");
    }

    #[test]
    fn apply_migrations_chain() {
        struct V1ToV2;
        impl SaveMigration for V1ToV2 {
            fn from_version(&self) -> u32 {
                1
            }
            fn to_version(&self) -> u32 {
                2
            }
            fn migrate(&self, data: &str) -> Result<String, String> {
                Ok(format!("{}_v2", data))
            }
        }
        struct V2ToV3;
        impl SaveMigration for V2ToV3 {
            fn from_version(&self) -> u32 {
                2
            }
            fn to_version(&self) -> u32 {
                3
            }
            fn migrate(&self, data: &str) -> Result<String, String> {
                Ok(format!("{}_v3", data))
            }
        }
        let result = apply_migrations("data", 1, 3, &[&V1ToV2, &V2ToV3]);
        assert_eq!(result.unwrap(), "data_v2_v3");
    }

    #[test]
    fn apply_migrations_missing_step_errors() {
        let result = apply_migrations("data", 1, 3, &[]);
        assert!(result.is_err());
    }
}
