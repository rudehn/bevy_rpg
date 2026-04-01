use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::game::turns::ProcessingPhase;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<UseItemMessage>()
            .add_systems(
                Update,
                handle_use_item.in_set(ProcessingPhase::ResolveActions),
            );
    }
}

use crate::{
    components::{Inventory, Name},
    constants::BASE_ACTION_COST,
    game::{
        actions::{finish_turn, ActionFinishedEvent, ActionKind},
        combat::Health,
        items::{ItemProperties, ItemStack},
        magic::{StatusEffectKind, StatusEffects},
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
    /// Prompt the player to select a weapon/armor to enchant by +1.
    EnchantItem,
    /// Apply Hasted status for N turns.
    ApplyHaste(u32),
    /// Apply temporary fire resistance for N turns.
    ApplyFireResistance(u32),
    /// Remove all poison effects and apply temporary poison resistance for N turns.
    Antidote(u32),
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
            &mut StatusEffects,
        ),
        With<Player>,
    >,
    item_query: Query<(&Name, &ItemProperties, Option<&ItemStack>)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut next_ingame: ResMut<NextState<crate::game::InGameState>>,
) {
    let Ok((player_entity, mut inv, mut health, mut status_effects)) =
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
            finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
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
            Effect::EnchantItem => {
                // Transition to enchant selection UI — player picks which item to enchant.
                // The scroll is consumed here; the actual enchanting happens in the selection UI.
                next_ingame.set(crate::game::InGameState::EnchantSelect);
            }
            Effect::ApplyHaste(turns) => {
                status_effects.add(StatusEffectKind::Hasted, turns);
                log_writer.write(GameLogMessage(format!(
                    "You drink the {} and feel incredibly fast! ({} turns)",
                    item_name, turns
                )));
            }
            Effect::ApplyFireResistance(turns) => {
                status_effects.add(StatusEffectKind::FireResistance, turns);
                log_writer.write(GameLogMessage(format!(
                    "You drink the {} and feel resistant to fire! ({} turns)",
                    item_name, turns
                )));
            }
            Effect::Antidote(turns) => {
                // Remove all poison effects
                let was_poisoned = status_effects.is_poisoned();
                status_effects.remove_kind(|k| matches!(k, StatusEffectKind::Poisoned { .. }));
                // Apply temporary poison resistance
                status_effects.add(StatusEffectKind::PoisonResistance, turns);
                if was_poisoned {
                    log_writer.write(GameLogMessage(format!(
                        "You drink the {}. The poison is purged from your body!",
                        item_name
                    )));
                } else {
                    log_writer.write(GameLogMessage(format!(
                        "You drink the {} and feel resistant to poison. ({} turns)",
                        item_name, turns
                    )));
                }
            }
        }

        // Consume one from the stack; only remove/despawn when count reaches 0.
        if let Some((count, max_stack)) = stack_info
            && count > 1 {
                commands.entity(msg.item_entity).insert(ItemStack { count: count - 1, max_stack });
                finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
                continue;
            }
        inv.items.retain(|&e| e != msg.item_entity);
        commands.entity(msg.item_entity).despawn();

        finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}
