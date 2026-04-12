//! Faction identifier components.
//!
//! `Faction` is a Bevy component attached to entities that participate
//! in the hostility system. `FactionKind` is a newtype over `String` that
//! serves as the faction's stable identifier — it's compared against
//! other `FactionKind`s via the [`crate::factions::FactionMatrix`]
//! resource, not by case-sensitive string equality.
//!
//! The engine ships no specific faction names. Games define their own
//! roster (typically in a RON asset loaded via `FactionMatrix`) and
//! construct `FactionKind`s via [`FactionKind::new`] or the `From<&str>`
//! impl.

use bevy::ecs::component::Component;

/// Determines how this entity relates to others for AI targeting and
/// hostility checks. Hostility is resolved via the
/// [`crate::factions::FactionMatrix`] resource, not by comparing
/// `FactionKind`s directly.
#[derive(Component, Clone, PartialEq, Eq, Debug)]
pub struct Faction(pub FactionKind);

/// String-based faction identifier.
///
/// Hostility between factions is determined by the
/// [`crate::factions::FactionMatrix`] resource the game loads at
/// startup. `FactionKind` is intentionally a newtype over `String`
/// rather than an enum, so games can define an arbitrary roster
/// without modifying the engine.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct FactionKind(pub String);

impl FactionKind {
    /// Build a `FactionKind` from any string-like value.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Borrow the underlying faction name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for FactionKind {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for FactionKind {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for FactionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_str_and_string() {
        let a = FactionKind::new("Player");
        let b = FactionKind::new(String::from("Player"));
        assert_eq!(a, b);
    }

    #[test]
    fn as_str_returns_inner() {
        let k = FactionKind::new("Kobold");
        assert_eq!(k.as_str(), "Kobold");
    }

    #[test]
    fn from_str_builds_kind() {
        let k: FactionKind = "Monster".into();
        assert_eq!(k, FactionKind::new("Monster"));
    }

    #[test]
    fn display_prints_the_name() {
        let k = FactionKind::new("Rat");
        assert_eq!(format!("{}", k), "Rat");
    }

    #[test]
    fn equality_is_case_sensitive() {
        // The engine does not normalize case — games that want
        // case-insensitive matching should canonicalize before
        // constructing a FactionKind.
        assert_ne!(FactionKind::new("Player"), FactionKind::new("player"));
    }

    #[test]
    fn faction_wraps_kind() {
        let f = Faction(FactionKind::new("Player"));
        assert_eq!(f.0.as_str(), "Player");
    }
}
