//! Attribute component, character-creation payload, and the pure helpers that
//! compose a final `Attributes` from race + class and derive the HP value.
//!
//! Phase 2 changes from Phase 1:
//! - **CON removed.** HP no longer scales with a CON modifier; it derives
//!   from race + level via [`max_hp_for_level`].
//! - **Modifier anchor moved from 10 → 16.** Scores at chargen are
//!   typically below 16 (often negative mod), and players grow into
//!   competence across levels.
//! - **No free-point chargen allocation.** `CharacterChoice` carries only
//!   race and class; the attribute sum is fully race + class.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::character::asset::{ClassAsset, RaceAsset};
use crate::character::class::{Attribute, Class};
use crate::character::race::Race;

/// Ability modifier anchored at 16: `floor((score - 16) / 2)`.
///
/// - 16 → 0  (every -2 below 16 is one step lower)
/// - 14 → -1, 12 → -2, 10 → -3, 8 → -4
/// - 18 → +1, 20 → +2, 26 → +5, 28 → +6
///
/// Uses `div_euclid` so negative scores round toward negative infinity
/// (i.e. 6 → -5, not -4).
pub fn ability_mod(score: i32) -> i32 {
    (score - 16).div_euclid(2)
}

/// Final attribute scores carried on the player entity. Three attributes:
/// STR (melee hit/damage), DEX (ranged hit/damage, dodge), INT (staff
/// damage, future spellcasting). CON was removed in Phase 2 — HP scales
/// from race + level via the HP formula.
#[derive(
    Component, Reflect, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default,
)]
#[reflect(Component)]
pub struct Attributes {
    pub strength: i32,
    pub dexterity: i32,
    pub intelligence: i32,
}

impl Attributes {
    pub fn str_mod(&self) -> i32 {
        ability_mod(self.strength)
    }
    pub fn dex_mod(&self) -> i32 {
        ability_mod(self.dexterity)
    }
    pub fn int_mod(&self) -> i32 {
        ability_mod(self.intelligence)
    }

    pub fn mod_of(&self, attr: Attribute) -> i32 {
        match attr {
            Attribute::Str => self.str_mod(),
            Attribute::Dex => self.dex_mod(),
            Attribute::Int => self.int_mod(),
        }
    }

    pub fn set(&mut self, attr: Attribute, score: i32) {
        match attr {
            Attribute::Str => self.strength = score,
            Attribute::Dex => self.dexterity = score,
            Attribute::Int => self.intelligence = score,
        }
    }

    pub fn get(&self, attr: Attribute) -> i32 {
        match attr {
            Attribute::Str => self.strength,
            Attribute::Dex => self.dexterity,
            Attribute::Int => self.intelligence,
        }
    }

    /// In-place addition. Used by the racial schedule and player-choice
    /// ASI flows on level-up (Phase 2).
    pub fn add(&mut self, attr: Attribute, delta: i32) {
        self.set(attr, self.get(attr) + delta);
    }
}

/// A 12-point distribution declared on `ClassAsset`. Negatives are
/// allowed in the schema for future classes that want to penalize a
/// stat (none in the initial Phase 2 data). Sum is validated against
/// the class's declared `attribute_distribution.total()` by a
/// maintenance test.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttributeDistribution {
    pub str: i32,
    pub dex: i32,
    pub int: i32,
}

impl AttributeDistribution {
    pub fn total(&self) -> i32 {
        self.str + self.dex + self.int
    }
}

/// Payload from the character-creation screen to the player spawner.
/// Phase 2 simplification: just race + class. Attribute scores are
/// fully derived; there is no free-point allocation step.
///
/// Stored as a Bevy `Resource` so the spawner can read it at player-spawn
/// time. The character-creation UI overwrites the default before the run
/// starts; the save-load path also overwrites it from `PlayerSaveData`.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterChoice {
    pub race: Race,
    pub class: Class,
}

impl Default for CharacterChoice {
    fn default() -> Self {
        Self {
            race: Race::Human,
            class: Class::Warrior,
        }
    }
}

/// Compose final attribute scores from race + class. **Pure** — no
/// validation, no cap enforcement.
///
/// Final score per stat = `race.{stat}_bonus + class.attribute_distribution.{stat}`.
pub fn compose_attributes(race_asset: &RaceAsset, class_asset: &ClassAsset) -> Attributes {
    Attributes {
        strength: race_asset.str_bonus + class_asset.attribute_distribution.str,
        dexterity: race_asset.dex_bonus + class_asset.attribute_distribution.dex,
        intelligence: race_asset.int_bonus + class_asset.attribute_distribution.int,
    }
}

/// HP formula (Phase 2, DCSS-inspired, no Fighting term yet).
///
/// ```text
/// max_hp = floor(race_hp_mod × (8 + 11 × xp_level / 2))
/// ```
///
/// At XL 1 with the standard race multipliers the player starts at:
/// Dwarf (×1.20) 16 · Human (×1.00) 13 · Elf (×0.90) 12. By XL 27 the
/// same races land at 187 / 156 / 140. When the Skills phase ships, the
/// formula will gain a `Fighting`-scaled term.
pub fn max_hp_for_level(race_hp_mod: f32, xp_level: u32) -> i32 {
    // The 8 + 11*XL/2 grows roughly linearly with level. f32 keeps the
    // multiplier honest; we floor to i32 at the end so HP is integral.
    let base = 8.0 + (11.0 * xp_level as f32) / 2.0;
    (race_hp_mod * base).floor() as i32
}

/// Initial values to bake into the existing stat components at spawn.
/// HitBonus / DamageBonus get **0** at spawn — all attribute scaling
/// happens dynamically at hit-check and damage-roll time via
/// [`attack_attribute_bonus`]. Dodge gets DEX_mod baked in because dodge
/// is defender-side (attack-type-agnostic). MaxHp is derived from the
/// race × level formula.
///
/// Ranged/staff fields exist purely for the character-creation preview
/// UI; they don't correspond to a stored component.
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

/// Pick the attacker's attribute modifier that applies to a given
/// `DamageSource`. Used at hit-check and damage-roll time so attribute
/// scaling branches by weapon type.
///
/// - Melee: STR
/// - Ranged: DEX
/// - Spell / Environment / anything else: 0 (staff zaps add INT_mod
///   separately in `handle_zap_staff`; environment damage is
///   attribute-independent)
pub fn attack_attribute_bonus(
    source: roguelike_engine::combat::DamageSource,
    attrs: Option<&Attributes>,
) -> i32 {
    use roguelike_engine::combat::DamageSource;
    let Some(attrs) = attrs else {
        return 0;
    };
    match source {
        DamageSource::Melee => attrs.str_mod(),
        DamageSource::Ranged => attrs.dex_mod(),
        _ => 0,
    }
}

/// Compute the spawn-time / preview-time derived stats for a character at
/// the given experience level. The character creation UI uses this for
/// the live preview; the spawner uses `max_hp` and `dodge` directly and
/// leaves HitBonus/DamageBonus at 0 (the dynamic branch handles them).
pub fn derive_stats(race_asset: &RaceAsset, attrs: &Attributes, xp_level: u32) -> DerivedStats {
    let str_m = attrs.str_mod();
    let dex_m = attrs.dex_mod();
    let int_m = attrs.int_mod();
    DerivedStats {
        max_hp: max_hp_for_level(race_asset.hp_mod, xp_level),
        hit_bonus_melee: str_m,
        hit_bonus_ranged: dex_m,
        damage_bonus_melee: str_m,
        damage_bonus_ranged: dex_m,
        damage_bonus_staff: int_m,
        dodge: dex_m,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::race::RaceGainSchedule;

    /// New modifier anchor: 16 → 0, each -2 below is -1.
    #[test]
    fn ability_mod_anchored_at_16() {
        let cases = [
            (4, -6),
            (6, -5),
            (8, -4),
            (9, -4),
            (10, -3),
            (11, -3),
            (12, -2),
            (13, -2),
            (14, -1),
            (15, -1),
            (16, 0),
            (17, 0),
            (18, 1),
            (19, 1),
            (20, 2),
            (22, 3),
            (24, 4),
            (26, 5),
            (28, 6),
            (30, 7),
        ];
        for (score, expected) in cases {
            assert_eq!(
                ability_mod(score),
                expected,
                "ability_mod({score}) should be {expected}"
            );
        }
    }

    /// Negative scores aren't reachable through normal allocation but the
    /// floor-division behavior is worth pinning for future curse/drain
    /// effects.
    #[test]
    fn ability_mod_handles_negative_scores() {
        assert_eq!(ability_mod(0), -8);
        assert_eq!(ability_mod(-1), -9);
        assert_eq!(ability_mod(-2), -9);
        assert_eq!(ability_mod(-3), -10);
    }

    #[test]
    fn attributes_methods_delegate_to_ability_mod() {
        let a = Attributes {
            strength: 20,
            dexterity: 16,
            intelligence: 12,
        };
        assert_eq!(a.str_mod(), 2);
        assert_eq!(a.dex_mod(), 0);
        assert_eq!(a.int_mod(), -2);
        assert_eq!(a.mod_of(Attribute::Str), 2);
        assert_eq!(a.mod_of(Attribute::Int), -2);
    }

    #[test]
    fn attributes_get_set_add_round_trip() {
        let mut a = Attributes::default();
        a.set(Attribute::Str, 18);
        a.set(Attribute::Int, 14);
        assert_eq!(a.get(Attribute::Str), 18);
        assert_eq!(a.get(Attribute::Int), 14);
        assert_eq!(a.get(Attribute::Dex), 0);

        a.add(Attribute::Str, 1);
        a.add(Attribute::Int, -2);
        assert_eq!(a.get(Attribute::Str), 19);
        assert_eq!(a.get(Attribute::Int), 12);
    }

    fn test_race(s: i32, d: i32, i: i32, hp_mod: f32) -> RaceAsset {
        RaceAsset {
            name: "Test Race".to_string(),
            str_bonus: s,
            dex_bonus: d,
            int_bonus: i,
            hp_mod,
            gain_schedule: RaceGainSchedule {
                interval: 4,
                allowed: vec![Attribute::Str, Attribute::Dex, Attribute::Int],
            },
            aptitudes: Default::default(),
            description: String::new(),
        }
    }

    fn test_class(str: i32, dex: i32, int: i32) -> ClassAsset {
        ClassAsset {
            name: "Test Class".to_string(),
            attribute_distribution: AttributeDistribution { str, dex, int },
            starting_kit: Vec::new(),
            starting_skills: Default::default(),
            description: String::new(),
        }
    }

    /// Dwarf Warrior chargen — spec table from §4 of the plan.
    /// Race: +12 STR / +4 DEX / +8 INT. Class: +8 / +2 / +2.
    /// Final: STR 20 (+2), DEX 6 (-5), INT 10 (-3).
    #[test]
    fn compose_attributes_dwarf_warrior() {
        let race = test_race(12, 4, 8, 1.20);
        let class = test_class(8, 2, 2);
        let attrs = compose_attributes(&race, &class);
        assert_eq!(attrs.strength, 20);
        assert_eq!(attrs.dexterity, 6);
        assert_eq!(attrs.intelligence, 10);
        assert_eq!(attrs.str_mod(), 2);
        assert_eq!(attrs.dex_mod(), -5);
        assert_eq!(attrs.int_mod(), -3);
    }

    /// Elf Mage chargen — spec table.
    /// Race: +4 STR / +10 DEX / +10 INT. Class: +1 / +3 / +8.
    /// Final: STR 5 (-6), DEX 13 (-2), INT 18 (+1).
    #[test]
    fn compose_attributes_elf_mage() {
        let race = test_race(4, 10, 10, 0.90);
        let class = test_class(1, 3, 8);
        let attrs = compose_attributes(&race, &class);
        assert_eq!(attrs.strength, 5);
        assert_eq!(attrs.dexterity, 13);
        assert_eq!(attrs.intelligence, 18);
        assert_eq!(attrs.str_mod(), -6);
        assert_eq!(attrs.dex_mod(), -2);
        assert_eq!(attrs.int_mod(), 1);
    }

    /// HP formula: spec values at L1, L9, L18, L27 for the three race mods.
    #[test]
    fn hp_formula_matches_spec() {
        // Dwarf ×1.20
        assert_eq!(max_hp_for_level(1.20, 1), 16);
        assert_eq!(max_hp_for_level(1.20, 9), 69);
        assert_eq!(max_hp_for_level(1.20, 18), 128);
        assert_eq!(max_hp_for_level(1.20, 27), 187);
        // Human ×1.00
        assert_eq!(max_hp_for_level(1.00, 1), 13);
        assert_eq!(max_hp_for_level(1.00, 9), 57);
        assert_eq!(max_hp_for_level(1.00, 18), 107);
        assert_eq!(max_hp_for_level(1.00, 27), 156);
        // Elf ×0.90
        assert_eq!(max_hp_for_level(0.90, 1), 12);
        assert_eq!(max_hp_for_level(0.90, 9), 51);
        assert_eq!(max_hp_for_level(0.90, 18), 96);
        assert_eq!(max_hp_for_level(0.90, 27), 140);
    }

    #[test]
    fn derive_stats_has_no_class_fudge_factors() {
        // Class no longer carries class_attack_bonus / class_dodge_bonus —
        // all combat values come from stats. A Warrior with STR 20 gets +2
        // hit on melee; a Rogue with DEX 16 gets +0 hit on ranged.
        let race = test_race(0, 0, 0, 1.0);
        let class = test_class(0, 0, 0);
        let attrs = Attributes {
            strength: 20, // mod +2
            dexterity: 16, // mod 0
            intelligence: 12, // mod -2
        };
        let derived = derive_stats(&race, &attrs, 1);
        assert_eq!(derived.hit_bonus_melee, 2);
        assert_eq!(derived.hit_bonus_ranged, 0);
        assert_eq!(derived.damage_bonus_melee, 2);
        assert_eq!(derived.damage_bonus_ranged, 0);
        assert_eq!(derived.damage_bonus_staff, -2);
        assert_eq!(derived.dodge, 0); // pure DEX_mod
    }

    #[test]
    fn attack_attribute_bonus_picks_str_for_melee() {
        use roguelike_engine::combat::DamageSource;
        let attrs = Attributes {
            strength: 20,
            dexterity: 12,
            intelligence: 10,
        };
        assert_eq!(attack_attribute_bonus(DamageSource::Melee, Some(&attrs)), 2);
    }

    #[test]
    fn attack_attribute_bonus_picks_dex_for_ranged() {
        use roguelike_engine::combat::DamageSource;
        let attrs = Attributes {
            strength: 10,
            dexterity: 20,
            intelligence: 12,
        };
        assert_eq!(attack_attribute_bonus(DamageSource::Ranged, Some(&attrs)), 2);
    }

    #[test]
    fn attack_attribute_bonus_zero_for_spell_or_environment() {
        use roguelike_engine::combat::DamageSource;
        let attrs = Attributes {
            strength: 24,
            dexterity: 24,
            intelligence: 24,
        };
        assert_eq!(attack_attribute_bonus(DamageSource::Spell, Some(&attrs)), 0);
        assert_eq!(
            attack_attribute_bonus(DamageSource::Environment, Some(&attrs)),
            0
        );
    }

    #[test]
    fn attack_attribute_bonus_zero_for_entities_without_attributes() {
        use roguelike_engine::combat::DamageSource;
        assert_eq!(attack_attribute_bonus(DamageSource::Melee, None), 0);
        assert_eq!(attack_attribute_bonus(DamageSource::Ranged, None), 0);
    }

    #[test]
    fn character_choice_default_is_human_warrior() {
        let c = CharacterChoice::default();
        assert_eq!(c.race, Race::Human);
        assert_eq!(c.class, Class::Warrior);
    }

    #[test]
    fn attribute_distribution_total() {
        let warrior = AttributeDistribution { str: 8, dex: 2, int: 2 };
        let mage = AttributeDistribution { str: 1, dex: 3, int: 8 };
        assert_eq!(warrior.total(), 12);
        assert_eq!(mage.total(), 12);
    }
}
