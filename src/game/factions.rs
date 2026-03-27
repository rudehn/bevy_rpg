use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Relationship between two factions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Relation {
    Hostile,
    Neutral,
    Allied,
}

/// Data-driven hostility matrix loaded from `factions.ron`.
/// Lookup is symmetric — `(A, B)` and `(B, A)` yield the same result.
/// Unknown pairs default to `Hostile`. Same faction is always `Allied`.
#[derive(Resource, Debug, Default, Clone)]
pub struct FactionMatrix {
    relations: HashMap<(String, String), Relation>,
}

impl FactionMatrix {
    /// Build from a list of `(faction_a, faction_b, relation)` triples.
    pub fn from_entries(entries: &[(String, String, Relation)]) -> Self {
        let mut relations = HashMap::new();
        for (a, b, rel) in entries {
            relations.insert((a.clone(), b.clone()), *rel);
            relations.insert((b.clone(), a.clone()), *rel);
        }
        Self { relations }
    }

    pub fn is_hostile_to(&self, a: &str, b: &str) -> bool {
        if a == b { return false; }
        self.get(a, b) == Relation::Hostile
    }

    pub fn is_allied_to(&self, a: &str, b: &str) -> bool {
        if a == b { return true; }
        self.get(a, b) == Relation::Allied
    }

    pub fn is_neutral(&self, a: &str, b: &str) -> bool {
        if a == b { return false; }
        self.get(a, b) == Relation::Neutral
    }

    fn get(&self, a: &str, b: &str) -> Relation {
        self.relations
            .get(&(a.to_string(), b.to_string()))
            .copied()
            .unwrap_or(Relation::Hostile)
    }
}

/// RON asset type for `factions.ron`.
#[derive(Debug, Clone, Serialize, Deserialize, bevy::asset::Asset, bevy::reflect::TypePath)]
pub struct FactionMatrixAsset {
    pub relations: Vec<FactionRelationEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionRelationEntry {
    pub a: String,
    pub b: String,
    pub relation: Relation,
}

/// Handle to the loaded `factions.ron` asset.
#[derive(Resource, Default)]
pub struct FactionMatrixHandle(pub Handle<FactionMatrixAsset>);

pub struct FactionsPlugin;

impl Plugin for FactionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FactionMatrix>()
            .init_resource::<FactionMatrixHandle>()
            .add_systems(Update, apply_faction_matrix_asset);
    }
}

/// Once the faction RON asset is loaded, build the `FactionMatrix` resource from it.
fn apply_faction_matrix_asset(
    handle: Res<FactionMatrixHandle>,
    assets: Res<Assets<FactionMatrixAsset>>,
    mut matrix: ResMut<FactionMatrix>,
) {
    if !matrix.relations.is_empty() { return; } // Already loaded.

    let Some(asset) = assets.get(&handle.0) else { return; };
    let entries: Vec<_> = asset.relations.iter()
        .map(|e| (e.a.clone(), e.b.clone(), e.relation))
        .collect();
    *matrix = FactionMatrix::from_entries(&entries);
    info!("Faction matrix loaded with {} relations", entries.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_matrix() -> FactionMatrix {
        FactionMatrix::from_entries(&[
            ("Player".into(), "Monster".into(), Relation::Hostile),
            ("Player".into(), "Kobold".into(), Relation::Hostile),
            ("Monster".into(), "Kobold".into(), Relation::Neutral),
        ])
    }

    #[test]
    fn same_faction_always_allied() {
        let m = test_matrix();
        assert!(m.is_allied_to("Player", "Player"));
        assert!(m.is_allied_to("Monster", "Monster"));
        assert!(m.is_allied_to("Kobold", "Kobold"));
    }

    #[test]
    fn player_hostile_to_monsters() {
        let m = test_matrix();
        assert!(m.is_hostile_to("Player", "Monster"));
        assert!(m.is_hostile_to("Monster", "Player"));
    }

    #[test]
    fn player_hostile_to_kobolds() {
        let m = test_matrix();
        assert!(m.is_hostile_to("Player", "Kobold"));
        assert!(m.is_hostile_to("Kobold", "Player"));
    }

    #[test]
    fn monsters_neutral_to_kobolds() {
        let m = test_matrix();
        assert!(m.is_neutral("Monster", "Kobold"));
        assert!(m.is_neutral("Kobold", "Monster"));
        assert!(!m.is_hostile_to("Monster", "Kobold"));
        assert!(!m.is_allied_to("Monster", "Kobold"));
    }

    #[test]
    fn unknown_faction_defaults_to_hostile() {
        let m = test_matrix();
        assert!(m.is_hostile_to("Player", "Unknown"));
        assert!(m.is_hostile_to("Unknown", "Monster"));
    }

    #[test]
    fn symmetric_lookup() {
        let m = test_matrix();
        assert_eq!(m.get("Player", "Monster"), m.get("Monster", "Player"));
        assert_eq!(m.get("Monster", "Kobold"), m.get("Kobold", "Monster"));
    }
}
