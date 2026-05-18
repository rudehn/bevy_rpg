use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{Equipped, FloorEntityMarker, GameEntityMarker, InInventory, Inventory, Item, Name, Position};
use crate::constants::{BASE_ACTION_COST, UNARMED_DAMAGE, Z_ITEM};
use crate::game::abilities::OnHitTriggerMessage;
use crate::game::actions::{finish_turn, ActionFinishedEvent, ActionKind};
use crate::game::combat::{Damage, DamageSource, GameRng, HealEvent, Health, HealthRegen};
use crate::game::effects::Effect;
use crate::game::enchantment::{display_item_name, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified};
use crate::game::actions::SpeedStats;
use crate::game::magic::{GameStatusEffectsExt, StatusEffectKind, StatusEffects};
use crate::game::stats::{Armor, DamageBonus, Dodge, HitBonus};
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
    Staff,
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ItemKind::Consumable => "Consumable",
            ItemKind::Weapon => "Weapon",
            ItemKind::Armor => "Armor",
            ItemKind::Ring => "Ring",
            ItemKind::Amulet => "Amulet",
            ItemKind::Staff => "Staff",
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

/// On-hit effect attached to a weapon. Procs after a successful melee
/// or ranged hit, in addition to the base damage roll. Mirrors a
/// subset of monster `AbilityDef` variants — the ones that make sense
/// to bake into gear rather than into a creature's intrinsic abilities.
///
/// Wielder-agnostic: the same dagger procs the same poison whether the
/// player or a cultist swings it.
#[derive(Debug, Clone, Reflect, Serialize, Deserialize, PartialEq, Eq)]
pub enum OnHitEffect {
    BurningStrike { damage_per_turn: i32, duration: u32, chance: u32 },
    PoisonStrike { damage_per_turn: i32, duration: u32, chance: u32 },
    StunningBlow { duration: u32, chance: u32 },
    SlowStrike { duration: u32, chance: u32 },
    LifeDrain { percent: i32 },
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
    /// Attack speed multiplier for weapons (0.5 = half cost / twice as fast, 1.0 = normal).
    /// Defaults to 1.0 if not specified.
    #[serde(default = "default_attack_speed")]
    pub attack_speed: f32,
    /// Staff effect type (only for Staff items).
    #[serde(default)]
    pub staff_effect: Option<crate::game::staves::StaffEffect>,
    /// Base recharge rate for staves (turns per charge at +0 enchantment).
    #[serde(default)]
    pub base_recharge: u32,
    /// Dodge bonus granted when equipped.
    #[serde(default)]
    pub dodge_bonus: i32,
    /// Flat hit bonus granted when equipped.
    #[serde(default)]
    pub hit_bonus: i32,
    /// Flat damage bonus granted when equipped (from rings/amulets, not weapon dice).
    #[serde(default)]
    pub damage_bonus: i32,
    /// Regen rate bonus granted when equipped.
    #[serde(default)]
    pub regen_bonus: i32,
    /// Max HP bonus granted when equipped.
    #[serde(default)]
    pub max_hp_bonus: i32,
    /// Speed delay modifier when equipped (negative = faster, positive = slower).
    #[serde(default)]
    pub delay_modifier: f32,
    /// Vision range bonus when equipped (Ring of Perception).
    #[serde(default)]
    pub vision_bonus: i32,
    /// Per-damage-type resistance percentages granted while equipped.
    /// Already parsed from the manifest's string keys to `DamageType`.
    #[serde(default)]
    pub resistances: std::collections::HashMap<crate::game::combat::DamageType, i32>,
    /// Active weapon ability name (e.g. "Backstab", "Cleave"). The Sword
    /// has none — it's the no-ability balance baseline.
    #[serde(default)]
    pub weapon_ability: Option<String>,
    /// Phase 3: which weapon-family skill applies on melee. None for
    /// non-weapons, staves (which use Evocations on zap, Fighting on
    /// bash), and any future weapon-shaped items that don't slot into
    /// a family.
    #[serde(default)]
    pub weapon_skill: Option<crate::game::skills::WeaponSkill>,
    /// For shields (OffHand armor): max successful block attempts the
    /// wearer gets per turn. 1 = buckler / 2 = kite / 3 = tower. Set
    /// to 0 for any non-shield item; the field is ignored unless the
    /// `armor_slot` is `OffHand`.
    #[serde(default)]
    pub max_blocks: u32,
    /// On-hit effects applied after a successful attack with this weapon.
    /// Empty for plain weapons; populated for proc-weapons (e.g. Ritual
    /// Dagger with `PoisonStrike`).
    #[serde(default)]
    pub on_hit_effects: Vec<OnHitEffect>,
    /// Phase 4 stealth penalty applied while equipped. Subtracted from
    /// the wearer's d20 stealth roll. 0 for non-armor + cloth/robe; up
    /// to ~5 for plate. Mirrors `ItemAsset.armor_stealth_penalty` and
    /// is summed across equipped armor slots by
    /// `crate::game::stealth::equipped_armor_stealth_penalty`.
    #[serde(default)]
    pub armor_stealth_penalty: i32,
}

fn default_attack_speed() -> f32 { 1.0 }

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
    /// non-equippable items (Consumable, Staff) or Armor missing an armor_slot.
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

/// Pure stat deltas computed from item properties and enchantment level.
/// Used by `apply_item_effects` and `unapply_item_effects` to keep the
/// core arithmetic testable without ECS dependencies.
#[derive(Debug, Clone, PartialEq)]
pub struct StatDelta {
    pub armor: i32,
    pub block: i32,
    /// Max shield-block attempts per turn. Non-zero only for shields
    /// (OffHand armor). Additive in the same shape as the rest of the
    /// delta, but since only one off-hand item is equipped at a time
    /// the additive form collapses to "the equipped shield's value".
    pub max_blocks: u32,
    pub dodge: i32,
    pub hit_bonus: i32,
    pub damage_bonus: i32,
    pub max_hp: i32,
    pub regen: i32,
    pub delay: f32,
    pub vision: i32,
}

/// Compute the stat deltas an item grants when equipped (including enchantment bonuses).
///
/// **Routing:** items in the `OffHand` slot (shields) send their
/// `defense` to `block` instead of `armor`. Block is a flat, all-damage-
/// types reduction; armor is a random roll on physical only. This lets
/// shields keep the existing `defense` field in items.ron without
/// schema churn while behaving fundamentally differently from chest
/// pieces.
pub fn compute_stat_delta(props: &ItemProperties, enchantment: Option<&crate::game::enchantment::Enchantment>) -> StatDelta {
    let is_offhand = matches!(props.armor_slot, Some(ArmorSlot::OffHand));
    let mut delta = StatDelta {
        armor: if is_offhand { 0 } else { props.defense },
        block: if is_offhand { props.defense } else { 0 },
        max_blocks: if is_offhand { props.max_blocks } else { 0 },
        dodge: props.dodge_bonus,
        hit_bonus: props.hit_bonus,
        damage_bonus: props.damage_bonus,
        max_hp: props.max_hp_bonus,
        regen: props.regen_bonus,
        delay: props.delay_modifier,
        vision: props.vision_bonus,
    };
    if let Some(ench) = enchantment {
        match props.kind {
            ItemKind::Weapon => { delta.damage_bonus += ench.level; }
            ItemKind::Armor  => {
                // Enchanting a shield boosts its Block; enchanting a
                // chest piece boosts its Armor.
                if is_offhand {
                    delta.block += ench.level;
                } else {
                    delta.armor += ench.level;
                }
            }
            _ => {}
        }
    }
    delta
}

/// Helper: reverses the armor/damage effects of an equipped item.
pub(crate) fn unapply_item_effects(
    props: &ItemProperties,
    enchantment: Option<&crate::game::enchantment::Enchantment>,
    armor: &mut Armor,
    block: &mut crate::game::stats::Block,
    dodge: &mut crate::game::stats::Dodge,
    hit_bonus: &mut crate::game::stats::HitBonus,
    damage: &mut crate::game::combat::Damage,
    damage_bonus: &mut crate::game::stats::DamageBonus,
    health: &mut crate::game::combat::Health,
    health_regen: &mut crate::game::combat::HealthRegen,
    speed: &mut crate::game::actions::SpeedStats,
    viewshed: &mut crate::components::Viewshed,
    resistances: &mut crate::game::combat::Resistances,
) {
    let d = compute_stat_delta(props, enchantment);
    armor.0 -= d.armor;
    block.0 -= d.block;
    dodge.0 -= d.dodge;
    hit_bonus.0 -= d.hit_bonus;
    damage_bonus.0 -= d.damage_bonus;
    health.max -= d.max_hp;
    health.current = health.current.min(health.max);
    health_regen.regen_rate -= d.regen;
    speed.base_movement_delay -= d.delay;
    speed.base_attack_delay -= d.delay;
    if d.vision != 0 {
        viewshed.range = (viewshed.range - d.vision).max(0);
        viewshed.dirty = true;
    }
    for (dt, amount) in &props.resistances {
        let entry = resistances.0.entry(*dt).or_insert(0);
        *entry -= amount;
        if *entry == 0 {
            resistances.0.remove(dt);
        }
    }
    if props.kind == ItemKind::Weapon {
        damage.0 = UNARMED_DAMAGE.to_string();
    }
}

/// Helper: applies the armor/damage effects of an equipped item.
fn apply_item_effects(
    props: &ItemProperties,
    enchantment: Option<&crate::game::enchantment::Enchantment>,
    armor: &mut Armor,
    block: &mut crate::game::stats::Block,
    dodge: &mut crate::game::stats::Dodge,
    hit_bonus: &mut crate::game::stats::HitBonus,
    damage: &mut crate::game::combat::Damage,
    damage_bonus: &mut crate::game::stats::DamageBonus,
    health: &mut crate::game::combat::Health,
    health_regen: &mut crate::game::combat::HealthRegen,
    speed: &mut crate::game::actions::SpeedStats,
    viewshed: &mut crate::components::Viewshed,
    resistances: &mut crate::game::combat::Resistances,
) {
    let d = compute_stat_delta(props, enchantment);
    armor.0 += d.armor;
    block.0 += d.block;
    dodge.0 += d.dodge;
    hit_bonus.0 += d.hit_bonus;
    damage_bonus.0 += d.damage_bonus;
    health.max += d.max_hp;
    health.current += d.max_hp;
    health_regen.regen_rate += d.regen;
    speed.base_movement_delay += d.delay;
    speed.base_attack_delay += d.delay;
    if d.vision != 0 {
        viewshed.range += d.vision;
        viewshed.dirty = true;
    }
    for (dt, amount) in &props.resistances {
        *resistances.0.entry(*dt).or_insert(0) += amount;
    }
    if props.kind == ItemKind::Weapon
        && let Some(dmg) = &props.damage {
            damage.0 = dmg.clone();
        }
}

/// Equips an item from the player's inventory into the appropriate slot.
/// If the slot is already occupied the old item is unequipped first (stays in inventory).
pub fn handle_equip_item(
    mut commands: Commands,
    mut messages: MessageReader<EquipItemMessage>,
    mut player_query: Query<
        (Entity, &mut Equipment, &Inventory, &mut Armor, &mut crate::game::stats::Block, &mut Dodge, &mut HitBonus, &mut Damage, &mut DamageBonus, &mut Health, &mut HealthRegen, &mut SpeedStats, &mut crate::components::Viewshed, &mut crate::game::combat::Resistances),
        With<Player>,
    >,
    mut shield_query: Query<(&mut crate::game::stats::MaxShieldBlocks, &mut crate::game::stats::ShieldBlocksUsed), With<Player>>,
    item_query: Query<(&ItemProperties, &Name, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    let Ok((player_entity, mut equipment, inventory, mut armor, mut block, mut dodge, mut hit_bonus, mut damage, mut damage_bonus, mut health, mut health_regen, mut speed, mut viewshed, mut resistances)) =
        player_query.single_mut()
    else {
        return;
    };
    let Ok((mut max_shield_blocks, mut shield_blocks_used)) = shield_query.single_mut() else {
        return;
    };

    for msg in messages.read() {
        // Item must be in inventory
        if !inventory.items.contains(&msg.item_entity) {
            continue;
        }
        let Ok((props, name, enchant, weapon_runic, armor_runic, runic_id)) = item_query.get(msg.item_entity) else {
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
            if let Ok((old_props, _, old_enchant, _, _, _)) = item_query.get(old_entity) {
                unapply_item_effects(old_props, old_enchant, &mut armor, &mut block, &mut dodge, &mut hit_bonus, &mut damage, &mut damage_bonus, &mut health, &mut health_regen, &mut speed, &mut viewshed, &mut resistances);
                if old_props.armor_slot == Some(ArmorSlot::OffHand) {
                    max_shield_blocks.0 = 0;
                    shield_blocks_used.0 = 0;
                }
                commands.entity(old_entity).remove::<Equipped>();
            } else {
                warn!("Equipped item entity {:?} in slot '{}' no longer exists; clearing slot.", old_entity, slot);
            }
            equipment.set_slot(slot, None);
        }

        // Equip the new item
        equipment.set_slot(slot, Some(msg.item_entity));
        commands.entity(msg.item_entity).insert(Equipped);
        apply_item_effects(props, enchant, &mut armor, &mut block, &mut dodge, &mut hit_bonus, &mut damage, &mut damage_bonus, &mut health, &mut health_regen, &mut speed, &mut viewshed, &mut resistances);
        if props.armor_slot == Some(ArmorSlot::OffHand) {
            max_shield_blocks.0 = props.max_blocks;
            shield_blocks_used.0 = 0;
        }

        let dname = display_item_name(&name.0, enchant, weapon_runic, armor_runic, runic_id);
        log_writer.write(GameLogMessage(format!("You equip the {}.", dname)));
        finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}

/// Unequips an item, keeping it in inventory.
pub fn handle_unequip_item(
    mut commands: Commands,
    mut messages: MessageReader<UnequipItemMessage>,
    mut player_query: Query<
        (Entity, &mut Equipment, &mut Armor, &mut crate::game::stats::Block, &mut Dodge, &mut HitBonus, &mut Damage, &mut DamageBonus, &mut Health, &mut HealthRegen, &mut SpeedStats, &mut crate::components::Viewshed, &mut crate::game::combat::Resistances),
        With<Player>,
    >,
    mut shield_query: Query<(&mut crate::game::stats::MaxShieldBlocks, &mut crate::game::stats::ShieldBlocksUsed), With<Player>>,
    item_query: Query<(&ItemProperties, &Name, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    let Ok((player_entity, mut equipment, mut armor, mut block, mut dodge, mut hit_bonus, mut damage, mut damage_bonus, mut health, mut health_regen, mut speed, mut viewshed, mut resistances)) =
        player_query.single_mut()
    else {
        return;
    };
    let Ok((mut max_shield_blocks, mut shield_blocks_used)) = shield_query.single_mut() else {
        return;
    };

    for msg in messages.read() {
        let Some(slot) = equipment.find_slot(msg.item_entity) else {
            continue;
        };
        let Ok((props, name, enchant, weapon_runic, armor_runic, runic_id)) = item_query.get(msg.item_entity) else {
            warn!("Equipped item entity {:?} in slot '{}' no longer exists; clearing slot.", msg.item_entity, slot);
            equipment.set_slot(slot, None);
            continue;
        };

        equipment.set_slot(slot, None);
        commands.entity(msg.item_entity).remove::<Equipped>();
        unapply_item_effects(props, enchant, &mut armor, &mut block, &mut dodge, &mut hit_bonus, &mut damage, &mut damage_bonus, &mut health, &mut health_regen, &mut speed, &mut viewshed, &mut resistances);
        if props.armor_slot == Some(ArmorSlot::OffHand) {
            max_shield_blocks.0 = 0;
            shield_blocks_used.0 = 0;
        }

        let dname = display_item_name(&name.0, enchant, weapon_runic, armor_runic, runic_id);
        log_writer.write(GameLogMessage(format!("You unequip the {}.", dname)));
        finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}

/// Removes an item from inventory and places it at the player's feet.
/// For stackable items with count > 1, splits off one item to the floor.
/// Auto-unequips if the item is currently equipped.
pub fn handle_drop_item(
    mut commands: Commands,
    mut messages: MessageReader<DropItemMessage>,
    mut player_query: Query<
        (Entity, &mut Equipment, &mut Inventory, &Position, &mut Armor, &mut crate::game::stats::Block, &mut Dodge, &mut HitBonus, &mut Damage, &mut DamageBonus, &mut Health, &mut HealthRegen, &mut SpeedStats, &mut crate::components::Viewshed, &mut crate::game::combat::Resistances),
        With<Player>,
    >,
    mut shield_query: Query<(&mut crate::game::stats::MaxShieldBlocks, &mut crate::game::stats::ShieldBlocksUsed), With<Player>>,
    item_query: Query<(&Name, &ItemProperties, Option<&ItemStack>, &Transform, Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    let Ok((player_entity, mut equipment, mut inv, player_pos, mut armor, mut block, mut dodge, mut hit_bonus, mut damage, mut damage_bonus, mut health, mut health_regen, mut speed, mut viewshed, mut resistances)) =
        player_query.single_mut()
    else {
        return;
    };
    let Ok((mut max_shield_blocks, mut shield_blocks_used)) = shield_query.single_mut() else {
        return;
    };

    for msg in messages.read() {
        // Auto-unequip if equipped
        if let Some(slot) = equipment.find_slot(msg.item_entity) {
            equipment.set_slot(slot, None);
            commands.entity(msg.item_entity).remove::<Equipped>();
            if let Ok((_, props, _, _, enchant, _, _, _)) = item_query.get(msg.item_entity) {
                unapply_item_effects(props, enchant, &mut armor, &mut block, &mut dodge, &mut hit_bonus, &mut damage, &mut damage_bonus, &mut health, &mut health_regen, &mut speed, &mut viewshed, &mut resistances);
                if props.armor_slot == Some(ArmorSlot::OffHand) {
                    max_shield_blocks.0 = 0;
                    shield_blocks_used.0 = 0;
                }
            }
        }

        let Some(idx) = inv.items.iter().position(|&e| e == msg.item_entity) else {
            continue;
        };

        let Ok((item_name, item_props, item_stack, item_transform, item_enchant, item_weapon_runic, item_armor_runic, item_runic_id)) =
            item_query.get(msg.item_entity)
        else {
            continue;
        };

        let dname = display_item_name(&item_name.0, item_enchant, item_weapon_runic, item_armor_runic, item_runic_id);
        log_writer.write(GameLogMessage(format!("You drop the {}.", dname)));

        // For stackable items with count > 1, split off one item to the floor.
        if let Some(stack) = item_stack
            && stack.count > 1 {
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

                finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
                continue;
            }

        // Single item (or non-stackable): move the entity itself to the floor.
        inv.items.remove(idx);
        commands
            .entity(msg.item_entity)
            .insert(Position { x: player_pos.x, y: player_pos.y })
            .insert(Visibility::Inherited)
            .insert(FloorEntityMarker)
            .remove::<InInventory>();

        finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}

/// On-hit handler for weapon-side procs (`ItemProperties.on_hit_effects`).
/// Runs alongside the monster ability on-hit handlers in `CombatReactionSet`.
/// Reads the attacker's equipped weapon and dispatches each declared effect.
/// Wielder-agnostic: works for any entity with an `Equipment` component
/// pointing at a weapon entity.
pub fn handle_weapon_on_hit_effects(
    mut messages: MessageReader<OnHitTriggerMessage>,
    mut game_rng: ResMut<GameRng>,
    attacker_query: Query<(&Name, &Equipment)>,
    defender_query: Query<&Name>,
    item_props_query: Query<&ItemProperties>,
    mut status_query: Query<&mut StatusEffects>,
    mut heal_writer: MessageWriter<HealEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for msg in messages.read() {
        if !matches!(msg.source, DamageSource::Melee | DamageSource::Ranged) {
            continue;
        }
        let Ok((attacker_name, equipment)) = attacker_query.get(msg.attacker) else { continue; };
        let Some(weapon_entity) = equipment.weapon else { continue; };
        let Ok(weapon_props) = item_props_query.get(weapon_entity) else { continue; };
        if weapon_props.on_hit_effects.is_empty() { continue; }
        let Ok(defender_name) = defender_query.get(msg.defender) else { continue; };

        for effect in &weapon_props.on_hit_effects {
            match effect {
                OnHitEffect::PoisonStrike { damage_per_turn, duration, chance } => {
                    if game_rng.0.roll_dice(1, 100) <= *chance as i32 {
                        if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                            effects.add_effect_with_magnitude(
                                StatusEffectKind::Poisoned, *duration, *damage_per_turn, None,
                            );
                        }
                        log_writer.write(GameLogMessage(format!(
                            "{}'s attack poisons {}!", attacker_name.0, defender_name.0
                        )));
                    }
                }
                OnHitEffect::BurningStrike { damage_per_turn, duration, chance } => {
                    if game_rng.0.roll_dice(1, 100) <= *chance as i32 {
                        if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                            effects.add_effect_with_magnitude(
                                StatusEffectKind::Burning, *duration, *damage_per_turn, None,
                            );
                        }
                        log_writer.write(GameLogMessage(format!(
                            "{}'s attack sets {} ablaze!", attacker_name.0, defender_name.0
                        )));
                    }
                }
                OnHitEffect::StunningBlow { duration, chance } => {
                    if game_rng.0.roll_dice(1, 100) <= *chance as i32 {
                        if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                            effects.add_effect(StatusEffectKind::Stunned, *duration);
                        }
                        log_writer.write(GameLogMessage(format!(
                            "{}'s blow stuns {}!", attacker_name.0, defender_name.0
                        )));
                    }
                }
                OnHitEffect::SlowStrike { duration, chance } => {
                    if game_rng.0.roll_dice(1, 100) <= *chance as i32 {
                        if let Ok(mut effects) = status_query.get_mut(msg.defender) {
                            effects.add_effect(StatusEffectKind::Slowed, *duration);
                        }
                        log_writer.write(GameLogMessage(format!(
                            "{}'s strike slows {}!", attacker_name.0, defender_name.0
                        )));
                    }
                }
                OnHitEffect::LifeDrain { percent } => {
                    let heal_amount = (msg.final_damage * percent / 100).max(1);
                    heal_writer.write(HealEvent {
                        target: msg.attacker,
                        amount: heal_amount,
                        source: None,
                    });
                    log_writer.write(GameLogMessage(format!(
                        "{} drains life from {}! (+{} HP)",
                        attacker_name.0, defender_name.0, heal_amount
                    )));
                }
            }
        }
    }
}

// --- Monster Loadout (deferred equip) ---

/// Marker placed by the monster spawner on entities whose `MonsterAsset`
/// declares `equipped: [..]`. A follow-up system reads the list, spawns
/// each item entity, and attaches them to the monster's `Equipment`.
///
/// The deferred design keeps `spawn_monster`'s signature small — the
/// processing system pulls `ItemManifest` via `Res` instead of forcing
/// every caller to thread it through.
#[derive(Component, Debug, Clone)]
pub struct UnequippedLoadout(pub Vec<String>);

/// Build `ItemProperties` from an `ItemAsset` — pure data conversion,
/// no entity spawning. Same shape the item spawner uses; extracted here
/// so the monster loadout system doesn't depend on the world-placement
/// half of `spawn_item`.
pub fn item_properties_from_asset(asset: &crate::assets::ItemAsset) -> ItemProperties {
    use crate::game::combat::DamageType;
    let kind = asset.item_kind();
    let armor_slot = asset.armor_data().map(|a| a.slot.clone());
    let defense = asset.armor_data().map(|a| a.defense).unwrap_or(0);
    let max_blocks = asset.armor_data().map(|a| a.max_blocks).unwrap_or(0);
    let damage = asset.weapon_data().map(|w| w.damage.clone());
    let weapon_range = asset.weapon_data().map(|w| w.weapon_range).unwrap_or(0);
    let attack_speed = asset.weapon_data().map(|w| w.attack_speed).unwrap_or(1.0);
    let weapon_ability = asset.weapon_data().and_then(|w| w.weapon_ability.clone());
    let weapon_skill = asset.weapon_data().and_then(|w| w.weapon_skill);
    let on_hit_effects = asset
        .weapon_data()
        .map(|w| w.on_hit_effects.clone())
        .unwrap_or_default();
    let staff_effect = asset.staff_data().map(|s| s.effect);
    let staff_base_recharge = asset.staff_data().map(|s| s.base_recharge).unwrap_or(0);
    let effect = asset.consumable_data().and_then(|c| c.effect.clone());
    let resistances = asset
        .resistances
        .iter()
        .map(|(k, v)| (DamageType::from_str(k), *v))
        .collect();
    ItemProperties {
        kind,
        armor_slot,
        damage,
        defense,
        rarity: asset.rarity.clone(),
        effect,
        weapon_range,
        attack_speed,
        staff_effect,
        base_recharge: staff_base_recharge,
        dodge_bonus: asset.dodge_bonus,
        hit_bonus: asset.hit_bonus,
        damage_bonus: asset.damage_bonus,
        regen_bonus: asset.regen_bonus,
        max_hp_bonus: asset.max_hp_bonus,
        delay_modifier: asset.delay_modifier,
        vision_bonus: asset.vision_bonus,
        resistances,
        weapon_ability,
        weapon_skill,
        max_blocks,
        on_hit_effects,
        armor_stealth_penalty: asset.armor_stealth_penalty,
    }
}

/// Processes `UnequippedLoadout` markers placed by the monster spawner.
/// For each declared item name: look up the asset, spawn a minimal item
/// entity (no world placement, no rendering, no inventory marker),
/// attach to the monster's `Equipment` in the appropriate slot, and
/// apply stat overrides (weapon damage dice replace the intrinsic
/// `damage`; armor `defense` adds to the monster's flat `Armor`;
/// shield `defense` adds to `Block`; armor `dodge_bonus` adds to
/// `Dodge`). Removes the marker on success.
///
/// Runs every frame (cheap when no loadouts are pending). Items are
/// tagged with `GameEntityMarker` so they're cleaned up on game-over.
pub fn process_monster_loadout_system(
    mut commands: Commands,
    item_manifests: Res<bevy::asset::Assets<crate::assets::ItemManifest>>,
    item_manifest_handle: Res<crate::assets::ItemManifestHandle>,
    mut loadout_query: Query<
        (
            Entity,
            &UnequippedLoadout,
            &mut Equipment,
            &mut Damage,
            &mut Armor,
            &mut crate::game::stats::Block,
            &mut Dodge,
            &mut crate::game::stats::MaxShieldBlocks,
        ),
    >,
) {
    let Some(manifest) = item_manifests.get(&item_manifest_handle.0) else { return; };

    for (monster_entity, loadout, mut equipment, mut damage, mut armor, mut block, mut dodge, mut max_blocks) in loadout_query.iter_mut() {
        for item_name in &loadout.0 {
            let Some(asset) = manifest.items.get(item_name) else {
                warn!("Monster loadout references unknown item '{}'", item_name);
                continue;
            };

            let props = item_properties_from_asset(asset);

            // Pick the destination slot.
            let slot = match Equipment::slot_for(&props) {
                Some(s) => s,
                None => {
                    warn!("Item '{}' has no equipment slot (kind: {:?}); skipping in monster loadout", item_name, props.kind);
                    continue;
                }
            };

            // Apply stat overrides BEFORE spawning the item entity so we
            // don't depend on the entity existing.
            match &props.kind {
                ItemKind::Weapon => {
                    if let Some(dmg) = &props.damage {
                        damage.0 = dmg.clone();
                    }
                }
                ItemKind::Armor => {
                    if matches!(props.armor_slot, Some(ArmorSlot::OffHand)) {
                        block.0 += props.defense;
                        max_blocks.0 = props.max_blocks;
                    } else {
                        armor.0 += props.defense;
                    }
                    dodge.0 += props.dodge_bonus;
                }
                _ => {}
            }

            // Spawn the equipped item entity. Minimal — no Position,
            // no FloorEntity, no Item marker (don't want it picked up).
            // GameEntityMarker so game-over cleanup catches it.
            let item_entity = commands
                .spawn((
                    crate::components::Name(asset.name.clone()),
                    props,
                    Equipped,
                    crate::components::GameEntityMarker,
                ))
                .id();

            // Attach to the monster's Equipment slot.
            equipment.set_slot(slot, Some(item_entity));
        }

        // Done processing this loadout — drop the marker.
        commands.entity(monster_entity).remove::<UnequippedLoadout>();
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
        use crate::game::turns::{CombatReactionSet, ProcessingPhase};
        app.register_type::<ItemKind>()
            .register_type::<ArmorSlot>()
            .register_type::<Rarity>()
            .register_type::<Effect>()
            .register_type::<OnHitEffect>()
            .register_type::<ItemProperties>()
            .register_type::<ItemStack>()
            .register_type::<Equipment>()
            .init_resource::<SelectedInventorySlot>()
            .add_message::<DropItemMessage>()
            .add_message::<EquipItemMessage>()
            .add_message::<UnequipItemMessage>()
            .add_systems(
                Update,
                (handle_equip_item, handle_unequip_item, handle_drop_item)
                    .in_set(ProcessingPhase::ResolveActions),
            )
            // Weapon-side on-hit procs (poison dagger, vampiric blade, …)
            // run in the same set as monster ability on-hit handlers so
            // they see fresh damage events from the same combat tick.
            .add_systems(
                Update,
                handle_weapon_on_hit_effects.in_set(CombatReactionSet),
            )
            // Phase B monster equipment: drains UnequippedLoadout markers
            // placed by the spawner. Runs every Update — cheap when empty;
            // monsters declaring `equipped:` resolve within 1-2 frames of
            // spawn, before the turn queue advances to their slot.
            .add_systems(
                Update,
                process_monster_loadout_system
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

// =======================================================================
// Tests
// =======================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::enchantment::Enchantment;
    use crate::game::combat::{Damage, Health, HealthRegen};
    use crate::game::stats::{Armor, Dodge, HitBonus, DamageBonus};
    use crate::game::actions::SpeedStats;

    // -- helpers ---------------------------------------------------------

    /// Create a test Entity from a u32 index. Bevy 0.17 removed `Entity::from_raw`;
    /// we use `from_raw_u32` which returns `Option<Entity>`.
    fn test_entity(n: u32) -> Entity {
        Entity::from_raw_u32(n).expect("valid test entity index")
    }

    /// Shorthand to build an `ItemProperties` with only the fields a test cares about.
    fn make_props(kind: ItemKind) -> ItemProperties {
        ItemProperties {
            kind,
            ..Default::default()
        }
    }

    fn make_weapon(damage_dice: &str) -> ItemProperties {
        ItemProperties {
            kind: ItemKind::Weapon,
            damage: Some(damage_dice.to_string()),
            ..Default::default()
        }
    }

    fn make_armor(defense: i32, slot: ArmorSlot) -> ItemProperties {
        ItemProperties {
            kind: ItemKind::Armor,
            defense,
            armor_slot: Some(slot),
            ..Default::default()
        }
    }

    fn make_ring(dodge: i32, hit: i32, dmg_bonus: i32) -> ItemProperties {
        ItemProperties {
            kind: ItemKind::Ring,
            dodge_bonus: dodge,
            hit_bonus: hit,
            damage_bonus: dmg_bonus,
            ..Default::default()
        }
    }

    /// A full stat set for use in apply/unapply helpers.
    struct StatSet {
        armor: Armor,
        block: crate::game::stats::Block,
        dodge: Dodge,
        hit_bonus: HitBonus,
        damage: Damage,
        damage_bonus: DamageBonus,
        health: Health,
        health_regen: HealthRegen,
        speed: SpeedStats,
        viewshed: crate::components::Viewshed,
        resistances: crate::game::combat::Resistances,
    }

    fn baseline_stats() -> StatSet {
        StatSet {
            armor: Armor(0),
            block: crate::game::stats::Block(0),
            dodge: Dodge(0),
            hit_bonus: HitBonus(0),
            damage: Damage(UNARMED_DAMAGE.to_string()),
            damage_bonus: DamageBonus(0),
            health: Health { current: 20, max: 20 },
            health_regen: HealthRegen { regen_rate: 0, regen_accumulator: 0 },
            speed: SpeedStats {
                base_movement_delay: 1.0,
                base_attack_delay: 1.0,
                movement_delay: 1.0,
                attack_delay: 1.0,
            },
            viewshed: crate::components::Viewshed::new(8),
            resistances: crate::game::combat::Resistances::default(),
        }
    }

    fn apply(stats: &mut StatSet, props: &ItemProperties, ench: Option<&Enchantment>) {
        apply_item_effects(
            props, ench,
            &mut stats.armor, &mut stats.block, &mut stats.dodge, &mut stats.hit_bonus,
            &mut stats.damage, &mut stats.damage_bonus,
            &mut stats.health, &mut stats.health_regen, &mut stats.speed,
            &mut stats.viewshed, &mut stats.resistances,
        );
    }

    fn unapply(stats: &mut StatSet, props: &ItemProperties, ench: Option<&Enchantment>) {
        unapply_item_effects(
            props, ench,
            &mut stats.armor, &mut stats.block, &mut stats.dodge, &mut stats.hit_bonus,
            &mut stats.damage, &mut stats.damage_bonus,
            &mut stats.health, &mut stats.health_regen, &mut stats.speed,
            &mut stats.viewshed, &mut stats.resistances,
        );
    }

    // ====================================================================
    // Equipment::slot_for
    // ====================================================================

    #[test]
    fn slot_for_weapon() {
        let props = make_props(ItemKind::Weapon);
        assert_eq!(Equipment::slot_for(&props), Some("weapon"));
    }

    #[test]
    fn slot_for_amulet() {
        let props = make_props(ItemKind::Amulet);
        assert_eq!(Equipment::slot_for(&props), Some("amulet"));
    }

    #[test]
    fn slot_for_ring_returns_ring_l() {
        let props = make_props(ItemKind::Ring);
        assert_eq!(Equipment::slot_for(&props), Some("ring_l"));
    }

    #[test]
    fn slot_for_armor_slots() {
        let cases = [
            (ArmorSlot::Chest, "chest"),
            (ArmorSlot::Helm, "helm"),
            (ArmorSlot::Gloves, "gloves"),
            (ArmorSlot::Boots, "boots"),
            (ArmorSlot::OffHand, "offhand"),
        ];
        for (slot, expected) in cases {
            let props = make_armor(5, slot);
            assert_eq!(Equipment::slot_for(&props), Some(expected));
        }
    }

    #[test]
    fn slot_for_armor_without_armor_slot_returns_none() {
        let props = ItemProperties {
            kind: ItemKind::Armor,
            armor_slot: None,
            ..Default::default()
        };
        assert_eq!(Equipment::slot_for(&props), None);
    }

    #[test]
    fn slot_for_consumable_returns_none() {
        let props = make_props(ItemKind::Consumable);
        assert_eq!(Equipment::slot_for(&props), None);
    }

    #[test]
    fn slot_for_staff_returns_none() {
        // Staves are activated from inventory, not equipped to a slot.
        let props = make_props(ItemKind::Staff);
        assert_eq!(Equipment::slot_for(&props), None);
    }

    // ====================================================================
    // Equipment::get_entity / set_slot / find_slot
    // ====================================================================

    #[test]
    fn get_entity_empty_equipment() {
        let eq = Equipment::default();
        for slot in ["weapon", "offhand", "helm", "chest", "gloves", "boots", "ring_l", "ring_r", "amulet"] {
            assert_eq!(eq.get_entity(slot), None, "slot {} should be None", slot);
        }
    }

    #[test]
    fn get_entity_unknown_slot_returns_none() {
        let eq = Equipment::default();
        assert_eq!(eq.get_entity("nonexistent"), None);
    }

    #[test]
    fn set_and_get_entity_roundtrip() {
        let mut eq = Equipment::default();
        let e = test_entity(42);
        eq.set_slot("weapon", Some(e));
        assert_eq!(eq.get_entity("weapon"), Some(e));

        eq.set_slot("weapon", None);
        assert_eq!(eq.get_entity("weapon"), None);
    }

    #[test]
    fn set_slot_unknown_does_not_panic() {
        let mut eq = Equipment::default();
        eq.set_slot("bogus", Some(test_entity(1)));
        // Should be a no-op; no slot is set
        for slot in ["weapon", "offhand", "helm", "chest", "gloves", "boots", "ring_l", "ring_r", "amulet"] {
            assert_eq!(eq.get_entity(slot), None);
        }
    }

    #[test]
    fn find_slot_returns_correct_slot() {
        let mut eq = Equipment::default();
        let e1 = test_entity(10);
        let e2 = test_entity(20);
        let e3 = test_entity(30);
        eq.set_slot("helm", Some(e1));
        eq.set_slot("ring_r", Some(e2));
        eq.set_slot("amulet", Some(e3));

        assert_eq!(eq.find_slot(e1), Some("helm"));
        assert_eq!(eq.find_slot(e2), Some("ring_r"));
        assert_eq!(eq.find_slot(e3), Some("amulet"));
    }

    #[test]
    fn find_slot_returns_none_when_entity_not_equipped() {
        let eq = Equipment::default();
        assert_eq!(eq.find_slot(test_entity(999)), None);
    }

    #[test]
    fn find_slot_after_clearing_returns_none() {
        let mut eq = Equipment::default();
        let e = test_entity(5);
        eq.set_slot("boots", Some(e));
        assert_eq!(eq.find_slot(e), Some("boots"));

        eq.set_slot("boots", None);
        assert_eq!(eq.find_slot(e), None);
    }

    // ====================================================================
    // Ring auto-fill logic (tested against the same branching as handle_equip_item)
    // ====================================================================

    /// Replicate the ring-slot selection logic from handle_equip_item.
    fn pick_ring_slot(equipment: &Equipment, item_entity: Entity) -> &'static str {
        if equipment.ring_l.is_none() || equipment.ring_l == Some(item_entity) {
            "ring_l"
        } else if equipment.ring_r.is_none() {
            "ring_r"
        } else {
            "ring_l" // fallback: replace left ring
        }
    }

    #[test]
    fn ring_auto_fill_first_goes_left() {
        let eq = Equipment::default();
        let ring = test_entity(1);
        assert_eq!(pick_ring_slot(&eq, ring), "ring_l");
    }

    #[test]
    fn ring_auto_fill_second_goes_right() {
        let mut eq = Equipment::default();
        eq.ring_l = Some(test_entity(1));
        let ring2 = test_entity(2);
        assert_eq!(pick_ring_slot(&eq, ring2), "ring_r");
    }

    #[test]
    fn ring_auto_fill_both_full_replaces_left() {
        let mut eq = Equipment::default();
        eq.ring_l = Some(test_entity(1));
        eq.ring_r = Some(test_entity(2));
        let ring3 = test_entity(3);
        assert_eq!(pick_ring_slot(&eq, ring3), "ring_l");
    }

    #[test]
    fn ring_auto_fill_same_entity_stays_in_left() {
        let e = test_entity(1);
        let mut eq = Equipment::default();
        eq.ring_l = Some(e);
        assert_eq!(pick_ring_slot(&eq, e), "ring_l");
    }

    // ====================================================================
    // Equipment slot conflict
    // ====================================================================

    #[test]
    fn slot_conflict_same_slot_occupied() {
        let mut eq = Equipment::default();
        let sword = test_entity(1);
        let axe = test_entity(2);
        eq.set_slot("weapon", Some(sword));

        // Slot is occupied by a different entity
        assert!(eq.get_entity("weapon").is_some());
        assert_ne!(eq.get_entity("weapon"), Some(axe));

        // After clearing and re-equipping, the new entity takes the slot
        eq.set_slot("weapon", Some(axe));
        assert_eq!(eq.get_entity("weapon"), Some(axe));
    }

    // ====================================================================
    // compute_stat_delta
    // ====================================================================

    #[test]
    fn stat_delta_weapon_no_enchantment() {
        let props = ItemProperties {
            kind: ItemKind::Weapon,
            hit_bonus: 2,
            damage_bonus: 1,
            ..Default::default()
        };
        let d = compute_stat_delta(&props, None);
        assert_eq!(d.armor, 0);
        assert_eq!(d.dodge, 0);
        assert_eq!(d.hit_bonus, 2);
        assert_eq!(d.damage_bonus, 1);
        assert_eq!(d.max_hp, 0);
        assert_eq!(d.regen, 0);
        assert_eq!(d.delay, 0.0);
    }

    #[test]
    fn stat_delta_weapon_with_enchantment_adds_damage_bonus() {
        let props = make_weapon("2d6");
        let ench = Enchantment { level: 3 };
        let d = compute_stat_delta(&props, Some(&ench));
        assert_eq!(d.damage_bonus, 3);
        assert_eq!(d.armor, 0); // enchantment goes to damage, not armor
    }

    #[test]
    fn stat_delta_armor_with_enchantment_adds_armor() {
        let props = make_armor(4, ArmorSlot::Chest);
        let ench = Enchantment { level: 2 };
        let d = compute_stat_delta(&props, Some(&ench));
        assert_eq!(d.armor, 6); // base 4 + enchant 2
        assert_eq!(d.damage_bonus, 0);
    }

    #[test]
    fn stat_delta_ring_enchantment_does_not_add_armor_or_damage() {
        let props = make_ring(1, 2, 3);
        let ench = Enchantment { level: 5 };
        let d = compute_stat_delta(&props, Some(&ench));
        // Ring enchantments do not add to armor or damage_bonus beyond base props
        assert_eq!(d.dodge, 1);
        assert_eq!(d.hit_bonus, 2);
        assert_eq!(d.damage_bonus, 3);
        assert_eq!(d.armor, 0);
    }

    #[test]
    fn stat_delta_full_stat_item() {
        let props = ItemProperties {
            kind: ItemKind::Ring,
            defense: 1,
            dodge_bonus: 2,
            hit_bonus: 3,
            damage_bonus: 4,
            max_hp_bonus: 10,
            regen_bonus: 5,
            delay_modifier: -0.1,
            ..Default::default()
        };
        let d = compute_stat_delta(&props, None);
        assert_eq!(d, StatDelta {
            armor: 1,
            block: 0,
            max_blocks: 0,
            dodge: 2,
            hit_bonus: 3,
            damage_bonus: 4,
            max_hp: 10,
            regen: 5,
            delay: -0.1,
            vision: 0,
        });
    }

    /// An item in the OffHand slot (a shield) routes its `defense` to
    /// `block` instead of `armor`, and routes its enchantment level
    /// the same way. This is the core schema-routing decision behind
    /// the Block stat.
    #[test]
    fn compute_stat_delta_routes_offhand_defense_to_block() {
        let props = ItemProperties {
            armor_slot: Some(ArmorSlot::OffHand),
            defense: 3,
            ..Default::default()
        };
        let d = compute_stat_delta(&props, None);
        assert_eq!(d.armor, 0);
        assert_eq!(d.block, 3);

        // With an Armor-kind enchantment on a shield: the enchant level
        // boosts Block, not Armor.
        let mut props_shield = props.clone();
        props_shield.kind = ItemKind::Armor;
        let ench = crate::game::enchantment::Enchantment { level: 2, ..Default::default() };
        let d2 = compute_stat_delta(&props_shield, Some(&ench));
        assert_eq!(d2.armor, 0);
        assert_eq!(d2.block, 5);
    }

    // ====================================================================
    // apply / unapply item effects
    // ====================================================================

    #[test]
    fn apply_armor_increases_stats() {
        let mut s = baseline_stats();
        let props = make_armor(5, ArmorSlot::Chest);
        apply(&mut s, &props, None);
        assert_eq!(s.armor.0, 5);
    }

    #[test]
    fn apply_weapon_sets_damage_dice() {
        let mut s = baseline_stats();
        let props = make_weapon("2d8+3");
        apply(&mut s, &props, None);
        assert_eq!(s.damage.0, "2d8+3");
    }

    #[test]
    fn unapply_weapon_resets_to_unarmed() {
        let mut s = baseline_stats();
        let props = make_weapon("2d8+3");
        apply(&mut s, &props, None);
        unapply(&mut s, &props, None);
        assert_eq!(s.damage.0, UNARMED_DAMAGE);
    }

    #[test]
    fn apply_then_unapply_is_identity_for_armor() {
        let mut s = baseline_stats();
        let original_armor = s.armor.0;
        let original_dodge = s.dodge.0;

        let props = ItemProperties {
            kind: ItemKind::Armor,
            defense: 7,
            dodge_bonus: 3,
            armor_slot: Some(ArmorSlot::Chest),
            ..Default::default()
        };
        let ench = Enchantment { level: 2 };
        apply(&mut s, &props, Some(&ench));
        unapply(&mut s, &props, Some(&ench));

        assert_eq!(s.armor.0, original_armor);
        assert_eq!(s.dodge.0, original_dodge);
    }

    #[test]
    fn apply_then_unapply_is_identity_for_weapon() {
        let mut s = baseline_stats();
        let original_hit = s.hit_bonus.0;
        let original_dmg_bonus = s.damage_bonus.0;

        let props = ItemProperties {
            kind: ItemKind::Weapon,
            hit_bonus: 4,
            damage_bonus: 2,
            damage: Some("3d6".to_string()),
            ..Default::default()
        };
        let ench = Enchantment { level: 3 };
        apply(&mut s, &props, Some(&ench));
        unapply(&mut s, &props, Some(&ench));

        assert_eq!(s.hit_bonus.0, original_hit);
        assert_eq!(s.damage_bonus.0, original_dmg_bonus);
        assert_eq!(s.damage.0, UNARMED_DAMAGE);
    }

    #[test]
    fn apply_then_unapply_is_identity_for_ring() {
        let mut s = baseline_stats();
        let orig = (s.dodge.0, s.hit_bonus.0, s.damage_bonus.0);

        let props = make_ring(2, 3, 1);
        let ench = Enchantment { level: 1 };
        apply(&mut s, &props, Some(&ench));
        unapply(&mut s, &props, Some(&ench));

        assert_eq!((s.dodge.0, s.hit_bonus.0, s.damage_bonus.0), orig);
    }

    #[test]
    fn apply_then_unapply_is_identity_for_health() {
        let mut s = baseline_stats();
        let orig_max = s.health.max;
        let orig_cur = s.health.current;

        let props = ItemProperties {
            kind: ItemKind::Amulet,
            max_hp_bonus: 15,
            ..Default::default()
        };
        apply(&mut s, &props, None);
        assert_eq!(s.health.max, orig_max + 15);
        assert_eq!(s.health.current, orig_cur + 15);

        unapply(&mut s, &props, None);
        assert_eq!(s.health.max, orig_max);
        // Current is clamped to max, so it should return to original
        assert_eq!(s.health.current, orig_cur);
    }

    #[test]
    fn apply_then_unapply_is_identity_for_speed() {
        let mut s = baseline_stats();
        let orig_move = s.speed.base_movement_delay;
        let orig_atk = s.speed.base_attack_delay;

        let props = ItemProperties {
            kind: ItemKind::Armor,
            delay_modifier: 0.3,
            armor_slot: Some(ArmorSlot::Boots),
            ..Default::default()
        };
        apply(&mut s, &props, None);
        assert!((s.speed.base_movement_delay - (orig_move + 0.3)).abs() < f32::EPSILON);

        unapply(&mut s, &props, None);
        assert!((s.speed.base_movement_delay - orig_move).abs() < f32::EPSILON);
        assert!((s.speed.base_attack_delay - orig_atk).abs() < f32::EPSILON);
    }

    #[test]
    fn apply_then_unapply_is_identity_for_regen() {
        let mut s = baseline_stats();
        let orig_regen = s.health_regen.regen_rate;

        let props = ItemProperties {
            kind: ItemKind::Ring,
            regen_bonus: 10,
            ..Default::default()
        };
        apply(&mut s, &props, None);
        assert_eq!(s.health_regen.regen_rate, orig_regen + 10);

        unapply(&mut s, &props, None);
        assert_eq!(s.health_regen.regen_rate, orig_regen);
    }

    #[test]
    fn unapply_hp_clamps_current_to_new_max() {
        let mut s = baseline_stats();
        let props = ItemProperties {
            kind: ItemKind::Amulet,
            max_hp_bonus: 10,
            ..Default::default()
        };
        apply(&mut s, &props, None);
        // current is 30, max is 30 after apply
        assert_eq!(s.health.current, 30);

        // Simulate the player taking some damage (still above original max)
        s.health.current = 25;

        unapply(&mut s, &props, None);
        // max goes back to 20, current should clamp from 25 to 20
        assert_eq!(s.health.max, 20);
        assert_eq!(s.health.current, 20);
    }

    #[test]
    fn apply_multiple_items_stacks_additively() {
        let mut s = baseline_stats();
        let helm = make_armor(2, ArmorSlot::Helm);
        let chest = make_armor(5, ArmorSlot::Chest);
        let ring = make_ring(1, 0, 0);

        apply(&mut s, &helm, None);
        apply(&mut s, &chest, None);
        apply(&mut s, &ring, None);

        assert_eq!(s.armor.0, 7);
        assert_eq!(s.dodge.0, 1);
    }

    // ====================================================================
    // ItemStack
    // ====================================================================

    #[test]
    fn item_stack_default_is_single() {
        let stack = ItemStack::default();
        assert_eq!(stack.count, 1);
        assert_eq!(stack.max_stack, 1);
    }

    #[test]
    fn stack_split_drops_one_leaves_rest() {
        let mut stack = ItemStack { count: 5, max_stack: 10 };
        // Simulate dropping one item from the stack
        let dropped = ItemStack { count: 1, max_stack: stack.max_stack };
        stack.count -= 1;

        assert_eq!(stack.count, 4);
        assert_eq!(dropped.count, 1);
        assert_eq!(dropped.max_stack, 10);
    }

    #[test]
    fn stack_split_from_two_leaves_one() {
        let mut stack = ItemStack { count: 2, max_stack: 10 };
        stack.count -= 1;
        assert_eq!(stack.count, 1);
    }

    #[test]
    fn stack_split_single_item_has_count_zero_after() {
        let mut stack = ItemStack { count: 1, max_stack: 1 };
        // For non-stackable (max_stack=1), dropping removes the entity itself,
        // so count going to 0 is only relevant for the > 1 branch.
        stack.count -= 1;
        assert_eq!(stack.count, 0);
    }

    // ====================================================================
    // Equipment: all-slots iteration via find_slot
    // ====================================================================

    #[test]
    fn all_slots_distinguishable() {
        let mut eq = Equipment::default();
        let slots = ["weapon", "offhand", "helm", "chest", "gloves", "boots", "ring_l", "ring_r", "amulet"];
        for (i, slot) in slots.iter().enumerate() {
            let e = test_entity(i as u32);
            eq.set_slot(slot, Some(e));
        }
        for (i, slot) in slots.iter().enumerate() {
            let e = test_entity(i as u32);
            assert_eq!(eq.get_entity(slot), Some(e), "get_entity failed for {}", slot);
            assert_eq!(eq.find_slot(e), Some(*slot), "find_slot failed for entity {}", i);
        }
    }

    #[test]
    fn find_slot_returns_first_matching_slot() {
        // If the same entity were somehow in two slots (shouldn't happen in practice),
        // find_slot returns the first one in its search order (weapon first).
        let mut eq = Equipment::default();
        let e = test_entity(1);
        eq.weapon = Some(e);
        eq.helm = Some(e);
        // weapon comes first in find_slot's check order
        assert_eq!(eq.find_slot(e), Some("weapon"));
    }

    // ====================================================================
    // Display impls (sanity)
    // ====================================================================

    #[test]
    fn item_kind_display() {
        assert_eq!(format!("{}", ItemKind::Weapon), "Weapon");
        assert_eq!(format!("{}", ItemKind::Consumable), "Consumable");
        assert_eq!(format!("{}", ItemKind::Staff), "Staff");
    }

    #[test]
    fn armor_slot_display() {
        assert_eq!(format!("{}", ArmorSlot::OffHand), "Off-Hand");
        assert_eq!(format!("{}", ArmorSlot::Chest), "Chest");
    }

    #[test]
    fn rarity_display() {
        assert_eq!(format!("{}", Rarity::Legendary), "Legendary");
        assert_eq!(format!("{}", Rarity::Common), "Common");
    }

    // ====================================================================
    // Vision bonus (Ring of Perception)
    // ====================================================================

    #[test]
    fn vision_bonus_extends_viewshed_range_and_marks_dirty() {
        let mut s = baseline_stats();
        let baseline_range = s.viewshed.range;
        s.viewshed.dirty = false;
        let props = ItemProperties {
            kind: ItemKind::Ring,
            vision_bonus: 4,
            ..Default::default()
        };
        apply(&mut s, &props, None);
        assert_eq!(s.viewshed.range, baseline_range + 4);
        assert!(s.viewshed.dirty, "FOV must recompute after vision bonus");
    }

    #[test]
    fn vision_bonus_unapply_restores_range() {
        let mut s = baseline_stats();
        let baseline_range = s.viewshed.range;
        let props = ItemProperties {
            kind: ItemKind::Ring,
            vision_bonus: 4,
            ..Default::default()
        };
        apply(&mut s, &props, None);
        unapply(&mut s, &props, None);
        assert_eq!(s.viewshed.range, baseline_range);
    }

    #[test]
    fn vision_bonus_zero_does_not_mark_dirty() {
        // Equipping a non-vision item shouldn't force a viewshed recompute.
        let mut s = baseline_stats();
        s.viewshed.dirty = false;
        let props = make_armor(2, ArmorSlot::Chest);
        apply(&mut s, &props, None);
        assert!(!s.viewshed.dirty);
    }

    // ====================================================================
    // Resistances (amulets)
    // ====================================================================

    fn make_amulet_with_resist(dt: crate::game::combat::DamageType, percent: i32) -> ItemProperties {
        let mut resistances = std::collections::HashMap::new();
        resistances.insert(dt, percent);
        ItemProperties {
            kind: ItemKind::Amulet,
            resistances,
            ..Default::default()
        }
    }

    #[test]
    fn amulet_apply_adds_resistance_entry() {
        use crate::game::combat::DamageType;
        let mut s = baseline_stats();
        let props = make_amulet_with_resist(DamageType::Fire, 50);
        apply(&mut s, &props, None);
        assert_eq!(s.resistances.0.get(&DamageType::Fire).copied(), Some(50));
    }

    #[test]
    fn amulet_unapply_removes_resistance_entry() {
        use crate::game::combat::DamageType;
        let mut s = baseline_stats();
        let props = make_amulet_with_resist(DamageType::Lightning, 50);
        apply(&mut s, &props, None);
        unapply(&mut s, &props, None);
        // After unapply, the entry must be cleaned up — not just zeroed.
        assert!(s.resistances.0.get(&DamageType::Lightning).is_none());
    }

    #[test]
    fn stacked_resistances_accumulate_and_separate_keys_dont_collide() {
        // Two amulets of different damage types are independent;
        // and equipping then unequipping one leaves the other intact.
        use crate::game::combat::DamageType;
        let mut s = baseline_stats();
        let inferno = make_amulet_with_resist(DamageType::Fire, 50);
        let antivenom = make_amulet_with_resist(DamageType::Poison, 50);

        apply(&mut s, &inferno, None);
        apply(&mut s, &antivenom, None);
        assert_eq!(s.resistances.0.get(&DamageType::Fire).copied(), Some(50));
        assert_eq!(s.resistances.0.get(&DamageType::Poison).copied(), Some(50));

        unapply(&mut s, &inferno, None);
        assert!(s.resistances.0.get(&DamageType::Fire).is_none());
        assert_eq!(
            s.resistances.0.get(&DamageType::Poison).copied(),
            Some(50),
            "removing fire amulet must not affect poison resistance"
        );
    }

    // ====================================================================
    // Tower Shield delay penalty
    // ====================================================================

    // ====================================================================
    // Phase B: monster equipment / loadout
    // ====================================================================

    /// `item_properties_from_asset` unpacks a tagged-union `ItemAsset`
    /// into the flat runtime `ItemProperties` — same shape the spawner
    /// produces. Verifies a weapon round-trips, including on_hit_effects.
    #[test]
    fn item_properties_from_asset_unpacks_weapon_with_on_hit() {
        let asset: crate::assets::ItemAsset = ron::from_str(r#"(
            name: "Test Dagger",
            kind: Weapon((
                damage: "1d4",
                attack_speed: 0.5,
                weapon_ability: Some("Backstab"),
                on_hit_effects: [
                    PoisonStrike(damage_per_turn: 1, duration: 3, chance: 30),
                ],
            )),
        )"#).expect("parse");
        let props = item_properties_from_asset(&asset);
        assert_eq!(props.kind, ItemKind::Weapon);
        assert_eq!(props.damage.as_deref(), Some("1d4"));
        assert_eq!(props.attack_speed, 0.5);
        assert_eq!(props.weapon_ability.as_deref(), Some("Backstab"));
        assert_eq!(props.on_hit_effects.len(), 1);
        assert!(matches!(
            props.on_hit_effects[0],
            OnHitEffect::PoisonStrike { chance: 30, .. }
        ));
    }

    /// Armor asset → ItemProperties: slot, defense, and max_blocks land
    /// in the flat fields the equip pipeline expects.
    #[test]
    fn item_properties_from_asset_unpacks_armor() {
        let asset: crate::assets::ItemAsset = ron::from_str(r#"(
            name: "Test Buckler",
            kind: Armor((slot: OffHand, defense: 1, max_blocks: 1)),
            dodge_bonus: -1,
        )"#).expect("parse");
        let props = item_properties_from_asset(&asset);
        assert_eq!(props.kind, ItemKind::Armor);
        assert_eq!(props.armor_slot, Some(ArmorSlot::OffHand));
        assert_eq!(props.defense, 1);
        assert_eq!(props.max_blocks, 1);
        assert_eq!(props.dodge_bonus, -1);
    }

    /// A monster declaring `equipped: ["Item"]` parses with the new
    /// field populated.
    #[test]
    fn monster_asset_parses_with_equipped_loadout() {
        let ron = r#"(
            name: "Test Cultist",
            vision: 8,
            damage: "1d2",
            base_hp: 9,
            species: Humanoid,
            equipped: ["Ritual Dagger", "Cult Robes"],
        )"#;
        let asset: crate::assets::MonsterAsset = ron::from_str(ron).expect("parse");
        assert_eq!(asset.equipped, vec!["Ritual Dagger".to_string(), "Cult Robes".to_string()]);
    }

    /// Default `equipped:` is empty — existing monsters without the
    /// field keep working unchanged.
    #[test]
    fn monster_asset_default_equipped_is_empty() {
        let ron = r#"(
            name: "Test Goblin",
            vision: 8,
            damage: "1d4",
            base_hp: 5,
            species: Humanoid,
        )"#;
        let asset: crate::assets::MonsterAsset = ron::from_str(ron).expect("parse");
        assert!(asset.equipped.is_empty());
    }

    /// `Equipment::slot_for` picks "weapon" for any weapon-kind item the
    /// monster loadout system might encounter — verifies the slot lookup
    /// the monster loadout path depends on. Smoke test against a typical
    /// weapon asset's properties.
    #[test]
    fn weapon_asset_routes_to_weapon_slot() {
        let asset: crate::assets::ItemAsset = ron::from_str(r#"(
            name: "Sword",
            kind: Weapon((damage: "1d6")),
        )"#).expect("parse");
        let props = item_properties_from_asset(&asset);
        assert_eq!(Equipment::slot_for(&props), Some("weapon"));
    }

    /// An OffHand armor (shield) routes to the offhand slot — important
    /// for the monster loadout path so shields end up in the right slot
    /// and contribute to Block rather than Armor.
    #[test]
    fn shield_asset_routes_to_offhand_slot() {
        let asset: crate::assets::ItemAsset = ron::from_str(r#"(
            name: "Buckler",
            kind: Armor((slot: OffHand, defense: 1, max_blocks: 1)),
        )"#).expect("parse");
        let props = item_properties_from_asset(&asset);
        assert_eq!(Equipment::slot_for(&props), Some("offhand"));
    }

    #[test]
    fn tower_shield_delay_penalty_increases_action_delay() {
        // Equipping a +0.1 delay shield raises base move and attack delay
        // by exactly 0.1; unequipping restores the baseline.
        let mut s = baseline_stats();
        let baseline_move = s.speed.base_movement_delay;
        let baseline_attack = s.speed.base_attack_delay;

        let props = ItemProperties {
            kind: ItemKind::Armor,
            armor_slot: Some(ArmorSlot::OffHand),
            defense: 5,
            delay_modifier: 0.1,
            ..Default::default()
        };
        apply(&mut s, &props, None);
        assert!((s.speed.base_movement_delay - (baseline_move + 0.1)).abs() < 1e-6);
        assert!((s.speed.base_attack_delay - (baseline_attack + 0.1)).abs() < 1e-6);

        unapply(&mut s, &props, None);
        assert!((s.speed.base_movement_delay - baseline_move).abs() < 1e-6);
        assert!((s.speed.base_attack_delay - baseline_attack).abs() < 1e-6);
    }
}
