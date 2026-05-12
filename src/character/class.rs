//! Player class: marker component and the three-attribute enum it indexes into.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// One of four player classes chosen during character creation.
///
/// Class allocates a 12-point distribution across STR/DEX/INT (see
/// `ClassAsset::attribute_distribution`) and ships a deliberately weak
/// starting kit. All class combat differentiation flows through the
/// attribute distribution — there is no flat `class_attack_bonus` or
/// `class_dodge_bonus`. See `docs/design/CHARACTER.md` §Classes.
#[derive(
    Component,
    Reflect,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
)]
#[reflect(Component)]
pub enum Class {
    #[default]
    Warrior,
    Rogue,
    Mage,
    Ranger,
}

impl Class {
    pub const fn name(self) -> &'static str {
        match self {
            Class::Warrior => "Warrior",
            Class::Rogue => "Rogue",
            Class::Mage => "Mage",
            Class::Ranger => "Ranger",
        }
    }
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The three player attributes (Phase 2 — CON removed; HP now derives
/// from race + level via the HP formula in `attributes.rs`).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attribute {
    Str,
    Dex,
    Int,
}

impl Attribute {
    pub const fn name(self) -> &'static str {
        match self {
            Attribute::Str => "STR",
            Attribute::Dex => "DEX",
            Attribute::Int => "INT",
        }
    }

    /// Single-letter shorthand used by the racial gain-schedule notation
    /// (`4:SDI` means "every 4 levels, choose S, D, or I").
    pub const fn letter(self) -> char {
        match self {
            Attribute::Str => 'S',
            Attribute::Dex => 'D',
            Attribute::Int => 'I',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_deserializes_from_unit_variant_name() {
        let cases = [
            ("Warrior", Class::Warrior),
            ("Rogue", Class::Rogue),
            ("Mage", Class::Mage),
            ("Ranger", Class::Ranger),
        ];
        for (input, expected) in cases {
            let parsed: Class = ron::from_str(input).expect("parse class");
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn attribute_deserializes_from_unit_variant_name() {
        let cases = [
            ("Str", Attribute::Str),
            ("Dex", Attribute::Dex),
            ("Int", Attribute::Int),
        ];
        for (input, expected) in cases {
            let parsed: Attribute = ron::from_str(input).expect("parse attr");
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn attribute_letter_matches_first_char_of_name() {
        assert_eq!(Attribute::Str.letter(), 'S');
        assert_eq!(Attribute::Dex.letter(), 'D');
        assert_eq!(Attribute::Int.letter(), 'I');
    }
}
