use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    assets::SpellRegistryHandle,
    components::{Inventory, Name},
    constants::BASE_ACTION_COST,
    game::{
        actions::ActionFinishedEvent,
        combat::Health,
        items::{ItemProperties, ItemStack},
        magic::{ActiveSpells, KnownSpells},
        stats::Mana,
        spells::SpellRegistry,
    },
    player::Player,
    ui::game_log::GameLogMessage,
};

// --- Effect Enum ---

/// Describes the one-shot effect produced when a consumable item is used.
#[derive(Debug, Clone, PartialEq, Reflect, Serialize, Deserialize)]
pub enum Effect {
    /// Restore N hit points to the user (clamped to max HP).
    HealHp(i32),
    /// Restore N mana to the user (clamped to max mana).
    RestoreMana(i32),
    /// Teach the player a new spell (spellbook). Value is the spell ID from spells.ron.
    LearnSpell(String),
}

// --- Messages ---

/// Sent when the player chooses to use (consume) an item from their inventory.
#[derive(Message, Debug)]
pub struct UseItemMessage {
    pub item_entity: Entity,
}

// --- Systems ---

/// Applies the consumable effect of an item, removes it from inventory, and despawns it.
/// Costs one full turn via `ActionFinishedEvent`.
pub fn handle_use_item(
    mut commands: Commands,
    mut messages: MessageReader<UseItemMessage>,
    mut player_query: Query<
        (
            Entity,
            &mut Inventory,
            &mut Health,
            &mut Mana,
            &mut KnownSpells,
            &mut ActiveSpells,
        ),
        With<Player>,
    >,
    item_query: Query<(&Name, &ItemProperties, Option<&ItemStack>)>,
    spell_registry_handle: Res<SpellRegistryHandle>,
    spell_registries: Res<Assets<SpellRegistry>>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    let Ok((player_entity, mut inv, mut health, mut mana, mut known_spells, mut active_spells)) =
        player_query.single_mut()
    else {
        return;
    };

    for msg in messages.read() {
        if !inv.items.contains(&msg.item_entity) {
            continue;
        }

        // Clone what we need before releasing the item_query borrow.
        let (effect, item_name, stack_info) = {
            let Ok((name, props, stack)) = item_query.get(msg.item_entity) else {
                continue;
            };
            let stack_info = stack.map(|s| (s.count, s.max_stack));
            (props.effect.clone(), name.0.clone(), stack_info)
        };

        let Some(effect) = effect else {
            log_writer.write(GameLogMessage(format!(
                "The {} has no effect.",
                item_name
            )));
            finish_writer.write(ActionFinishedEvent {
                entity: player_entity,
                base_cost: BASE_ACTION_COST,
            });
            continue;
        };

        match effect {
            Effect::HealHp(amount) => {
                let before = health.current;
                health.current = (health.current + amount).min(health.max);
                let healed = health.current - before;
                log_writer.write(GameLogMessage(format!(
                    "You drink the {} and recover {} HP.",
                    item_name, healed
                )));
            }
            Effect::RestoreMana(amount) => {
                let before = mana.current;
                mana.current = (mana.current + amount).min(mana.max);
                let restored = mana.current - before;
                log_writer.write(GameLogMessage(format!(
                    "You drink the {} and restore {} mana.",
                    item_name, restored
                )));
            }
            Effect::LearnSpell(spell_id) => {
                // Look up display name from registry; fall back to the ID.
                let spell_name = spell_registries
                    .get(&spell_registry_handle.0)
                    .and_then(|r| r.spells.get(&spell_id))
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| spell_id.clone());

                if known_spells.spells.contains(&spell_id) {
                    log_writer.write(GameLogMessage(format!(
                        "You already know {}.",
                        spell_name
                    )));
                } else {
                    // Auto-slot into first empty slot if one exists.
                    let auto_slotted = active_spells
                        .slots
                        .iter_mut()
                        .find(|s| s.is_none())
                        .map(|slot| {
                            *slot = Some(spell_id.clone());
                            true
                        })
                        .unwrap_or(false);

                    known_spells.spells.push(spell_id.clone());

                    if auto_slotted {
                        log_writer.write(GameLogMessage(format!(
                            "You learn {} and it is slotted automatically.",
                            spell_name
                        )));
                    } else {
                        log_writer.write(GameLogMessage(format!(
                            "You learn {}. Open [S] Spells to assign it to a slot.",
                            spell_name
                        )));
                    }
                }
            }
        }

        // Consume one from the stack; only remove/despawn when count reaches 0.
        if let Some((count, max_stack)) = stack_info {
            if count > 1 {
                commands.entity(msg.item_entity).insert(ItemStack { count: count - 1, max_stack });
                finish_writer.write(ActionFinishedEvent {
                    entity: player_entity,
                    base_cost: BASE_ACTION_COST,
                });
                continue;
            }
        }
        inv.items.retain(|&e| e != msg.item_entity);
        commands.entity(msg.item_entity).despawn();

        finish_writer.write(ActionFinishedEvent {
            entity: player_entity,
            base_cost: BASE_ACTION_COST,
        });
    }
}
