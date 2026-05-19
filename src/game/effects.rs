use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        use crate::game::turns::ProcessingPhase;
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
        actions::{finish_turn, ActionFinishedEvent, ActionKind, PendingPlayerAction},
        combat::Health,
        items::{ItemProperties, ItemStack},
        magic::{GameStatusEffectsExt, StatusEffectKind, StatusEffects},
        turns::TurnState,
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
    /// Zap a staff — opens targeting UI. The staff's StaffData determines targeting mode.
    ZapStaff,
}

// --- Pure helpers (testable without ECS) ---

/// Result of applying a heal effect to an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealResult {
    /// HP actually restored (after clamping to max).
    pub healed: i32,
    /// New current HP value.
    pub new_current: i32,
}

/// Compute the result of healing `amount` HP on an entity with the given
/// current/max health. Returns the clamped result without mutating anything.
pub fn compute_heal(current_hp: i32, max_hp: i32, amount: i32) -> HealResult {
    let new_current = (current_hp + amount).min(max_hp);
    let healed = new_current - current_hp;
    HealResult { healed, new_current }
}

/// Apply an antidote to `status_effects`: remove all Poisoned effects and add
/// PoisonResistance for `turns`. Returns whether the entity was poisoned before.
pub fn apply_antidote(status_effects: &mut StatusEffects, turns: u32) -> bool {
    let was_poisoned = status_effects.is_poisoned();
    status_effects.remove_kind(|k| matches!(k, StatusEffectKind::Poisoned));
    status_effects.add_effect(StatusEffectKind::PoisonResistance, turns);
    was_poisoned
}

/// Result of consuming one charge from a stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsumeStackResult {
    /// Stack still has items remaining (new count).
    Decremented { new_count: u32, max_stack: u32 },
    /// Stack is exhausted — item should be removed/despawned.
    Exhausted,
}

/// Compute the result of consuming one item from a stack.
/// If `stack_info` is `None`, the item is non-stackable and always exhausted.
pub fn consume_stack(stack_info: Option<(u32, u32)>) -> ConsumeStackResult {
    match stack_info {
        Some((count, max_stack)) if count > 1 => {
            ConsumeStackResult::Decremented { new_count: count - 1, max_stack }
        }
        _ => ConsumeStackResult::Exhausted,
    }
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
    staff_query: Query<(
        &crate::game::staves::StaffData,
        &crate::game::staves::Rechargeable,
        Option<&crate::game::enchantment::Enchantment>,
    )>,
    consumable_query: Query<(), With<crate::components::Consumable>>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut next_ingame: ResMut<NextState<crate::game::InGameState>>,
    mut next_turn: ResMut<NextState<TurnState>>,
    mut targeting_context: ResMut<crate::game::targeting::TargetingContext>,
    mut pending: ResMut<PendingPlayerAction>,
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
                let result = compute_heal(health.current, health.max, amount);
                health.current = result.new_current;
                log_writer.write(GameLogMessage(format!(
                    "You drink the {} and recover {} HP.",
                    item_name, result.healed
                )));
            }
            Effect::EnchantItem => {
                // Transition to enchant selection UI — player picks which item to enchant.
                // The scroll is consumed here; the actual enchanting happens in the selection UI.
                next_ingame.set(crate::game::InGameState::EnchantSelect);
            }
            Effect::ApplyHaste(turns) => {
                status_effects.add_effect(StatusEffectKind::Hasted, turns);
                log_writer.write(GameLogMessage(format!(
                    "You drink the {} and feel incredibly fast! ({} turns)",
                    item_name, turns
                )));
            }
            Effect::ApplyFireResistance(turns) => {
                status_effects.add_effect(StatusEffectKind::FireResistance, turns);
                log_writer.write(GameLogMessage(format!(
                    "You drink the {} and feel resistant to fire! ({} turns)",
                    item_name, turns
                )));
            }
            Effect::Antidote(turns) => {
                let was_poisoned = apply_antidote(&mut status_effects, turns);
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
            Effect::ZapStaff => {
                // Go directly to targeting with this staff.
                if let Ok((staff_data, rech, enchant)) = staff_query.get(msg.item_entity) {
                    match crate::game::staves::begin_staff_zap(
                        msg.item_entity, player_entity,
                        staff_data, rech, enchant,
                        &mut targeting_context,
                    ) {
                        crate::game::staves::ZapResult::NoCharges => {
                            log_writer.write(GameLogMessage(format!(
                                "The {} has no charges left.", item_name
                            )));
                        }
                        crate::game::staves::ZapResult::SelfTarget { action } => {
                            pending.0 = Some(action);
                            next_turn.set(TurnState::Processing);
                            next_ingame.set(crate::game::InGameState::Running);
                        }
                        crate::game::staves::ZapResult::NeedsTargeting => {
                            next_ingame.set(crate::game::InGameState::Targeting);
                        }
                    }
                }
            }
        }

        // Only consume items with the Consumable marker (potions, scrolls).
        // Non-consumable items (staves, equipment) stay in inventory.
        if !consumable_query.contains(msg.item_entity) {
            // Non-consumable effects that don't handle their own turn (like ZapStaff
            // which opens a sub-screen) must NOT call finish_turn here — the sub-screen
            // system handles turn completion.
            continue;
        }

        // Consume one from the stack; only remove/despawn when count reaches 0.
        match consume_stack(stack_info) {
            ConsumeStackResult::Decremented { new_count, max_stack } => {
                commands.entity(msg.item_entity).insert(ItemStack { count: new_count, max_stack });
                finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
                continue;
            }
            ConsumeStackResult::Exhausted => {}
        }
        inv.items.retain(|&e| e != msg.item_entity);
        commands.entity(msg.item_entity).despawn();

        finish_turn(&mut commands, &mut finish_writer, player_entity, BASE_ACTION_COST, ActionKind::Movement);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Helper constructors ---

    fn health(current: i32, max: i32) -> Health {
        Health { current, max }
    }

    fn poisoned_effects(dpt: i32, turns: u32) -> StatusEffects {
        let mut fx = StatusEffects::default();
        fx.add_effect_with_magnitude(StatusEffectKind::Poisoned, turns, dpt, None);
        fx
    }

    // ================================================================
    // compute_heal
    // ================================================================

    #[test]
    fn heal_partial_damage() {
        // 80/100 HP, heal 15 -> 95/100
        let result = compute_heal(80, 100, 15);
        assert_eq!(result.new_current, 95);
        assert_eq!(result.healed, 15);
    }

    #[test]
    fn heal_clamps_to_max_hp() {
        // 90/100 HP, heal 20 -> 100/100, only 10 actually healed
        let result = compute_heal(90, 100, 20);
        assert_eq!(result.new_current, 100);
        assert_eq!(result.healed, 10);
    }

    #[test]
    fn heal_at_full_hp_has_no_effect() {
        // 100/100 HP, heal 50 -> still 100, healed 0
        let result = compute_heal(100, 100, 50);
        assert_eq!(result.new_current, 100);
        assert_eq!(result.healed, 0);
    }

    #[test]
    fn heal_from_one_hp() {
        // 1/100 HP, heal 99 -> 100
        let result = compute_heal(1, 100, 99);
        assert_eq!(result.new_current, 100);
        assert_eq!(result.healed, 99);
    }

    #[test]
    fn heal_exact_to_max() {
        // 50/100 HP, heal 50 -> exactly 100
        let result = compute_heal(50, 100, 50);
        assert_eq!(result.new_current, 100);
        assert_eq!(result.healed, 50);
    }

    #[test]
    fn heal_large_overheal_clamps() {
        // 1/50 HP, heal 1000 -> 50
        let result = compute_heal(1, 50, 1000);
        assert_eq!(result.new_current, 50);
        assert_eq!(result.healed, 49);
    }

    #[test]
    fn heal_zero_amount() {
        let result = compute_heal(50, 100, 0);
        assert_eq!(result.new_current, 50);
        assert_eq!(result.healed, 0);
    }

    // ================================================================
    // Effect enum variants
    // ================================================================

    #[test]
    fn effect_heal_hp_stores_amount() {
        let effect = Effect::HealHp(25);
        assert_eq!(effect, Effect::HealHp(25));
    }

    #[test]
    fn effect_apply_haste_stores_duration() {
        let effect = Effect::ApplyHaste(10);
        assert_eq!(effect, Effect::ApplyHaste(10));
    }

    #[test]
    fn effect_apply_fire_resistance_stores_duration() {
        let effect = Effect::ApplyFireResistance(15);
        assert_eq!(effect, Effect::ApplyFireResistance(15));
    }

    #[test]
    fn effect_antidote_stores_duration() {
        let effect = Effect::Antidote(20);
        assert_eq!(effect, Effect::Antidote(20));
    }

    #[test]
    fn effect_enchant_item_variant() {
        let effect = Effect::EnchantItem;
        assert_eq!(effect, Effect::EnchantItem);
    }

    #[test]
    fn effect_variants_are_distinct() {
        assert_ne!(Effect::HealHp(10), Effect::ApplyHaste(10));
        assert_ne!(Effect::ApplyHaste(10), Effect::ApplyFireResistance(10));
        assert_ne!(Effect::Antidote(10), Effect::ApplyHaste(10));
        assert_ne!(Effect::EnchantItem, Effect::HealHp(0));
    }

    #[test]
    fn effect_clone_preserves_value() {
        let original = Effect::HealHp(42);
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    // ================================================================
    // consume_stack
    // ================================================================

    #[test]
    fn consume_stack_decrements_when_multiple() {
        let result = consume_stack(Some((5, 10)));
        assert_eq!(result, ConsumeStackResult::Decremented { new_count: 4, max_stack: 10 });
    }

    #[test]
    fn consume_stack_exhausted_at_one() {
        let result = consume_stack(Some((1, 10)));
        assert_eq!(result, ConsumeStackResult::Exhausted);
    }

    #[test]
    fn consume_stack_exhausted_when_none() {
        // Non-stackable item (no ItemStack component)
        let result = consume_stack(None);
        assert_eq!(result, ConsumeStackResult::Exhausted);
    }

    #[test]
    fn consume_stack_from_two_to_one() {
        let result = consume_stack(Some((2, 5)));
        assert_eq!(result, ConsumeStackResult::Decremented { new_count: 1, max_stack: 5 });
    }

    #[test]
    fn consume_stack_preserves_max_stack() {
        let result = consume_stack(Some((3, 99)));
        assert_eq!(result, ConsumeStackResult::Decremented { new_count: 2, max_stack: 99 });
    }

    // ================================================================
    // Status effect application — Haste
    // ================================================================

    #[test]
    fn apply_haste_sets_hasted_status() {
        let mut fx = StatusEffects::default();
        fx.add_effect(StatusEffectKind::Hasted, 10);
        assert!(fx.is_hasted());
    }

    #[test]
    fn apply_haste_correct_duration() {
        let mut fx = StatusEffects::default();
        fx.add_effect(StatusEffectKind::Hasted, 10);
        assert_eq!(fx.effects.len(), 1);
        assert_eq!(fx.effects[0].remaining_turns, 10);
    }

    #[test]
    fn apply_haste_speed_multiplier() {
        let mut fx = StatusEffects::default();
        fx.add_effect(StatusEffectKind::Hasted, 5);
        assert_eq!(fx.speed_delay_multiplier(), 0.5);
    }

    // ================================================================
    // Status effect application — Poison
    // ================================================================

    #[test]
    fn apply_poison_sets_poisoned_status() {
        let fx = poisoned_effects(3, 5);
        assert!(fx.is_poisoned());
    }

    #[test]
    fn apply_poison_correct_damage_per_turn() {
        let fx = poisoned_effects(4, 8);
        assert_eq!(fx.poison_damage(), Some(4));
    }

    #[test]
    fn apply_poison_correct_duration() {
        let fx = poisoned_effects(2, 6);
        assert_eq!(fx.effects[0].remaining_turns, 6);
    }

    // ================================================================
    // Antidote effect
    // ================================================================

    #[test]
    fn antidote_removes_poison() {
        let mut fx = poisoned_effects(3, 10);
        assert!(fx.is_poisoned());

        let was_poisoned = apply_antidote(&mut fx, 5);
        assert!(was_poisoned);
        assert!(!fx.is_poisoned());
    }

    #[test]
    fn antidote_adds_poison_resistance() {
        let mut fx = poisoned_effects(3, 10);
        apply_antidote(&mut fx, 8);

        // Should have PoisonResistance but no Poisoned
        assert!(fx.is_poison_resistant());
        assert!(!fx.is_poisoned());
    }

    #[test]
    fn antidote_resistance_has_correct_duration() {
        let mut fx = StatusEffects::default();
        apply_antidote(&mut fx, 12);

        let resist = fx.effects.iter().find(|e| e.kind == StatusEffectKind::PoisonResistance).unwrap();
        assert_eq!(resist.remaining_turns, 12);
    }

    #[test]
    fn antidote_when_not_poisoned_returns_false() {
        let mut fx = StatusEffects::default();
        let was_poisoned = apply_antidote(&mut fx, 5);
        assert!(!was_poisoned);
        // Still adds resistance
        assert!(fx.is_poison_resistant());
    }

    #[test]
    fn antidote_preserves_other_status_effects() {
        let mut fx = StatusEffects::default();
        fx.add_effect(StatusEffectKind::Hasted, 10);
        fx.add_effect_with_magnitude(StatusEffectKind::Poisoned, 8, 5, None);
        fx.add_effect_with_magnitude(StatusEffectKind::Burning, 3, 2, None);

        apply_antidote(&mut fx, 5);

        // Poison removed, haste and burning preserved, resistance added
        assert!(!fx.is_poisoned());
        assert!(fx.is_hasted());
        assert!(fx.is_burning());
        assert!(fx.is_poison_resistant());
    }

    // ================================================================
    // Fire resistance effect
    // ================================================================

    #[test]
    fn apply_fire_resistance_sets_status() {
        let mut fx = StatusEffects::default();
        fx.add_effect(StatusEffectKind::FireResistance, 10);
        assert!(fx.is_fire_resistant());
    }

    #[test]
    fn apply_fire_resistance_correct_duration() {
        let mut fx = StatusEffects::default();
        fx.add_effect(StatusEffectKind::FireResistance, 15);
        let entry = fx.effects.iter().find(|e| e.kind == StatusEffectKind::FireResistance).unwrap();
        assert_eq!(entry.remaining_turns, 15);
    }

    // ================================================================
    // Multiple effects scenario (simulated item with several effects)
    // ================================================================

    #[test]
    fn multiple_effects_all_apply() {
        // Simulate an item that heals and applies haste (testing that
        // both effect types can operate on the same entity state)
        let mut h = health(50, 100);
        let mut fx = StatusEffects::default();

        // Effect 1: Heal
        let heal_result = compute_heal(h.current, h.max, 20);
        h.current = heal_result.new_current;

        // Effect 2: Haste
        fx.add_effect(StatusEffectKind::Hasted, 10);

        // Both applied correctly
        assert_eq!(h.current, 70);
        assert!(fx.is_hasted());
    }

    #[test]
    fn heal_then_antidote_both_apply() {
        let mut h = health(30, 100);
        let mut fx = poisoned_effects(5, 10);

        // Heal
        let heal_result = compute_heal(h.current, h.max, 40);
        h.current = heal_result.new_current;

        // Antidote
        let was_poisoned = apply_antidote(&mut fx, 8);

        assert_eq!(h.current, 70);
        assert!(was_poisoned);
        assert!(!fx.is_poisoned());
    }

    // ================================================================
    // Edge cases
    // ================================================================

    #[test]
    fn heal_result_struct_debug_display() {
        let result = compute_heal(50, 100, 10);
        // Verify Debug trait works (compile-time check + runtime sanity)
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("HealResult"));
    }

    #[test]
    fn consume_stack_result_debug_display() {
        let result = consume_stack(Some((3, 5)));
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Decremented"));
    }

    #[test]
    fn antidote_double_application_still_has_resistance() {
        let mut fx = poisoned_effects(3, 5);
        apply_antidote(&mut fx, 5);
        // Apply antidote again when not poisoned
        let was_poisoned = apply_antidote(&mut fx, 10);
        assert!(!was_poisoned);
        // Resistance should have been refreshed to the longer duration
        let resist = fx.effects.iter().find(|e| e.kind == StatusEffectKind::PoisonResistance).unwrap();
        assert_eq!(resist.remaining_turns, 10);
    }
}
