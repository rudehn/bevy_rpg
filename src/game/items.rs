use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{Equipped, FloorEntityMarker, GameEntityMarker, InInventory, Inventory, Item, Name, Position};
use crate::constants::{BASE_ACTION_COST, UNARMED_DAMAGE, Z_ITEM};
use crate::game::actions::ActionFinishedEvent;
use crate::game::effects::Effect;
use crate::game::stats::Armor;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

// --- Enums ---

#[derive(Component, Debug, Clone, PartialEq, Eq, Reflect, Default, Serialize, Deserialize)]
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

#[derive(Component, Debug, Clone, PartialEq, Eq, Reflect, Default, Serialize, Deserialize)]
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

#[derive(Component, Debug, Clone, PartialEq, Eq, Reflect, Default, Serialize, Deserialize)]
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

/// Tracks how many of this item are in this stack slot.
/// max_stack == 1 means the item is not stackable.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct ItemStack {
    pub count: u32,
    pub max_stack: u32,
}

impl Default for ItemStack {
    fn default() -> Self {
        Self { count: 1, max_stack: 1 }
    }
}

/// All stat and mechanical properties of an item.
/// Equipment effects are applied via AttributeModifiers when equipped (M3).
#[derive(Component, Debug, Clone, Reflect, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct ItemProperties {
    pub kind: ItemKind,
    pub armor_slot: Option<ArmorSlot>,
    /// Damage dice string for weapons (e.g. "1d8+1").
    pub damage: Option<String>,
    /// Flat armor value for armor pieces.
    pub defense: i32,
    pub rarity: Rarity,
    /// One-shot effect applied when the item is consumed (Consumables only).
    pub effect: Option<Effect>,
    /// Range for ranged weapons (> 1 = ranged; 0 or 1 = melee).
    pub weapon_range: u32,
}

// --- Equipment Component ---

/// Tracks which item entity occupies each equipment slot on an actor.
/// Equipped items remain in Inventory.items; the Equipped marker is added to the item entity.
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub struct Equipment {
    pub weapon:  Option<Entity>,
    pub offhand: Option<Entity>,
    pub helm:    Option<Entity>,
    pub chest:   Option<Entity>,
    pub gloves:  Option<Entity>,
    pub boots:   Option<Entity>,
    pub ring_l:  Option<Entity>,
    pub ring_r:  Option<Entity>,
    pub amulet:  Option<Entity>,
}

impl Equipment {
    /// Determine the primary slot name for an item's properties. Returns `None` for
    /// non-equippable items (Consumable, Spellbook) or Armor missing an armor_slot.
    pub fn slot_for(props: &ItemProperties) -> Option<&'static str> {
        match &props.kind {
            ItemKind::Weapon    => Some("weapon"),
            ItemKind::Amulet    => Some("amulet"),
            ItemKind::Ring      => Some("ring_l"), // caller handles ring_l/ring_r logic
            ItemKind::Armor     => props.armor_slot.as_ref().map(|s| match s {
                ArmorSlot::Chest   => "chest",
                ArmorSlot::Helm    => "helm",
                ArmorSlot::Gloves  => "gloves",
                ArmorSlot::Boots   => "boots",
                ArmorSlot::OffHand => "offhand",
            }),
            _ => None,
        }
    }

    pub fn get_entity(&self, slot: &str) -> Option<Entity> {
        match slot {
            "weapon"  => self.weapon,
            "offhand" => self.offhand,
            "helm"    => self.helm,
            "chest"   => self.chest,
            "gloves"  => self.gloves,
            "boots"   => self.boots,
            "ring_l"  => self.ring_l,
            "ring_r"  => self.ring_r,
            "amulet"  => self.amulet,
            _         => None,
        }
    }

    pub fn set_slot(&mut self, slot: &str, entity: Option<Entity>) {
        match slot {
            "weapon"  => self.weapon  = entity,
            "offhand" => self.offhand = entity,
            "helm"    => self.helm    = entity,
            "chest"   => self.chest   = entity,
            "gloves"  => self.gloves  = entity,
            "boots"   => self.boots   = entity,
            "ring_l"  => self.ring_l  = entity,
            "ring_r"  => self.ring_r  = entity,
            "amulet"  => self.amulet  = entity,
            _         => {}
        }
    }

    /// Returns the slot name currently holding `entity`, if any.
    pub fn find_slot(&self, entity: Entity) -> Option<&'static str> {
        if self.weapon  == Some(entity) { return Some("weapon");  }
        if self.offhand == Some(entity) { return Some("offhand"); }
        if self.helm    == Some(entity) { return Some("helm");    }
        if self.chest   == Some(entity) { return Some("chest");   }
        if self.gloves  == Some(entity) { return Some("gloves");  }
        if self.boots   == Some(entity) { return Some("boots");   }
        if self.ring_l  == Some(entity) { return Some("ring_l");  }
        if self.ring_r  == Some(entity) { return Some("ring_r");  }
        if self.amulet  == Some(entity) { return Some("amulet");  }
        None
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

/// Sent when the player wants to equip an item from inventory.
#[derive(Message, Debug)]
pub struct EquipItemMessage {
    pub item_entity: Entity,
}

/// Sent when the player wants to unequip an item (returns it to inventory).
#[derive(Message, Debug)]
pub struct UnequipItemMessage {
    pub item_entity: Entity,
}

// --- Systems ---

/// Helper: reverses the armor/damage effects of an equipped item.
fn unapply_item_effects(
    props: &ItemProperties,
    armor: &mut Armor,
    damage: &mut crate::game::combat::Damage,
) {
    armor.0 -= props.defense;
    if props.kind == ItemKind::Weapon {
        damage.0 = UNARMED_DAMAGE.to_string();
    }
}

/// Helper: applies the armor/damage effects of an equipped item.
fn apply_item_effects(
    props: &ItemProperties,
    armor: &mut Armor,
    damage: &mut crate::game::combat::Damage,
) {
    armor.0 += props.defense;
    if props.kind == ItemKind::Weapon {
        if let Some(dmg) = &props.damage {
            damage.0 = dmg.clone();
        }
    }
}

/// Equips an item from the player's inventory into the appropriate slot.
/// If the slot is already occupied the old item is unequipped first (stays in inventory).
pub fn handle_equip_item(
    mut commands: Commands,
    mut messages: MessageReader<EquipItemMessage>,
    mut player_query: Query<
        (Entity, &mut Equipment, &Inventory, &mut Armor, &mut crate::game::combat::Damage),
        With<Player>,
    >,
    item_query: Query<(&ItemProperties, &Name)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    let Ok((player_entity, mut equipment, inventory, mut armor, mut damage)) =
        player_query.single_mut()
    else {
        return;
    };

    for msg in messages.read() {
        // Item must be in inventory
        if !inventory.items.contains(&msg.item_entity) {
            continue;
        }
        let Ok((props, name)) = item_query.get(msg.item_entity) else {
            continue;
        };

        // Determine target slot (rings fill ring_l first, then ring_r)
        let slot = match Equipment::slot_for(props) {
            Some("ring_l") => {
                if equipment.ring_l.is_none() || equipment.ring_l == Some(msg.item_entity) {
                    "ring_l"
                } else if equipment.ring_r.is_none() {
                    "ring_r"
                } else {
                    "ring_l" // Replace left ring
                }
            }
            Some(s) => s,
            None => continue, // Not equippable
        };

        // Already in this slot — skip
        if equipment.get_entity(slot) == Some(msg.item_entity) {
            continue;
        }

        // Unequip whatever is currently in that slot
        if let Some(old_entity) = equipment.get_entity(slot) {
            if let Ok((old_props, _)) = item_query.get(old_entity) {
                unapply_item_effects(old_props, &mut armor, &mut damage);
                commands.entity(old_entity).remove::<Equipped>();
            } else {
                warn!("Equipped item entity {:?} in slot '{}' no longer exists; clearing slot.", old_entity, slot);
            }
            equipment.set_slot(slot, None);
        }

        // Equip the new item
        equipment.set_slot(slot, Some(msg.item_entity));
        commands.entity(msg.item_entity).insert(Equipped);
        apply_item_effects(props, &mut armor, &mut damage);

        log_writer.write(GameLogMessage(format!("You equip the {}.", name.0)));
        finish_writer.write(ActionFinishedEvent { entity: player_entity, base_cost: BASE_ACTION_COST });
    }
}

/// Unequips an item, keeping it in inventory.
pub fn handle_unequip_item(
    mut commands: Commands,
    mut messages: MessageReader<UnequipItemMessage>,
    mut player_query: Query<
        (Entity, &mut Equipment, &mut Armor, &mut crate::game::combat::Damage),
        With<Player>,
    >,
    item_query: Query<(&ItemProperties, &Name)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    let Ok((player_entity, mut equipment, mut armor, mut damage)) =
        player_query.single_mut()
    else {
        return;
    };

    for msg in messages.read() {
        let Some(slot) = equipment.find_slot(msg.item_entity) else {
            continue;
        };
        let Ok((props, name)) = item_query.get(msg.item_entity) else {
            warn!("Equipped item entity {:?} in slot '{}' no longer exists; clearing slot.", msg.item_entity, slot);
            equipment.set_slot(slot, None);
            continue;
        };

        equipment.set_slot(slot, None);
        commands.entity(msg.item_entity).remove::<Equipped>();
        unapply_item_effects(props, &mut armor, &mut damage);

        log_writer.write(GameLogMessage(format!("You unequip the {}.", name.0)));
        finish_writer.write(ActionFinishedEvent { entity: player_entity, base_cost: BASE_ACTION_COST });
    }
}

/// Removes an item from inventory and places it at the player's feet.
/// For stackable items with count > 1, splits off one item to the floor.
/// Auto-unequips if the item is currently equipped.
pub fn handle_drop_item(
    mut commands: Commands,
    mut messages: MessageReader<DropItemMessage>,
    mut player_query: Query<
        (Entity, &mut Equipment, &mut Inventory, &Position, &mut Armor, &mut crate::game::combat::Damage),
        With<Player>,
    >,
    item_query: Query<(&Name, &ItemProperties, Option<&ItemStack>, &Sprite, &Transform)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    let Ok((player_entity, mut equipment, mut inv, player_pos, mut armor, mut damage)) =
        player_query.single_mut()
    else {
        return;
    };

    for msg in messages.read() {
        // Auto-unequip if equipped
        if let Some(slot) = equipment.find_slot(msg.item_entity) {
            equipment.set_slot(slot, None);
            commands.entity(msg.item_entity).remove::<Equipped>();
            if let Ok((_, props, _, _, _)) = item_query.get(msg.item_entity) {
                unapply_item_effects(props, &mut armor, &mut damage);
            }
        }

        let Some(idx) = inv.items.iter().position(|&e| e == msg.item_entity) else {
            continue;
        };

        let Ok((item_name, item_props, item_stack, item_sprite, item_transform)) =
            item_query.get(msg.item_entity)
        else {
            continue;
        };

        log_writer.write(GameLogMessage(format!("You drop the {}.", item_name.0)));

        // For stackable items with count > 1, split off one item to the floor.
        if let Some(stack) = item_stack {
            if stack.count > 1 {
                let new_count = stack.count - 1;
                let max_stack = stack.max_stack;

                // Decrement the inventory stack.
                commands.entity(msg.item_entity).insert(ItemStack { count: new_count, max_stack });

                // Spawn a new single-item floor entity by cloning the key components.
                let drop_pos = Position { x: player_pos.x, y: player_pos.y };
                commands.spawn((
                    Item,
                    Name(item_name.0.clone()),
                    GameEntityMarker,
                    FloorEntityMarker,
                    drop_pos,
                    item_props.clone(),
                    ItemStack { count: 1, max_stack },
                    item_sprite.clone(),
                    Transform {
                        translation: Vec3::new(
                            player_pos.x as f32 * crate::map::map::GRID_SIZE.x,
                            player_pos.y as f32 * crate::map::map::GRID_SIZE.y,
                            Z_ITEM,
                        ),
                        scale: item_transform.scale,
                        ..Default::default()
                    },
                    RenderLayers::layer(1),
                ));

                finish_writer.write(ActionFinishedEvent { entity: player_entity, base_cost: BASE_ACTION_COST });
                continue;
            }
        }

        // Single item (or non-stackable): move the entity itself to the floor.
        inv.items.remove(idx);
        commands
            .entity(msg.item_entity)
            .insert(Position { x: player_pos.x, y: player_pos.y })
            .insert(Visibility::Inherited)
            .insert(FloorEntityMarker)
            .remove::<InInventory>();

        finish_writer.write(ActionFinishedEvent { entity: player_entity, base_cost: BASE_ACTION_COST });
    }
}

// --- Loot Table ---

#[derive(Debug, Clone)]
pub struct LootEntry {
    pub item: String,
    pub spawn_chance: f32,
    pub count_min: u32,
    pub count_max: u32,
}

/// Component placed on monster entities that defines what items they may drop on death.
#[derive(Component, Debug, Clone, Default)]
pub struct LootTable {
    pub entries: Vec<LootEntry>,
}

// --- Plugin ---

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ItemKind>()
            .register_type::<ArmorSlot>()
            .register_type::<Rarity>()
            .register_type::<Effect>()
            .register_type::<ItemProperties>()
            .register_type::<ItemStack>()
            .register_type::<Equipment>()
            .init_resource::<SelectedInventorySlot>()
            // Messages are registered here; the handler systems live in TurnOrderPlugin's
            // Processing chain so they emit ActionFinishedEvent and cost a turn.
            .add_message::<DropItemMessage>()
            .add_message::<EquipItemMessage>()
            .add_message::<UnequipItemMessage>();
    }
}
