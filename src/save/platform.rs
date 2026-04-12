//! Platform-agnostic save I/O primitives.
//!
//! These functions form the engine-owned boundary of the save/load system:
//! they read and write opaque string data keyed by a caller-supplied key, and
//! know nothing about the schema or the game's components.
//!
//! - **Native** (non-WASM): reads and writes `saves/<key>.ron`
//! - **WASM**: reads and writes the browser's `localStorage` under `<key>`
//!
//! Game-specific save/load code (schema, serialization, auto-save scheduling)
//! is built on top of these primitives.

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
fn save_path_for(key: &str) -> PathBuf {
    let dir = PathBuf::from("saves");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{}.ron", key))
}

/// Write serialized data for `key`. Returns `true` on success.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_bytes(key: &str, data: &str) -> bool {
    match std::fs::write(save_path_for(key), data) {
        Ok(()) => true,
        Err(e) => {
            bevy::prelude::error!("Failed to write save file: {}", e);
            false
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn write_bytes(key: &str, data: &str) -> bool {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        bevy::prelude::error!("localStorage unavailable");
        return false;
    };
    storage.set_item(key, data).is_ok()
}

/// Read serialized data for `key`, if any exists.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_bytes(key: &str) -> Option<String> {
    std::fs::read_to_string(save_path_for(key))
        .map_err(|e| bevy::prelude::warn!("Could not read save file: {}", e))
        .ok()
}

#[cfg(target_arch = "wasm32")]
pub fn read_bytes(key: &str) -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(key)
        .ok()?
}

/// Returns `true` if a save exists for `key`.
#[cfg(not(target_arch = "wasm32"))]
pub fn exists(key: &str) -> bool {
    save_path_for(key).exists()
}

#[cfg(target_arch = "wasm32")]
pub fn exists(key: &str) -> bool {
    read_bytes(key).is_some()
}

/// Delete the stored data for `key`.
pub fn delete(key: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = save_path_for(key);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                bevy::prelude::warn!("Failed to delete save file: {}", e);
            } else {
                bevy::prelude::info!("Save file deleted ({}).", key);
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = storage.remove_item(key);
            bevy::prelude::info!("Save deleted from localStorage ({}).", key);
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_key() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("_engine_platform_{}_{}", std::process::id(), n)
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn write_and_read_roundtrip() {
        let key = unique_test_key();
        assert!(write_bytes(&key, "hello engine"));
        assert_eq!(read_bytes(&key).as_deref(), Some("hello engine"));
        delete(&key);
        assert!(!exists(&key));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn keys_are_independent() {
        let a = unique_test_key();
        let b = unique_test_key();
        write_bytes(&a, "A");
        write_bytes(&b, "B");
        assert_eq!(read_bytes(&a).as_deref(), Some("A"));
        assert_eq!(read_bytes(&b).as_deref(), Some("B"));
        delete(&a);
        assert!(!exists(&a));
        assert!(exists(&b));
        delete(&b);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn exists_reflects_state_transitions() {
        let key = unique_test_key();
        assert!(!exists(&key));
        write_bytes(&key, "x");
        assert!(exists(&key));
        delete(&key);
        assert!(!exists(&key));
    }
}
