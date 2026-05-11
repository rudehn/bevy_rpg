//! Player class: marker component and the four-attribute enum it indexes into.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// One of four player classes chosen during character creation.
///
/// Class sets primary/secondary attribute focus, base HP, small attack/dodge
/// constants, and a deliberately-weak starting kit (full data in
/// `assets/classes.ron`). See `docs/design/CHARACTER.md` §Classes.
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

/// The four player attributes. Used by `ClassAsset` to declare
/// primary/secondary focus and by future systems for save typing.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attribute {
    Str,
    Dex,
    Con,
    Int,
}

impl Attribute {
    pub const fn name(self) -> &'static str {
        match self {
            Attribute::Str => "STR",
            Attribute::Dex => "DEX",
            Attribute::Con => "CON",
            Attribute::Int => "INT",
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
            ("Con", Attribute::Con),
            ("Int", Attribute::Int),
        ];
        for (input, expected) in cases {
            let parsed: Attribute = ron::from_str(input).expect("parse attr");
            assert_eq!(parsed, expected);
        }
    }
}
