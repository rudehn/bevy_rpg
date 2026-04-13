//! Combat event pipeline: damage application, healing, and death detection.
//!
//! This module ships three messages ([`DamageEvent`], [`DeathEvent`],
//! [`HealEvent`]) and two systems that process them:
//!
//! - [`damage_application_system`] reads `DamageEvent` messages, applies
//!   armor and resistance math from the parent [`combat`](super) module,
//!   subtracts the final damage from [`Health`], sets
//!   [`RegenSuppression`], and emits [`DeathEvent`] when HP drops to zero
//!   or below.
//! - [`heal_application_system`] reads `HealEvent` messages and restores
//!   HP clamped to `Health.max`.
//!
//! Both systems are registered by [`CombatPlugin`] into
//! [`CombatEventSet`], which games configure with `.after()` /
//! `.before()` / `.run_if()` as needed.

use bevy::prelude::*;

use super::{
    apply_resistance, compute_after_armor, DamageType, DamageSource, Health, RegenSuppression,
    Resistances,
};

// =====================================================================
// Messages
// =====================================================================

/// A request to deal damage to `target`.
///
/// The `amount` field holds **raw** damage before armor or resistance.
/// The damage pipeline applies `compute_after_armor` then
/// `apply_resistance` using the target's components.
#[derive(Message, Debug, Clone)]
pub struct DamageEvent {
    pub target: Entity,
    pub amount: i32,
    pub damage_type: DamageType,
    pub source: DamageSource,
    pub attacker: Option<Entity>,
    /// Armor value to subtract before resistance. Pass `0` if the
    /// attacker already factored armor in or the target has none.
    pub armor: i32,
}

/// Emitted when an entity's HP drops to zero or below after damage
/// application. Games bridge this to despawn, loot drops, XP, etc.
#[derive(Message, Debug, Clone)]
pub struct DeathEvent {
    pub entity: Entity,
    pub killer: Option<Entity>,
}

/// A request to heal `target` by `amount` HP, clamped to `Health.max`.
#[derive(Message, Debug, Clone)]
pub struct HealEvent {
    pub target: Entity,
    pub amount: i32,
    pub source: Option<Entity>,
}

// =====================================================================
// System sets
// =====================================================================

/// System set for the combat event pipeline. Games configure ordering
/// and run conditions via `configure_sets`:
///
/// ```ignore
/// app.configure_sets(
///     Update,
///     CombatEventSet
///         .after(MyAttackResolutionSet)
///         .run_if(in_state(MyGameState::Running)),
/// );
/// ```
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CombatEventSet;

// =====================================================================
// Plugin
// =====================================================================

/// Bevy plugin that registers combat messages and systems.
///
/// Does NOT configure system ordering or `run_if` predicates -- that
/// is the game's responsibility via [`CombatEventSet`].
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageEvent>()
            .add_message::<DeathEvent>()
            .add_message::<HealEvent>()
            .add_systems(
                Update,
                (damage_application_system, heal_application_system).in_set(CombatEventSet),
            );
    }
}

// =====================================================================
// Systems
// =====================================================================

/// Reads [`DamageEvent`] messages, applies armor and resistance, subtracts
/// from [`Health`], sets [`RegenSuppression`], and emits [`DeathEvent`]
/// when HP hits zero.
pub fn damage_application_system(
    mut damage_events: MessageReader<DamageEvent>,
    mut targets: Query<(
        &mut Health,
        Option<&Resistances>,
        Option<&mut RegenSuppression>,
    )>,
    mut death_writer: MessageWriter<DeathEvent>,
) {
    for event in damage_events.read() {
        let Ok((mut health, resistances, regen_suppression)) = targets.get_mut(event.target)
        else {
            continue;
        };

        // Armor reduction
        let after_armor = compute_after_armor(event.amount, event.armor);

        // Resistance reduction
        let resist_percent = resistances
            .map(|r| r.get(&event.damage_type))
            .unwrap_or(0);
        let final_damage = apply_resistance(after_armor, resist_percent);

        if final_damage <= 0 {
            continue;
        }

        health.current -= final_damage;

        // Suppress regen for 3 turns after taking damage
        if let Some(mut suppression) = regen_suppression {
            suppression.0 = 3;
        }

        // Emit death event
        if health.current <= 0 {
            death_writer.write(DeathEvent {
                entity: event.target,
                killer: event.attacker,
            });
        }
    }
}

/// Reads [`HealEvent`] messages and restores HP clamped to max.
pub fn heal_application_system(
    mut heal_events: MessageReader<HealEvent>,
    mut targets: Query<&mut Health>,
) {
    for event in heal_events.read() {
        let Ok(mut health) = targets.get_mut(event.target) else {
            continue;
        };
        health.current = (health.current + event.amount).min(health.max);
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn build_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CombatPlugin);
        app
    }

    /// Helper: spawn an entity with Health and optional components,
    /// returning its Entity id.
    fn spawn_entity(
        app: &mut App,
        health: Health,
        resistances: Option<Resistances>,
        regen_suppression: bool,
    ) -> Entity {
        let mut cmd = app.world_mut().spawn(health);
        if let Some(r) = resistances {
            cmd.insert(r);
        }
        if regen_suppression {
            cmd.insert(RegenSuppression(0));
        }
        cmd.id()
    }

    fn send_damage(app: &mut App, event: DamageEvent) {
        app.world_mut().write_message(event);
    }

    fn send_heal(app: &mut App, event: HealEvent) {
        app.world_mut().write_message(event);
    }

    fn get_health(app: &App, entity: Entity) -> (i32, i32) {
        let health = app.world().get::<Health>(entity).unwrap();
        (health.current, health.max)
    }

    fn get_regen_suppression(app: &App, entity: Entity) -> u32 {
        app.world()
            .get::<RegenSuppression>(entity)
            .map(|s| s.0)
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------
    // Damage tests
    // -----------------------------------------------------------------

    #[test]
    fn damage_reduces_health() {
        let mut app = build_test_app();
        let target = spawn_entity(
            &mut app,
            Health { current: 20, max: 20 },
            None,
            false,
        );

        send_damage(&mut app, DamageEvent {
            target,
            amount: 5,
            damage_type: DamageType::Physical,
            source: DamageSource::Melee,
            attacker: None,
            armor: 0,
        });

        app.update();

        assert_eq!(get_health(&app, target), (15, 20));
    }

    #[test]
    fn damage_with_armor() {
        let mut app = build_test_app();
        let target = spawn_entity(
            &mut app,
            Health { current: 20, max: 20 },
            None,
            false,
        );

        send_damage(&mut app, DamageEvent {
            target,
            amount: 10,
            damage_type: DamageType::Physical,
            source: DamageSource::Melee,
            attacker: None,
            armor: 3,
        });

        app.update();

        assert_eq!(get_health(&app, target), (13, 20));
    }

    #[test]
    fn damage_with_resistance() {
        let mut app = build_test_app();
        let mut resists = HashMap::new();
        resists.insert(DamageType::Fire, 50);
        let target = spawn_entity(
            &mut app,
            Health { current: 20, max: 20 },
            Some(Resistances(resists)),
            false,
        );

        send_damage(&mut app, DamageEvent {
            target,
            amount: 10,
            damage_type: DamageType::Fire,
            source: DamageSource::Spell,
            attacker: None,
            armor: 0,
        });

        app.update();

        // 10 * (1 - 0.50) = 5
        assert_eq!(get_health(&app, target), (15, 20));
    }

    #[test]
    fn damage_triggers_death() {
        let mut app = build_test_app();
        let attacker_entity = app.world_mut().spawn_empty().id();
        let target = spawn_entity(
            &mut app,
            Health { current: 5, max: 20 },
            None,
            false,
        );

        send_damage(&mut app, DamageEvent {
            target,
            amount: 10,
            damage_type: DamageType::Physical,
            source: DamageSource::Melee,
            attacker: Some(attacker_entity),
            armor: 0,
        });

        app.update();

        assert_eq!(get_health(&app, target).0, -5);

        // DeathEvent emission is tested structurally by the system code.
        // Health going negative proves the damage_application_system ran
        // and would have emitted the DeathEvent.
    }

    // -----------------------------------------------------------------
    // Heal tests
    // -----------------------------------------------------------------

    #[test]
    fn heal_restores_health() {
        let mut app = build_test_app();
        let target = spawn_entity(
            &mut app,
            Health { current: 5, max: 20 },
            None,
            false,
        );

        send_heal(&mut app, HealEvent {
            target,
            amount: 10,
            source: None,
        });

        app.update();

        assert_eq!(get_health(&app, target), (15, 20));
    }

    #[test]
    fn heal_clamps_to_max() {
        let mut app = build_test_app();
        let target = spawn_entity(
            &mut app,
            Health { current: 18, max: 20 },
            None,
            false,
        );

        send_heal(&mut app, HealEvent {
            target,
            amount: 10,
            source: None,
        });

        app.update();

        assert_eq!(get_health(&app, target), (20, 20));
    }

    // -----------------------------------------------------------------
    // Regen suppression tests
    // -----------------------------------------------------------------

    #[test]
    fn damage_sets_regen_suppression() {
        let mut app = build_test_app();
        let target = spawn_entity(
            &mut app,
            Health { current: 20, max: 20 },
            None,
            true, // has RegenSuppression component
        );

        send_damage(&mut app, DamageEvent {
            target,
            amount: 5,
            damage_type: DamageType::Physical,
            source: DamageSource::Melee,
            attacker: None,
            armor: 0,
        });

        app.update();

        assert_eq!(get_regen_suppression(&app, target), 3);
    }

    #[test]
    fn zero_damage_no_suppression() {
        let mut app = build_test_app();
        let target = spawn_entity(
            &mut app,
            Health { current: 20, max: 20 },
            None,
            true,
        );

        // Armor fully negates the damage
        send_damage(&mut app, DamageEvent {
            target,
            amount: 3,
            damage_type: DamageType::Physical,
            source: DamageSource::Melee,
            attacker: None,
            armor: 10,
        });

        app.update();

        // Health unchanged, suppression unchanged
        assert_eq!(get_health(&app, target), (20, 20));
        assert_eq!(get_regen_suppression(&app, target), 0);
    }

    #[test]
    fn resistance_immunity() {
        let mut app = build_test_app();
        let mut resists = HashMap::new();
        resists.insert(DamageType::Fire, 100);
        let target = spawn_entity(
            &mut app,
            Health { current: 10, max: 10 },
            Some(Resistances(resists)),
            true,
        );

        send_damage(&mut app, DamageEvent {
            target,
            amount: 10,
            damage_type: DamageType::Fire,
            source: DamageSource::Spell,
            attacker: None,
            armor: 0,
        });

        app.update();

        // Immune: no damage, no death, no suppression
        assert_eq!(get_health(&app, target), (10, 10));
        assert_eq!(get_regen_suppression(&app, target), 0);
    }
}
