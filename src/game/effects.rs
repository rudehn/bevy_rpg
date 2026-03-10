use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{Inventory, Name},
    constants::BASE_ACTION_COST,
    game::{
        actions::ActionFinishedEvent,
        combat::Health,
        items::ItemProperties,
        stats::AttributeModifiers,
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
    /// Permanently add N to the user's strength modifier.
    GainStr(i32),
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
        (Entity, &mut Inventory, &mut Health, &mut AttributeModifiers),
        With<Player>,
    >,
    item_query: Query<(&Name, &ItemProperties)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
) {
    let Ok((player_entity, mut inv, mut health, mut attr_mods)) = player_query.single_mut() else {
        return;
    };

    for msg in messages.read() {
        if !inv.items.contains(&msg.item_entity) {
            continue;
        }

        // Clone what we need before releasing the item_query borrow.
        let (effect, item_name) = {
            let Ok((name, props)) = item_query.get(msg.item_entity) else {
                continue;
            };
            (props.effect.clone(), name.0.clone())
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
            Effect::GainStr(amount) => {
                attr_mods.strength += amount;
                log_writer.write(GameLogMessage(format!(
                    "You drink the {}. You feel stronger!",
                    item_name
                )));
            }
        }

        // Consume the item: remove from inventory and despawn.
        inv.items.retain(|&e| e != msg.item_entity);
        commands.entity(msg.item_entity).despawn();

        finish_writer.write(ActionFinishedEvent {
            entity: player_entity,
            base_cost: BASE_ACTION_COST,
        });
    }
}
