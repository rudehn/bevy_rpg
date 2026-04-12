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
//!
//! Games layer their own schema and serialization on top.

use bevy::prelude::Resource;

pub mod platform;

/// The default save key used if a game doesn't configure its own.
///
/// Games that care about branding should insert their own
/// [`SaveFrameworkConfig`] before `SavePlugin` runs.
pub const DEFAULT_SAVE_KEY: &str = "game_save";

/// Per-game configuration for the save framework.
///
/// Games override the default key by inserting
/// `SaveFrameworkConfig { save_key: "my_game_save".into() }` before
/// the save plugin runs (or calling
/// `app.insert_resource(SaveFrameworkConfig { .. })`).
///
/// The default value uses [`DEFAULT_SAVE_KEY`] — games SHOULD override
/// this to a game-specific string so multiple games can coexist on the
/// same filesystem / `localStorage` without overwriting each other.
#[derive(Resource, Clone, Debug)]
pub struct SaveFrameworkConfig {
    pub save_key: String,
}

impl Default for SaveFrameworkConfig {
    fn default() -> Self {
        Self {
            save_key: DEFAULT_SAVE_KEY.to_string(),
        }
    }
}

/// Whether a save file currently exists for the configured save key.
///
/// Populated at startup and on entering the main menu by the game's
/// save plugin. Menus read it to enable or disable a "Continue" option.
#[derive(Resource, Default)]
pub struct SaveExists(pub bool);

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
}
