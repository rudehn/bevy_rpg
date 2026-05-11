//! RON asset schemas for race and class manifests.
//!
//! The actual `RonAssetPlugin` registration and `OnEnter(Loading)` load
//! systems live in [`crate::assets::LoadingPlugin`] so the loading-state
//! handshake stays centralized.

use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

use crate::assets::StartingItemDef;
use crate::character::class::Attribute;

/// One race entry, keyed by its lowercase id (e.g. `"human"`) in
/// `assets/races.ron`. The id must match the lowercase form of the
/// [`crate::character::Race`] variant name so the asset can be resolved
/// from a `Race` value at spawn time.
#[derive(Deserialize, Debug, Clone)]
pub struct RaceAsset {
    pub name: String,
    pub str_bonus: i32,
    pub dex_bonus: i32,
    pub con_bonus: i32,
    pub int_bonus: i32,
    pub description: String,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct RaceManifest {
    pub races: HashMap<String, RaceAsset>,
}

#[derive(Resource, Default)]
pub struct RaceManifestHandle(pub Handle<RaceManifest>);

/// One class entry, keyed by its lowercase id (e.g. `"warrior"`).
#[derive(Deserialize, Debug, Clone)]
pub struct ClassAsset {
    pub name: String,
    pub primary_attr: Attribute,
    pub secondary_attr: Attribute,
    pub base_hp: i32,
    #[serde(default)]
    pub class_attack_bonus: i32,
    #[serde(default)]
    pub class_dodge_bonus: i32,
    #[serde(default)]
    pub starting_kit: Vec<StartingItemDef>,
    pub description: String,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct ClassManifest {
    pub classes: HashMap<String, ClassAsset>,
}

#[derive(Resource, Default)]
pub struct ClassManifestHandle(pub Handle<ClassManifest>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Race;

    /// `assets/races.ron` parses and contains exactly the four shipping races.
    #[test]
    fn shipped_races_manifest_parses() {
        let src = include_str!("../../assets/races.ron");
        let manifest: RaceManifest = ron::from_str(src).expect("races.ron must parse");

        let mut keys: Vec<&String> = manifest.races.keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                &"dwarf".to_string(),
                &"elf".to_string(),
                &"halfling".to_string(),
                &"human".to_string()
            ]
        );

        // Every Race enum variant has a matching asset id.
        for race in [Race::Human, Race::Dwarf, Race::Elf, Race::Halfling] {
            let id = race.name().to_lowercase();
            assert!(
                manifest.races.contains_key(&id),
                "races.ron missing entry for {race} (id={id})"
            );
        }
    }

    /// Spec values from `docs/design/CHARACTER.md` §Races. If this fails, the
    /// race table in the design doc and the shipped data have drifted.
    #[test]
    fn shipped_race_bonuses_match_spec() {
        let src = include_str!("../../assets/races.ron");
        let manifest: RaceManifest = ron::from_str(src).expect("parse");

        let human = manifest.races.get("human").expect("human entry");
        assert_eq!(
            (human.str_bonus, human.dex_bonus, human.con_bonus, human.int_bonus),
            (1, 1, 1, 1)
        );

        let dwarf = manifest.races.get("dwarf").expect("dwarf entry");
        assert_eq!(
            (dwarf.str_bonus, dwarf.dex_bonus, dwarf.con_bonus, dwarf.int_bonus),
            (2, 0, 2, 0)
        );

        let elf = manifest.races.get("elf").expect("elf entry");
        assert_eq!(
            (elf.str_bonus, elf.dex_bonus, elf.con_bonus, elf.int_bonus),
            (0, 2, 0, 2)
        );

        let halfling = manifest.races.get("halfling").expect("halfling entry");
        assert_eq!(
            (
                halfling.str_bonus,
                halfling.dex_bonus,
                halfling.con_bonus,
                halfling.int_bonus
            ),
            (0, 2, 1, 1)
        );
    }

    /// `assets/classes.ron` parses and contains exactly the four shipping classes.
    #[test]
    fn shipped_classes_manifest_parses() {
        let src = include_str!("../../assets/classes.ron");
        let manifest: ClassManifest = ron::from_str(src).expect("classes.ron must parse");

        let mut keys: Vec<&String> = manifest.classes.keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                &"mage".to_string(),
                &"ranger".to_string(),
                &"rogue".to_string(),
                &"warrior".to_string()
            ]
        );
    }

    /// Maintenance contract: every shipping race in `assets/races.ron` must
    /// be documented in `docs/design/CHARACTER.md`. The trait keyword for
    /// each race (Versatile, Stoneblood, Keen Senses, Lucky) must also
    /// appear. Failing this test means you added or renamed a race in the
    /// RON without updating the writeup. Fix the doc, not the test.
    #[test]
    fn character_md_documents_every_shipping_race() {
        let doc = include_str!("../../docs/design/CHARACTER.md");
        let src = include_str!("../../assets/races.ron");
        let manifest: RaceManifest = ron::from_str(src).expect("parse races");

        for (id, race_asset) in &manifest.races {
            assert!(
                doc.contains(&race_asset.name),
                "CHARACTER.md is missing the race name '{}' (id={})",
                race_asset.name,
                id
            );
        }

        // Each race's trait keyword must appear. Hardcoded list is fine —
        // adding a new RaceTrait variant requires updating this test, which
        // is the right place to also nudge updating the doc.
        for trait_keyword in ["Versatile", "Stoneblood", "Keen Senses", "Lucky"] {
            assert!(
                doc.contains(trait_keyword),
                "CHARACTER.md is missing the race trait keyword '{}'",
                trait_keyword
            );
        }
    }

    /// Maintenance contract: every class in `assets/classes.ron` must be
    /// documented in `docs/design/CHARACTER.md` with its name AND its
    /// `base_hp` value appearing in the same row. Drift here means
    /// players will see preview numbers that disagree with the writeup.
    #[test]
    fn character_md_documents_every_shipping_class() {
        let doc = include_str!("../../docs/design/CHARACTER.md");
        let src = include_str!("../../assets/classes.ron");
        let manifest: ClassManifest = ron::from_str(src).expect("parse classes");

        for (id, class_asset) in &manifest.classes {
            assert!(
                doc.contains(&class_asset.name),
                "CHARACTER.md is missing the class name '{}' (id={})",
                class_asset.name,
                id
            );
            // Look for "| ClassName |" followed (eventually) by the base_hp
            // value in the same markdown row. Single-line search by walking
            // the doc for the row that starts with the class name.
            let row = doc
                .lines()
                .find(|l| l.contains(&format!("| {} ", class_asset.name)))
                .unwrap_or_else(|| {
                    panic!(
                        "CHARACTER.md is missing the class-row line for '{}'",
                        class_asset.name
                    )
                });
            assert!(
                row.contains(&format!(" {} ", class_asset.base_hp)),
                "CHARACTER.md class row for '{}' is missing base_hp={} \
                 (row was: {:?})",
                class_asset.name,
                class_asset.base_hp,
                row
            );
        }
    }

    /// Spec values from `docs/design/CHARACTER.md` §Classes. Bases drifting here
    /// breaks every HP-derivation downstream.
    #[test]
    fn shipped_class_baselines_match_spec() {
        let src = include_str!("../../assets/classes.ron");
        let manifest: ClassManifest = ron::from_str(src).expect("parse");

        let warrior = manifest.classes.get("warrior").expect("warrior");
        assert_eq!(warrior.primary_attr, Attribute::Str);
        assert_eq!(warrior.secondary_attr, Attribute::Con);
        assert_eq!(warrior.base_hp, 12);
        assert_eq!(warrior.class_attack_bonus, 1);
        assert_eq!(warrior.class_dodge_bonus, 0);

        let rogue = manifest.classes.get("rogue").expect("rogue");
        assert_eq!(rogue.primary_attr, Attribute::Dex);
        assert_eq!(rogue.secondary_attr, Attribute::Int);
        assert_eq!(rogue.base_hp, 8);
        assert_eq!(rogue.class_attack_bonus, 0);
        assert_eq!(rogue.class_dodge_bonus, 1);

        let mage = manifest.classes.get("mage").expect("mage");
        assert_eq!(mage.primary_attr, Attribute::Int);
        assert_eq!(mage.secondary_attr, Attribute::Con);
        assert_eq!(mage.base_hp, 6);
        assert_eq!(mage.class_attack_bonus, 0);
        assert_eq!(mage.class_dodge_bonus, 0);

        let ranger = manifest.classes.get("ranger").expect("ranger");
        assert_eq!(ranger.primary_attr, Attribute::Dex);
        assert_eq!(ranger.secondary_attr, Attribute::Str);
        assert_eq!(ranger.base_hp, 8);
        assert_eq!(ranger.class_attack_bonus, 0);
        assert_eq!(ranger.class_dodge_bonus, 0);
    }
}
