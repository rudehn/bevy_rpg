use bevy::prelude::*;
use serde::Deserialize;

use crate::components::{FloorEntityMarker, InInventory, Inventory, Item, Name, Position};
use crate::game::AppState;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

// --- Enums ---

#[derive(Component, Debug, Clone, PartialEq, Eq, Reflect, Default, Deserialize)]
pub enum ItemKind {
    #[default]
    Consumable,
    Weapon,
    Armor,
    Ring,
    Amulet,
    Spellbook,
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ItemKind::Consumable => "Consumable",
            ItemKind::Weapon => "Weapon",
            ItemKind::Armor => "Armor",
            ItemKind::Ring => "Ring",
            ItemKind::Amulet => "Amulet",
            ItemKind::Spellbook => "Spellbook",
        };
        write!(f, "{}", s)
    }
}

#[derive(Component, Debug, Clone, PartialEq, Eq, Reflect, Default, Deserialize)]
pub enum ArmorSlot {
    #[default]
    Chest,
    Helm,
    Gloves,
    Boots,
    OffHand,
}

impl std::fmt::Display for ArmorSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ArmorSlot::Chest => "Chest",
            ArmorSlot::Helm => "Helm",
            ArmorSlot::Gloves => "Gloves",
            ArmorSlot::Boots => "Boots",
            ArmorSlot::OffHand => "Off-Hand",
        };
        write!(f, "{}", s)
    }
}

#[derive(Component, Debug, Clone, PartialEq, Eq, Reflect, Default, Deserialize)]
pub enum Rarity {
    #[default]
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl Rarity {
    pub fn color(&self) -> Color {
        match self {
            Rarity::Common => Color::WHITE,
            Rarity::Uncommon => Color::srgb(0.0, 0.8, 0.0),
            Rarity::Rare => Color::srgb(0.2, 0.4, 1.0),
            Rarity::Legendary => Color::srgb(1.0, 0.5, 0.0),
        }
    }
}

impl std::fmt::Display for Rarity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Rarity::Common => "Common",
            Rarity::Uncommon => "Uncommon",
            Rarity::Rare => "Rare",
            Rarity::Legendary => "Legendary",
        };
        write!(f, "{}", s)
    }
}

// --- Components ---

/// All stat and mechanical properties of an item.
/// Equipment effects are applied via AttributeModifiers when equipped (M3).
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct ItemProperties {
    pub kind: ItemKind,
    pub armor_slot: Option<ArmorSlot>,
    /// Damage dice string for weapons (e.g. "1d8+1").
    pub damage: Option<String>,
    /// Flat armor value for armor pieces.
    pub defense: i32,
    pub rarity: Rarity,
    // Stat bonuses applied when equipped (wired to AttributeModifiers in M3)
    pub str_bonus: i32,
    pub dex_bonus: i32,
    pub con_bonus: i32,
    pub agi_bonus: i32,
    pub int_bonus: i32,
    pub per_bonus: i32,
}

impl ItemProperties {
    pub fn bonus_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.str_bonus != 0 { parts.push(format!("STR {:+}", self.str_bonus)); }
        if self.dex_bonus != 0 { parts.push(format!("DEX {:+}", self.dex_bonus)); }
        if self.con_bonus != 0 { parts.push(format!("CON {:+}", self.con_bonus)); }
        if self.agi_bonus != 0 { parts.push(format!("AGI {:+}", self.agi_bonus)); }
        if self.int_bonus != 0 { parts.push(format!("INT {:+}", self.int_bonus)); }
        if self.per_bonus != 0 { parts.push(format!("PER {:+}", self.per_bonus)); }
        parts.join("  ")
    }
}

// --- Resources ---

/// The currently selected slot index in the inventory UI (0-based).
#[derive(Resource, Default)]
pub struct SelectedInventorySlot(pub usize);

// --- Messages ---

/// Sent when the player drops a specific item from inventory.
#[derive(Message, Debug)]
pub struct DropItemMessage {
    pub item_entity: Entity,
}

// --- Systems ---

/// Removes an item from the player's inventory and places it at the player's position.
pub fn handle_drop_item(
    mut commands: Commands,
    mut messages: MessageReader<DropItemMessage>,
    mut inv_query: Query<(&mut Inventory, &Position), With<Player>>,
    item_query: Query<&Name, With<Item>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    let Ok((mut inv, player_pos)) = inv_query.single_mut() else {
        return;
    };

    for msg in messages.read() {
        let Some(slot) = inv.items.iter().position(|&e| e == msg.item_entity) else {
            continue;
        };
        inv.items.remove(slot);

        let item_name = item_query
            .get(msg.item_entity)
            .map(|n| n.0.as_str())
            .unwrap_or("item");
        log_writer.write(GameLogMessage(format!("You drop the {}.", item_name)));

        commands
            .entity(msg.item_entity)
            .insert(Position { x: player_pos.x, y: player_pos.y })
            .insert(Visibility::Inherited)
            .insert(FloorEntityMarker)
            .remove::<InInventory>();
    }
}

// --- Plugin ---

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ItemKind>()
            .register_type::<ArmorSlot>()
            .register_type::<Rarity>()
            .register_type::<ItemProperties>()
            .init_resource::<SelectedInventorySlot>()
            .add_message::<DropItemMessage>()
            .add_systems(
                Update,
                handle_drop_item.run_if(in_state(AppState::InGame)),
            );
    }
}
