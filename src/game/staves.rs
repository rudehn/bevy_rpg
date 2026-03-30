//! Brogue-inspired staff system — inventory items with charges that recharge over time.
//!
//! Staves are items with `ItemKind::Staff` that hold a `StaffData` component (effect type +
//! base recharge rate) and a `Rechargeable` component (current charges, recharge timer).
//! The player presses Z → picks a staff → enters targeting → zaps.

use bevy::prelude::*;
use bracket_lib::random::RandomNumberGenerator;
use serde::{Deserialize, Serialize};

use crate::components::{Collider, Inventory, Name, Position, Submerged};
use crate::constants::BASE_ACTION_COST;
use crate::game::actions::{finish_turn, ActionFinishedEvent};
use crate::game::combat::{
    ApplyDamageMessage, DamageSource, DamageType, GameRng, Health,
};
use crate::game::enchantment::Enchantment;
use crate::game::magic::{StatusEffectKind, StatusEffects};
use crate::game::turns::{ProcessingPhase, TurnEndEvent};
use crate::map::map::Map;
use crate::map::tile::is_walkable;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

// =====================================================================
// Staff Effect (data-driven from RON)
// =====================================================================

/// The type of effect a staff produces when zapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum StaffEffect {
    /// Piercing bolt: damage all creatures in a line.
    Lightning,
    /// Apply Poisoned DoT to single target. Halts HP regen.
    Poison,
    /// Teleport the caster along a line, stopping at obstacles.
    Blinking,
}

impl StaffEffect {
    pub fn name(&self) -> &'static str {
        match self {
            StaffEffect::Lightning => "Lightning",
            StaffEffect::Poison => "Poison",
            StaffEffect::Blinking => "Blinking",
        }
    }

    pub fn description(&self, enchant: i32) -> String {
        match self {
            StaffEffect::Lightning => {
                let (low, high) = lightning_damage(enchant);
                format!("Bolt damages all creatures in a line ({}-{} damage)", low, high)
            }
            StaffEffect::Poison => {
                let dur = poison_duration(enchant);
                let dmg = poison_dpt(enchant);
                format!("Poisons target for {} turns ({} dmg/turn, halts regen)", dur, dmg)
            }
            StaffEffect::Blinking => {
                let dist = blink_distance(enchant);
                format!("Teleport up to {} tiles in a direction", dist)
            }
        }
    }

    /// Targeting range for this staff effect.
    pub fn range(&self, enchant: i32) -> i32 {
        match self {
            StaffEffect::Lightning | StaffEffect::Poison => 8,
            StaffEffect::Blinking => blink_distance(enchant),
        }
    }
}

// =====================================================================
// Scaling Formulas
// =====================================================================

/// Lightning damage range (low, high) based on enchantment.
pub fn lightning_damage(enchant: i32) -> (i32, i32) {
    let e = enchant.max(0);
    let low = ((2 + e) * 3 / 4).max(1);
    let high = (4 + 5 * e / 2).max(low);
    (low, high)
}

/// Poison duration in turns.
pub fn poison_duration(enchant: i32) -> u32 {
    (3 + enchant.max(0) * 2) as u32
}

/// Poison damage per turn.
pub fn poison_dpt(enchant: i32) -> i32 {
    (1 + enchant.max(0) / 2).max(1)
}

/// Blink distance in tiles.
pub fn blink_distance(enchant: i32) -> i32 {
    2 + enchant.max(0) * 2
}

/// Max charges for a staff at given enchantment level.
pub fn staff_max_charges(enchant: i32) -> i32 {
    enchant.max(0) + 2
}

/// Recharge rate (turns per charge) for a staff.
pub fn staff_recharge_rate(base_recharge: u32, enchant: i32) -> u32 {
    (base_recharge / (enchant.max(0) as u32 + 1)).max(1)
}

// =====================================================================
// Components
// =====================================================================

/// Identifies the staff effect and its base recharge rate. Placed on item entities.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct StaffData {
    pub effect: StaffEffect,
    pub base_recharge: u32,
}

/// Tracks charges and recharge state. Placed on item entities with staves.
#[derive(Component, Debug, Clone, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Rechargeable {
    pub charges: i32,
    pub max_charges: i32,
    pub recharge_timer: u32,
    pub recharge_rate: u32,
}

impl Rechargeable {
    pub fn new(base_recharge: u32, enchant: i32) -> Self {
        let max = staff_max_charges(enchant);
        let rate = staff_recharge_rate(base_recharge, enchant);
        Self {
            charges: max,
            max_charges: max,
            recharge_timer: rate,
            recharge_rate: rate,
        }
    }

    /// Recalculate max_charges and recharge_rate after enchantment changes.
    pub fn update_from_enchantment(&mut self, base_recharge: u32, enchant: i32) {
        let old_max = self.max_charges;
        self.max_charges = staff_max_charges(enchant);
        self.recharge_rate = staff_recharge_rate(base_recharge, enchant);
        // Gain the extra charge from enchanting
        if self.max_charges > old_max {
            self.charges = (self.charges + (self.max_charges - old_max)).min(self.max_charges);
        }
    }
}

// =====================================================================
// Messages
// =====================================================================

/// Sent when the player zaps a staff at a target.
#[derive(Message, Debug)]
pub struct ZapStaffMessage {
    pub zapper: Entity,
    pub staff_entity: Entity,
    pub target: Entity,
    pub target_pos: Option<(i32, i32)>,
}

// =====================================================================
// Systems
// =====================================================================

/// Recharge all staves in inventories each turn.
pub fn staff_recharge_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    inv_query: Query<&Inventory>,
    mut staff_query: Query<(&mut Rechargeable, &Name)>,
) {
    for _ in turn_end.read() {
        for inv in inv_query.iter() {
            for &item_entity in &inv.items {
                if let Ok((mut rech, _name)) = staff_query.get_mut(item_entity) {
                    if rech.charges < rech.max_charges {
                        rech.recharge_timer = rech.recharge_timer.saturating_sub(1);
                        if rech.recharge_timer == 0 {
                            rech.charges = (rech.charges + 1).min(rech.max_charges);
                            rech.recharge_timer = rech.recharge_rate;
                        }
                    }
                }
            }
        }
    }
}

/// Handle staff zaps: deduct charge, apply effect.
pub fn handle_zap_staff(
    mut commands: Commands,
    mut messages: MessageReader<ZapStaffMessage>,
    mut game_rng: ResMut<GameRng>,
    mut staff_query: Query<(&StaffData, &mut Rechargeable, Option<&Enchantment>)>,
    zapper_query: Query<(&Name, &Position)>,
    target_query: Query<(Entity, &Name, &Position, &Health, Has<Submerged>), Without<Player>>,
    all_positions: Query<(Entity, &Position), With<Health>>,
    mut status_query: Query<&mut StatusEffects>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    collider_query: Query<&Position, With<Collider>>,
    map: Res<Map>,
) {
    for msg in messages.read() {
        let Ok((staff_data, mut rech, enchant)) = staff_query.get_mut(msg.staff_entity) else { continue; };

        if rech.charges <= 0 {
            log_writer.write(GameLogMessage("The staff has no charges!".to_string()));
            finish_turn(&mut commands, &mut finish_writer, msg.zapper, BASE_ACTION_COST);
            continue;
        }

        // Deduct charge and reset recharge timer
        rech.charges -= 1;
        rech.recharge_timer = rech.recharge_rate;

        let enchant_level = enchant.map(|e| e.level).unwrap_or(0);
        let Ok((_zapper_name, zapper_pos)) = zapper_query.get(msg.zapper) else { continue; };

        // Submerged targets cannot be hit by staff zaps (except Blinking which targets self).
        if staff_data.effect != StaffEffect::Blinking {
            if let Ok((_, _, _, _, is_submerged)) = target_query.get(msg.target) {
                if is_submerged {
                    // Refund the charge since the zap failed
                    rech.charges += 1;
                    log_writer.write(GameLogMessage("The target is submerged and cannot be hit!".to_string()));
                    finish_turn(&mut commands, &mut finish_writer, msg.zapper, BASE_ACTION_COST);
                    continue;
                }
            }
        }

        match staff_data.effect {
            StaffEffect::Lightning => {
                // Walk a line from zapper toward target, damage everything in path
                let Ok((_, _, target_pos, _, _)) = target_query.get(msg.target) else { continue; };
                let (low, high) = lightning_damage(enchant_level);

                let dx = (target_pos.x - zapper_pos.x).signum();
                let dy = (target_pos.y - zapper_pos.y).signum();
                if dx == 0 && dy == 0 { continue; }

                let mut x = zapper_pos.x + dx;
                let mut y = zapper_pos.y + dy;
                let mut hit_count = 0;

                for _ in 0..8 {
                    let idx = map.xy_idx(x, y);
                    if idx >= map.tiles.len() || !is_walkable(map.tiles[idx]) {
                        break;
                    }

                    // Check for entities at this position
                    for (entity, pos) in all_positions.iter() {
                        if entity != msg.zapper && pos.x == x && pos.y == y {
                            let dmg = game_rng.0.range(low, high + 1);
                            damage_writer.write(ApplyDamageMessage {
                                attacker: msg.zapper,
                                target: entity,
                                final_damage: dmg,
                                damage_type: DamageType::Lightning,
                                source: DamageSource::Environment,
                            });
                            if let Ok((_, name, _, _, _)) = target_query.get(entity) {
                                log_writer.write(GameLogMessage(format!(
                                    "Lightning strikes {} for {} damage!",
                                    name.0, dmg
                                )));
                            }
                            hit_count += 1;
                        }
                    }

                    x += dx;
                    y += dy;
                }

                if hit_count == 0 {
                    log_writer.write(GameLogMessage("The lightning bolt fizzles.".to_string()));
                }
            }
            StaffEffect::Poison => {
                let Ok((_, target_name, _, _, _)) = target_query.get(msg.target) else { continue; };
                let duration = poison_duration(enchant_level);
                let dmg = poison_dpt(enchant_level);

                if let Ok(mut effects) = status_query.get_mut(msg.target) {
                    effects.add(StatusEffectKind::Poisoned { damage_per_turn: dmg }, duration);
                }
                log_writer.write(GameLogMessage(format!(
                    "{} is poisoned for {} turns!",
                    target_name.0, duration
                )));
            }
            StaffEffect::Blinking => {
                let max_dist = blink_distance(enchant_level);
                let target_pos = msg.target_pos.unwrap_or((zapper_pos.x, zapper_pos.y));

                let dx = (target_pos.0 - zapper_pos.x).signum();
                let dy = (target_pos.1 - zapper_pos.y).signum();
                if dx == 0 && dy == 0 { continue; }

                let occupied: std::collections::HashSet<(i32, i32)> = collider_query
                    .iter()
                    .filter(|p| !(p.x == zapper_pos.x && p.y == zapper_pos.y))
                    .map(|p| (p.x, p.y))
                    .collect();

                let mut final_x = zapper_pos.x;
                let mut final_y = zapper_pos.y;
                for _ in 0..max_dist {
                    let nx = final_x + dx;
                    let ny = final_y + dy;
                    let idx = map.xy_idx(nx, ny);
                    if idx >= map.tiles.len() || !is_walkable(map.tiles[idx]) || occupied.contains(&(nx, ny)) {
                        break;
                    }
                    final_x = nx;
                    final_y = ny;
                }

                if final_x != zapper_pos.x || final_y != zapper_pos.y {
                    commands.entity(msg.zapper).insert(Position { x: final_x, y: final_y });
                    log_writer.write(GameLogMessage(format!(
                        "You blink {} tiles!",
                        (final_x - zapper_pos.x).abs() + (final_y - zapper_pos.y).abs()
                    )));
                } else {
                    log_writer.write(GameLogMessage("You can't blink there.".to_string()));
                }
            }
        }

        finish_turn(&mut commands, &mut finish_writer, msg.zapper, BASE_ACTION_COST);
    }
}

// =====================================================================
// Dice Helpers
// =====================================================================

/// Parse a dice expression in "NdM" format and roll it, returning the total.
/// Ignores malformed input (returns 0).
pub fn roll_dice_expr(rng: &mut bracket_lib::prelude::RandomNumberGenerator, expr: &str) -> i32 {
    let parts: Vec<&str> = expr.split('d').collect();
    if parts.len() != 2 {
        return 0;
    }
    let n = parts[0].parse::<i32>().unwrap_or(1);
    let m = parts[1].parse::<i32>().unwrap_or(6);
    rng.roll_dice(n, m)
}

// =====================================================================
// Monster Abilities (simplified spell replacement)
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub enum MonsterAbilityKind {
    /// Deal damage to target.
    Bolt { dice: String, damage_type: DamageType },
    /// Heal self or ally.
    Heal { dice: String },
    /// Apply status effect to target.
    ApplyStatus { effect: StatusEffectKind, duration: u32 },
    /// Buff self with status.
    SelfBuff { effect: StatusEffectKind, duration: u32 },
    /// Summon allied monsters.
    Summon { monster: String, count: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct MonsterAbilityDef {
    pub kind: MonsterAbilityKind,
    pub cooldown: u32,
    pub current_cooldown: u32,
    pub range: u32,
    pub name: String,
}

#[derive(Component, Debug, Clone, Default, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct MonsterAbilities(pub Vec<MonsterAbilityDef>);

/// Tick monster ability cooldowns each turn.
pub fn tick_monster_abilities_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut query: Query<&mut MonsterAbilities>,
) {
    for _ in turn_end.read() {
        for mut abilities in query.iter_mut() {
            for ability in abilities.0.iter_mut() {
                if ability.current_cooldown > 0 {
                    ability.current_cooldown -= 1;
                }
            }
        }
    }
}

// =====================================================================
// Plugin
// =====================================================================

pub struct StavesPlugin;

impl Plugin for StavesPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<StaffData>()
            .register_type::<Rechargeable>()
            .register_type::<MonsterAbilities>()
            .add_message::<ZapStaffMessage>()
            .add_systems(
                Update,
                (
                    handle_zap_staff.in_set(ProcessingPhase::ResolveActions),
                    staff_recharge_system,
                    tick_monster_abilities_system,
                )
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightning_damage_scales() {
        let (low0, high0) = lightning_damage(0);
        let (low3, high3) = lightning_damage(3);
        assert!(low3 > low0);
        assert!(high3 > high0);
    }

    #[test]
    fn poison_scales() {
        assert_eq!(poison_duration(0), 3);
        assert_eq!(poison_duration(3), 9);
        assert_eq!(poison_dpt(0), 1);
        assert_eq!(poison_dpt(4), 3);
    }

    #[test]
    fn blink_scales() {
        assert_eq!(blink_distance(0), 2);
        assert_eq!(blink_distance(3), 8);
    }

    #[test]
    fn max_charges_scales() {
        assert_eq!(staff_max_charges(0), 2);
        assert_eq!(staff_max_charges(3), 5);
    }

    #[test]
    fn recharge_rate_scales() {
        assert_eq!(staff_recharge_rate(250, 0), 250);
        assert_eq!(staff_recharge_rate(250, 3), 62);
        assert_eq!(staff_recharge_rate(400, 3), 100);
    }

    #[test]
    fn rechargeable_update() {
        let mut r = Rechargeable::new(250, 0);
        assert_eq!(r.max_charges, 2);
        assert_eq!(r.recharge_rate, 250);

        r.update_from_enchantment(250, 3);
        assert_eq!(r.max_charges, 5);
        assert_eq!(r.recharge_rate, 62);
        assert_eq!(r.charges, 5); // gained 3 charges from enchanting
    }
}
