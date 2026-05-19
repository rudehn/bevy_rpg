//! Faction hostility matrix — game-side re-export plus the
//! `Option<&Faction>`-aware helpers that the rest of the game uses.
//!
//! The full implementation lives in `roguelike_engine::factions`. Veiled
//! Tyrant's specific faction roster is defined in `assets/factions.ron`;
//! the engine ships only the type machinery and the `FactionMatrix`
//! methods that take borrowed `&Faction` or `&str`.
//!
//! What this module adds on top:
//!
//! 1. The `Option<&Faction>` overloads. ECS queries hand back
//!    `Option<&Faction>` (an entity might lack the component), and the
//!    game needs a single canonical answer for what "no faction" means
//!    in a hostility check. The two policies are:
//!
//!    - **Neutral default** ([`factions_hostile`], [`factions_allied`]):
//!      missing faction on either side ⇒ `false`. Used by combat,
//!      fleeing, and any "X vs. Y" check where the absence of faction
//!      data should mean "no relationship."
//!    - **Hostile-to-player default** ([`faction_hostile_to_player`]):
//!      missing faction ⇒ `true`. Used by the monster awareness/mode
//!      gate so unfactioned monsters still wake up and hunt the player
//!      (matches legacy behavior — see [`FACTIONS.md`](../../../docs/design/FACTIONS.md)
//!      §"default-Hostile gotcha").
//!
//! 2. Re-exports of the engine's `FactionMatrix` and friends so existing
//!    `crate::game::factions::*` imports keep working.

use roguelike_engine::components::Faction;

pub use roguelike_engine::factions::{
    apply_faction_matrix_asset, FactionMatrix, FactionMatrixAsset, FactionMatrixHandle,
    FactionRelationEntry, FactionsPlugin, Relation,
};

/// Are two entities' factions hostile to each other?
///
/// `None` on either side resolves to `false` — faction-less entities
/// are treated as inert in pairwise hostility checks. For the
/// player-targeting gate (which defaults `None` to *hostile*) see
/// [`faction_hostile_to_player`].
pub fn factions_hostile(
    a: Option<&Faction>,
    b: Option<&Faction>,
    matrix: &FactionMatrix,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => matrix.are_hostile(a, b),
        _ => false,
    }
}

/// Are two entities' factions allied?
///
/// `None` on either side resolves to `false`.
pub fn factions_allied(
    a: Option<&Faction>,
    b: Option<&Faction>,
    matrix: &FactionMatrix,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => matrix.are_allied(a, b),
        _ => false,
    }
}

/// Is `faction` hostile to the Player faction?
///
/// `None` (no faction component) resolves to `true` — matches the
/// legacy "unfactioned monsters always hunt the player" policy used by
/// the awareness / mode-transition gate so newly-spawned monsters react
/// on first sight even before their loadout is processed.
pub fn faction_hostile_to_player(
    faction: Option<&Faction>,
    matrix: &FactionMatrix,
) -> bool {
    match faction {
        Some(f) => matrix.is_hostile_to(f.0.as_str(), "Player"),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roguelike_engine::components::FactionKind;

    fn matrix() -> FactionMatrix {
        FactionMatrix::from_entries(&[
            ("Player".to_string(), "Monster".to_string(), Relation::Hostile),
            ("Player".to_string(), "Townsfolk".to_string(), Relation::Allied),
        ])
    }

    fn faction(name: &str) -> Faction {
        Faction(FactionKind::new(name))
    }

    #[test]
    fn factions_hostile_both_present_consults_matrix() {
        let m = matrix();
        let p = faction("Player");
        let mon = faction("Monster");
        assert!(factions_hostile(Some(&p), Some(&mon), &m));
        assert!(factions_hostile(Some(&mon), Some(&p), &m));
        // Same faction is never hostile.
        assert!(!factions_hostile(Some(&p), Some(&p), &m));
    }

    #[test]
    fn factions_hostile_missing_side_defaults_neutral() {
        let m = matrix();
        let mon = faction("Monster");
        assert!(!factions_hostile(None, Some(&mon), &m));
        assert!(!factions_hostile(Some(&mon), None, &m));
        assert!(!factions_hostile(None, None, &m));
    }

    #[test]
    fn factions_allied_both_present_consults_matrix() {
        let m = matrix();
        let p = faction("Player");
        let town = faction("Townsfolk");
        let mon = faction("Monster");
        assert!(factions_allied(Some(&p), Some(&town), &m));
        // Cross-faction hostiles are not allied.
        assert!(!factions_allied(Some(&p), Some(&mon), &m));
    }

    #[test]
    fn factions_allied_missing_side_defaults_neutral() {
        let m = matrix();
        let p = faction("Player");
        assert!(!factions_allied(None, Some(&p), &m));
        assert!(!factions_allied(Some(&p), None, &m));
        assert!(!factions_allied(None, None, &m));
    }

    #[test]
    fn faction_hostile_to_player_consults_matrix_when_present() {
        let m = matrix();
        let mon = faction("Monster");
        let town = faction("Townsfolk");
        assert!(faction_hostile_to_player(Some(&mon), &m));
        assert!(!faction_hostile_to_player(Some(&town), &m));
    }

    #[test]
    fn faction_hostile_to_player_defaults_true_when_missing() {
        // Unfactioned monsters fall back to "hunt the player" — this
        // is the asymmetric default the awareness gate relies on.
        let m = matrix();
        assert!(faction_hostile_to_player(None, &m));
    }
}
