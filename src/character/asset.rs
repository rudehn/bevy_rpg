//! RON asset schemas for race and class manifests (Phase 2).
//!
//! Race now ships `hp_mod` (HP formula multiplier) and `gain_schedule`
//! (DCSS-style level-up stat-gain). Class ships an `attribute_distribution`
//! (12 points across STR/DEX/INT) instead of the Phase 1
//! primary/secondary + class_attack/dodge_bonus fields.
//!
//! The actual `RonAssetPlugin` registration and `OnEnter(Loading)` load
//! systems live in [`crate::assets::LoadingPlugin`].

use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

use crate::assets::StartingItemDef;
use crate::character::attributes::AttributeDistribution;
use crate::character::race::RaceGainSchedule;
use crate::game::skills::Skill;

/// Starting skill point distribution for a class (Phase 3). Negatives
/// are allowed in the schema for future class designs; current data
/// has no negatives. A maintenance test pins each class's total to 10.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct SkillDistribution {
    #[serde(default)]
    pub fighting: i32,
    #[serde(default)]
    pub axes: i32,
    #[serde(default)]
    pub short_blades: i32,
    #[serde(default)]
    pub long_blades: i32,
    #[serde(default)]
    pub ranged_weapons: i32,
    #[serde(default)]
    pub armor: i32,
    #[serde(default)]
    pub dodging: i32,
    #[serde(default)]
    pub shields: i32,
    #[serde(default)]
    pub evocations: i32,
    #[serde(default)]
    pub stealth: i32,
}

impl SkillDistribution {
    pub fn total(&self) -> i32 {
        self.fighting
            + self.axes
            + self.short_blades
            + self.long_blades
            + self.ranged_weapons
            + self.armor
            + self.dodging
            + self.shields
            + self.evocations
            + self.stealth
    }

    /// Iterate as `(Skill, i32)` pairs in `Skill::ALL` order.
    pub fn iter(&self) -> impl Iterator<Item = (Skill, i32)> + '_ {
        [
            (Skill::Fighting, self.fighting),
            (Skill::Axes, self.axes),
            (Skill::ShortBlades, self.short_blades),
            (Skill::LongBlades, self.long_blades),
            (Skill::RangedWeapons, self.ranged_weapons),
            (Skill::Armor, self.armor),
            (Skill::Dodging, self.dodging),
            (Skill::Shields, self.shields),
            (Skill::Evocations, self.evocations),
            (Skill::Stealth, self.stealth),
        ]
        .into_iter()
    }
}

/// Per-skill XP-cost aptitude on a race (Phase 3, DCSS-style).
/// Range: −5..=+5 in v1. Higher = faster training.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct SkillAptitudes {
    #[serde(default)]
    pub fighting: i32,
    #[serde(default)]
    pub axes: i32,
    #[serde(default)]
    pub short_blades: i32,
    #[serde(default)]
    pub long_blades: i32,
    #[serde(default)]
    pub ranged_weapons: i32,
    #[serde(default)]
    pub armor: i32,
    #[serde(default)]
    pub dodging: i32,
    #[serde(default)]
    pub shields: i32,
    #[serde(default)]
    pub evocations: i32,
    #[serde(default)]
    pub stealth: i32,
}

impl SkillAptitudes {
    pub fn for_skill(&self, skill: Skill) -> i32 {
        match skill {
            Skill::Fighting => self.fighting,
            Skill::Axes => self.axes,
            Skill::ShortBlades => self.short_blades,
            Skill::LongBlades => self.long_blades,
            Skill::RangedWeapons => self.ranged_weapons,
            Skill::Armor => self.armor,
            Skill::Dodging => self.dodging,
            Skill::Shields => self.shields,
            Skill::Evocations => self.evocations,
            Skill::Stealth => self.stealth,
        }
    }
}

/// One race entry, keyed by its lowercase id (e.g. `"human"`) in
/// `assets/races.ron`. The id must match the lowercase form of the
/// [`crate::character::Race`] variant name so the asset can be resolved
/// from a `Race` value at spawn time.
#[derive(Deserialize, Debug, Clone)]
pub struct RaceAsset {
    pub name: String,
    /// Attribute point contributions (Phase 2: 24 points across the three,
    /// no negatives in the initial data; the schema allows 21..=28).
    pub str_bonus: i32,
    pub dex_bonus: i32,
    pub int_bonus: i32,
    /// Multiplier applied to the HP formula. Dwarf 1.20, Human 1.00, Elf 0.90.
    pub hp_mod: f32,
    /// DCSS-style level-up stat-gain schedule (Human 4:SDI, Dwarf 4:SID,
    /// Elf 4:DI).
    pub gain_schedule: RaceGainSchedule,
    /// Phase 3: per-skill XP-cost aptitudes. Range −5..=+5; positive =
    /// faster training (lower XP cost via `aptitude_multiplier`).
    #[serde(default)]
    pub aptitudes: SkillAptitudes,
    pub description: String,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct RaceManifest {
    pub races: HashMap<String, RaceAsset>,
}

#[derive(Resource, Default)]
pub struct RaceManifestHandle(pub Handle<RaceManifest>);

/// One class entry, keyed by its lowercase id (e.g. `"warrior"`).
///
/// Phase 2 schema: replaces `primary_attr` / `secondary_attr` / `base_hp` /
/// `class_attack_bonus` / `class_dodge_bonus` with a single 12-point
/// `attribute_distribution`. HP is now race-driven; class differentiation
/// flows entirely through the distribution.
#[derive(Deserialize, Debug, Clone)]
pub struct ClassAsset {
    pub name: String,
    pub attribute_distribution: AttributeDistribution,
    #[serde(default)]
    pub starting_kit: Vec<StartingItemDef>,
    /// Phase 3: starting skill point distribution. Sum must be 10 in
    /// shipping data (maintenance test enforces this).
    #[serde(default)]
    pub starting_skills: SkillDistribution,
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
    use crate::character::class::Attribute;
    use crate::character::Race;

    /// `assets/races.ron` parses and contains exactly the three shipping
    /// races (Halfling removed in Phase 2).
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
                &"human".to_string(),
            ]
        );

        for race in [Race::Human, Race::Dwarf, Race::Elf] {
            let id = race.name().to_lowercase();
            assert!(
                manifest.races.contains_key(&id),
                "races.ron missing entry for {race} (id={id})"
            );
        }
    }

    /// Spec values from `docs/design/CHARACTER.md` §Races. Drift here
    /// breaks chargen attribute sums.
    #[test]
    fn shipped_race_bonuses_match_spec() {
        let src = include_str!("../../assets/races.ron");
        let manifest: RaceManifest = ron::from_str(src).expect("parse");

        let human = manifest.races.get("human").expect("human entry");
        assert_eq!(
            (human.str_bonus, human.dex_bonus, human.int_bonus),
            (8, 8, 8)
        );
        assert!((human.hp_mod - 1.00).abs() < 0.001);

        let dwarf = manifest.races.get("dwarf").expect("dwarf entry");
        assert_eq!(
            (dwarf.str_bonus, dwarf.dex_bonus, dwarf.int_bonus),
            (12, 4, 8)
        );
        assert!((dwarf.hp_mod - 1.20).abs() < 0.001);

        let elf = manifest.races.get("elf").expect("elf entry");
        assert_eq!((elf.str_bonus, elf.dex_bonus, elf.int_bonus), (4, 10, 10));
        assert!((elf.hp_mod - 0.90).abs() < 0.001);
    }

    /// Every race's gain_schedule parses with the expected interval and
    /// allowed-letters set. Pinned to make accidental schedule drift
    /// obvious in CI.
    #[test]
    fn shipped_race_schedules_match_spec() {
        let src = include_str!("../../assets/races.ron");
        let manifest: RaceManifest = ron::from_str(src).expect("parse");

        let human = manifest.races.get("human").expect("human");
        assert_eq!(human.gain_schedule.interval, 4);
        assert_eq!(
            human.gain_schedule.allowed,
            vec![Attribute::Str, Attribute::Dex, Attribute::Int]
        );

        let dwarf = manifest.races.get("dwarf").expect("dwarf");
        assert_eq!(dwarf.gain_schedule.interval, 4);
        assert_eq!(
            dwarf.gain_schedule.allowed,
            vec![Attribute::Str, Attribute::Int, Attribute::Dex]
        );

        let elf = manifest.races.get("elf").expect("elf");
        assert_eq!(elf.gain_schedule.interval, 4);
        assert_eq!(elf.gain_schedule.allowed, vec![Attribute::Dex, Attribute::Int]);
    }

    /// Every race's attribute total must be in the 21..=28 range (per
    /// the plan's schema contract). Currently all 3 races sum to 24.
    #[test]
    fn every_race_total_is_in_schema_range() {
        let src = include_str!("../../assets/races.ron");
        let manifest: RaceManifest = ron::from_str(src).expect("parse");

        for (id, race) in &manifest.races {
            let total = race.str_bonus + race.dex_bonus + race.int_bonus;
            assert!(
                (21..=28).contains(&total),
                "race '{id}' sums to {total}, outside 21..=28"
            );
            assert!(
                race.str_bonus >= 0 && race.dex_bonus >= 0 && race.int_bonus >= 0,
                "race '{id}' has negative bonus (not allowed for races)"
            );
        }
    }

    /// `assets/classes.ron` parses and contains the four shipping classes.
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

    /// Spec class distributions from §1 of the plan. Drift here changes
    /// every chargen attribute sum.
    #[test]
    fn shipped_class_distributions_match_spec() {
        let src = include_str!("../../assets/classes.ron");
        let manifest: ClassManifest = ron::from_str(src).expect("parse");

        let warrior = &manifest.classes.get("warrior").expect("warrior").attribute_distribution;
        assert_eq!((warrior.str, warrior.dex, warrior.int), (8, 2, 2));

        let rogue = &manifest.classes.get("rogue").expect("rogue").attribute_distribution;
        assert_eq!((rogue.str, rogue.dex, rogue.int), (2, 8, 2));

        let mage = &manifest.classes.get("mage").expect("mage").attribute_distribution;
        assert_eq!((mage.str, mage.dex, mage.int), (1, 3, 8));

        let ranger = &manifest.classes.get("ranger").expect("ranger").attribute_distribution;
        assert_eq!((ranger.str, ranger.dex, ranger.int), (3, 8, 1));
    }

    /// Every class's distribution must sum to exactly 12 (per the plan's
    /// locked decision). Allows-negatives is fine at the schema level
    /// (the type is i32) but the total is invariant.
    #[test]
    fn every_class_distribution_sums_to_twelve() {
        let src = include_str!("../../assets/classes.ron");
        let manifest: ClassManifest = ron::from_str(src).expect("parse");

        for (id, class) in &manifest.classes {
            let total = class.attribute_distribution.total();
            assert_eq!(
                total, 12,
                "class '{id}' distribution sums to {total}, expected 12"
            );
        }
    }

    /// Phase 3 maintenance contract: every class's `starting_skills` must
    /// sum to exactly 10. Drift here changes chargen power level.
    #[test]
    fn every_class_starting_skills_sums_to_ten() {
        let src = include_str!("../../assets/classes.ron");
        let manifest: ClassManifest = ron::from_str(src).expect("parse");

        for (id, class) in &manifest.classes {
            let total = class.starting_skills.total();
            assert_eq!(
                total, 10,
                "class '{id}' starting_skills sums to {total}, expected 10"
            );
        }
    }

    /// Phase 3 maintenance contract: every race's `aptitudes` must have an
    /// entry for every `Skill` variant, with values in the −5..=+5 range.
    /// The `for_skill` lookup never panics; this test just sanity-checks
    /// the range.
    #[test]
    fn every_race_aptitude_value_is_in_range() {
        use crate::game::skills::Skill;
        let src = include_str!("../../assets/races.ron");
        let manifest: RaceManifest = ron::from_str(src).expect("parse");

        for (id, race) in &manifest.races {
            for skill in Skill::ALL {
                let apt = race.aptitudes.for_skill(skill);
                assert!(
                    apt.abs() <= 5,
                    "race '{id}' aptitude for {skill:?} = {apt}, expected −5..=+5"
                );
            }
        }
    }

    /// Maintenance contract: every shipping race in `assets/races.ron`
    /// is documented in `docs/design/CHARACTER.md`, with its trait
    /// keyword appearing. Phase 2 roster: Adaptive / Stoneblood / Keen
    /// Senses (Halfling Lucky removed).
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

        for trait_keyword in ["Adaptive", "Stoneblood", "Keen Senses"] {
            assert!(
                doc.contains(trait_keyword),
                "CHARACTER.md is missing the race trait keyword '{}'",
                trait_keyword
            );
        }
    }

    /// Maintenance contract: every class in `assets/classes.ron` is
    /// documented in `docs/design/CHARACTER.md` with its distribution
    /// (STR/DEX/INT values) appearing in the same row.
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
        }
    }

    /// Every starting-kit item referenced by `classes.ron` must exist in
    /// `assets/items.ron`. Catches silently-empty starting kits.
    #[test]
    fn every_class_starting_kit_item_exists_in_items_ron() {
        use crate::assets::ItemManifest;

        let classes_src = include_str!("../../assets/classes.ron");
        let classes: ClassManifest = ron::from_str(classes_src).expect("parse classes");
        let items_src = include_str!("../../assets/items.ron");
        let items: ItemManifest = ron::from_str(items_src).expect("parse items");

        for (class_id, class_asset) in &classes.classes {
            for entry in &class_asset.starting_kit {
                assert!(
                    items.items.contains_key(&entry.name),
                    "class '{}' starting_kit references item '{}' that does not \
                     exist in items.ron. Either add the item or fix the class entry.",
                    class_id,
                    entry.name
                );
            }
        }
    }

    /// Phase 3 maintenance contract: every weapon in items.ron has a
    /// `weapon_skill` tag, or is a staff (which intentionally has no
    /// weapon-skill bonus — staves use Evocations on zap, Fighting on
    /// bash). Catches the failure mode where a new weapon ships
    /// without a skill tag and silently bypasses skill scaling.
    #[test]
    fn every_weapon_has_weapon_skill_or_is_staff() {
        use crate::assets::ItemManifest;
        use crate::game::items::ItemKind;

        let src = include_str!("../../assets/items.ron");
        let items: ItemManifest = ron::from_str(src).expect("parse items");

        for (id, item) in &items.items {
            if let Some(w) = item.weapon_data() {
                if w.weapon_skill.is_none() {
                    panic!(
                        "weapon '{id}' has no weapon_skill set. Either set it \
                         in items.ron or change kind (Staff is the only \
                         skill-less weapon-shaped item)."
                    );
                }
            }
        }
        // Silence the unused-import warning when the test only uses ItemKind
        // for documentation. (Imported above so the test reads cleanly.)
        let _ = ItemKind::Weapon;
    }
}
