//! Attribute component, character-creation payload, and the pure helpers that
//! compose a final `Attributes` from race + class + allocated points, then
//! derive the initial stat-component values to bake at spawn time.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::character::asset::{ClassAsset, RaceAsset};
use crate::character::class::{Attribute, Class};
use crate::character::race::Race;

/// D&D 5e ability modifier: `floor((score - 10) / 2)`.
///
/// Uses `div_euclid` so negative scores round toward negative infinity
/// (i.e. 8 → -1, not 0).
pub fn ability_mod(score: i32) -> i32 {
    (score - 10).div_euclid(2)
}

/// Final attribute scores carried on the player entity. Set once at spawn
/// and currently immutable until Phase 2 adds ASI (Attribute Score
/// Improvements at level-up).
#[derive(
    Component, Reflect, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
#[reflect(Component)]
pub struct Attributes {
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub intelligence: i32,
}

impl Attributes {
    pub fn str_mod(&self) -> i32 {
        ability_mod(self.strength)
    }
    pub fn dex_mod(&self) -> i32 {
        ability_mod(self.dexterity)
    }
    pub fn con_mod(&self) -> i32 {
        ability_mod(self.constitution)
    }
    pub fn int_mod(&self) -> i32 {
        ability_mod(self.intelligence)
    }

    /// Return the mod for a named attribute. Used when class metadata
    /// references its primary/secondary attribute by enum.
    pub fn mod_of(&self, attr: Attribute) -> i32 {
        match attr {
            Attribute::Str => self.str_mod(),
            Attribute::Dex => self.dex_mod(),
            Attribute::Con => self.con_mod(),
            Attribute::Int => self.int_mod(),
        }
    }

    /// Set the score for a named attribute. Mostly used by allocation logic
    /// in the character-creation UI.
    pub fn set(&mut self, attr: Attribute, score: i32) {
        match attr {
            Attribute::Str => self.strength = score,
            Attribute::Dex => self.dexterity = score,
            Attribute::Con => self.constitution = score,
            Attribute::Int => self.intelligence = score,
        }
    }

    pub fn get(&self, attr: Attribute) -> i32 {
        match attr {
            Attribute::Str => self.strength,
            Attribute::Dex => self.dexterity,
            Attribute::Con => self.constitution,
            Attribute::Int => self.intelligence,
        }
    }
}

/// Payload from the character-creation screen to the player spawner.
/// `free_points` is the player's allocation in (STR, DEX, CON, INT) order,
/// applied **on top of** the race + class baselines. The UI is responsible
/// for enforcing the per-stat cap and floor before constructing this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterChoice {
    pub race: Race,
    pub class: Class,
    pub free_points: [i32; 4],
}

impl Default for CharacterChoice {
    fn default() -> Self {
        // Default preselection per CHARACTER.md §Character Creation Flow:
        // Human Warrior, all 4 free points into STR.
        Self {
            race: Race::Human,
            class: Class::Warrior,
            free_points: [4, 0, 0, 0],
        }
    }
}

/// Compose final attributes from race baseline + class baseline + allocated
/// free points. **Pure** — does not enforce caps or floors (the UI does that).
///
/// Order of contributions:
///   1. Start at `[10, 10, 10, 10]`
///   2. Add race bonuses (`RaceAsset.*_bonus`)
///   3. Add class baseline (+2 to `primary_attr`, +1 to `secondary_attr`)
///   4. Add `free_points` (player allocation)
pub fn compose_attributes(
    race_asset: &RaceAsset,
    class_asset: &ClassAsset,
    free_points: [i32; 4],
) -> Attributes {
    let mut attrs = Attributes {
        strength: 10 + race_asset.str_bonus + free_points[0],
        dexterity: 10 + race_asset.dex_bonus + free_points[1],
        constitution: 10 + race_asset.con_bonus + free_points[2],
        intelligence: 10 + race_asset.int_bonus + free_points[3],
    };
    attrs.set(class_asset.primary_attr, attrs.get(class_asset.primary_attr) + 2);
    attrs.set(
        class_asset.secondary_attr,
        attrs.get(class_asset.secondary_attr) + 1,
    );
    attrs
}

/// Initial values for the existing stat components, derived from class +
/// attributes. The spawner bakes these directly into `HitBonus`, `Dodge`,
/// `DamageBonus`, and `Health.max` (alongside `class_attack_bonus` /
/// `class_dodge_bonus`). Equipment continues to bump those components
/// incrementally on top via the existing equip/unequip pipeline.
///
/// **HitBonus / DamageBonus are stored as a single value per entity**, so
/// the spawner currently uses the melee form (STR-driven). Ranged and
/// staff variants are exposed for future combat-math integration that
/// branches on weapon type (see §Combat Math Integration in CHARACTER.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DerivedStats {
    pub max_hp: i32,
    pub hit_bonus_melee: i32,
    pub hit_bonus_ranged: i32,
    pub damage_bonus_melee: i32,
    pub damage_bonus_ranged: i32,
    pub damage_bonus_staff: i32,
    pub dodge: i32,
}

pub fn derive_stats(class_asset: &ClassAsset, attrs: &Attributes) -> DerivedStats {
    let str_m = attrs.str_mod();
    let dex_m = attrs.dex_mod();
    let con_m = attrs.con_mod();
    let int_m = attrs.int_mod();
    DerivedStats {
        max_hp: class_asset.base_hp + con_m,
        hit_bonus_melee: str_m + class_asset.class_attack_bonus,
        hit_bonus_ranged: dex_m + class_asset.class_attack_bonus,
        damage_bonus_melee: str_m,
        damage_bonus_ranged: dex_m,
        damage_bonus_staff: int_m,
        dodge: dex_m + class_asset.class_dodge_bonus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Matches the D&D 5e modifier table exactly. If any of these values
    /// drifts, downstream combat math will quietly produce wrong results.
    #[test]
    fn ability_mod_matches_5e_table() {
        let cases = [
            (1, -5),
            (2, -4),
            (3, -4),
            (4, -3),
            (5, -3),
            (6, -2),
            (7, -2),
            (8, -1),
            (9, -1),
            (10, 0),
            (11, 0),
            (12, 1),
            (13, 1),
            (14, 2),
            (15, 2),
            (16, 3),
            (17, 3),
            (18, 4),
            (19, 4),
            (20, 5),
            (21, 5),
            (22, 6),
            (29, 9),
            (30, 10),
        ];
        for (score, expected) in cases {
            assert_eq!(
                ability_mod(score),
                expected,
                "ability_mod({score}) should be {expected}"
            );
        }
    }

    /// Negative attribute scores aren't reachable through normal allocation
    /// (floor is 8), but the floor-division behavior is worth pinning down
    /// in case some future effect (curse, vampiric drain) pushes a stat low.
    #[test]
    fn ability_mod_handles_negative_scores() {
        assert_eq!(ability_mod(0), -5);
        assert_eq!(ability_mod(-1), -6);
        assert_eq!(ability_mod(-2), -6);
        assert_eq!(ability_mod(-3), -7);
    }

    #[test]
    fn attributes_methods_delegate_to_ability_mod() {
        let a = Attributes {
            strength: 14,
            dexterity: 12,
            constitution: 16,
            intelligence: 8,
        };
        assert_eq!(a.str_mod(), 2);
        assert_eq!(a.dex_mod(), 1);
        assert_eq!(a.con_mod(), 3);
        assert_eq!(a.int_mod(), -1);
        assert_eq!(a.mod_of(Attribute::Str), 2);
        assert_eq!(a.mod_of(Attribute::Int), -1);
    }

    #[test]
    fn attributes_get_set_round_trip() {
        let mut a = Attributes::default();
        a.set(Attribute::Str, 15);
        a.set(Attribute::Int, 13);
        assert_eq!(a.get(Attribute::Str), 15);
        assert_eq!(a.get(Attribute::Int), 13);
        assert_eq!(a.get(Attribute::Dex), 0); // default()
    }

    fn test_race(s: i32, d: i32, c: i32, i: i32) -> RaceAsset {
        RaceAsset {
            name: "Test Race".to_string(),
            str_bonus: s,
            dex_bonus: d,
            con_bonus: c,
            int_bonus: i,
            description: String::new(),
        }
    }

    fn test_class(primary: Attribute, secondary: Attribute, base_hp: i32) -> ClassAsset {
        ClassAsset {
            name: "Test Class".to_string(),
            primary_attr: primary,
            secondary_attr: secondary,
            base_hp,
            class_attack_bonus: 0,
            class_dodge_bonus: 0,
            starting_kit: Vec::new(),
            description: String::new(),
        }
    }

    /// Race +1/+1/+1/+1, class Warrior (STR primary, CON secondary), no free
    /// points → STR 13, DEX 11, CON 12, INT 11. Spec values from
    /// CHARACTER.md §Classes "Concrete L1 examples" row "Dwarf Warrior".
    /// (We use Human-shaped race +1/+1/+1/+1 for round-number arithmetic.)
    #[test]
    fn compose_attributes_human_warrior_no_points() {
        let race = test_race(1, 1, 1, 1);
        let class = test_class(Attribute::Str, Attribute::Con, 12);
        let attrs = compose_attributes(&race, &class, [0, 0, 0, 0]);
        // STR: 10 + 1 (race) + 2 (class primary) = 13
        // DEX: 10 + 1 = 11
        // CON: 10 + 1 + 1 (class secondary) = 12
        // INT: 10 + 1 = 11
        assert_eq!(attrs.strength, 13);
        assert_eq!(attrs.dexterity, 11);
        assert_eq!(attrs.constitution, 12);
        assert_eq!(attrs.intelligence, 11);
    }

    /// Dwarf (+2 STR, +2 CON) Warrior, 4 points into CON. Spec row:
    /// CON 17 → CON_mod +3 → HP 15.
    #[test]
    fn compose_attributes_dwarf_warrior_all_con() {
        let race = test_race(2, 0, 2, 0);
        let class = test_class(Attribute::Str, Attribute::Con, 12);
        let attrs = compose_attributes(&race, &class, [0, 0, 4, 0]);
        // STR: 10 + 2 + 2 = 14
        // DEX: 10
        // CON: 10 + 2 + 1 (sec) + 4 (alloc) = 17
        // INT: 10
        assert_eq!(attrs.strength, 14);
        assert_eq!(attrs.dexterity, 10);
        assert_eq!(attrs.constitution, 17);
        assert_eq!(attrs.intelligence, 10);

        let derived = derive_stats(&class, &attrs);
        // class_base 12 + CON_mod(17) = 12 + 3 = 15
        assert_eq!(derived.max_hp, 15);
    }

    /// Elf (+0 STR, +2 DEX, +0 CON, +2 INT) Mage (INT primary, CON secondary),
    /// no allocated points. Pins the HP-from-CON formula end-to-end.
    #[test]
    fn compose_and_derive_elf_mage_baseline() {
        let race = test_race(0, 2, 0, 2);
        let class = test_class(Attribute::Int, Attribute::Con, 6);
        let attrs = compose_attributes(&race, &class, [0, 0, 0, 0]);
        // CON: 10 + 0 (race) + 1 (class secondary) = 11 → mod 0
        // INT: 10 + 2 (race) + 2 (class primary)   = 14 → mod +2
        assert_eq!(attrs.constitution, 11);
        assert_eq!(attrs.intelligence, 14);

        let derived = derive_stats(&class, &attrs);
        // class_base 6 + CON_mod(0) = 6 HP
        assert_eq!(derived.max_hp, 6);
        // INT_mod +2 → staff damage bonus +2
        assert_eq!(derived.damage_bonus_staff, 2);
    }

    /// `class_attack_bonus` and `class_dodge_bonus` flow into the derived
    /// stats additively. Warrior has +1 attack, Rogue has +1 dodge — pin
    /// the math here so neither drifts silently.
    #[test]
    fn derive_stats_includes_class_attack_and_dodge_constants() {
        let mut warrior = test_class(Attribute::Str, Attribute::Con, 12);
        warrior.class_attack_bonus = 1;
        let attrs = Attributes {
            strength: 14, // mod +2
            dexterity: 10,
            constitution: 10,
            intelligence: 10,
        };
        let derived = derive_stats(&warrior, &attrs);
        // hit_bonus_melee = STR_mod (+2) + class_attack_bonus (+1) = +3
        assert_eq!(derived.hit_bonus_melee, 3);
        // hit_bonus_ranged = DEX_mod (0) + class_attack_bonus (+1) = +1
        assert_eq!(derived.hit_bonus_ranged, 1);
        // dodge = DEX_mod (0) + class_dodge_bonus (0) = 0
        assert_eq!(derived.dodge, 0);

        let mut rogue = test_class(Attribute::Dex, Attribute::Int, 8);
        rogue.class_dodge_bonus = 1;
        let attrs = Attributes {
            strength: 10,
            dexterity: 14, // mod +2
            constitution: 10,
            intelligence: 10,
        };
        let derived = derive_stats(&rogue, &attrs);
        // dodge = DEX_mod (+2) + class_dodge_bonus (+1) = +3
        assert_eq!(derived.dodge, 3);
        // hit_bonus_ranged = DEX_mod (+2) + 0 = +2
        assert_eq!(derived.hit_bonus_ranged, 2);
    }

    /// Default `CharacterChoice` is Human Warrior, 4 points into STR, per
    /// CHARACTER.md §Character Creation Flow. Locked in by test to make
    /// any silent change to the default visible.
    #[test]
    fn character_choice_default_is_human_warrior_all_str() {
        let c = CharacterChoice::default();
        assert_eq!(c.race, Race::Human);
        assert_eq!(c.class, Class::Warrior);
        assert_eq!(c.free_points, [4, 0, 0, 0]);
    }
}
