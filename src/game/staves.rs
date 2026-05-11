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
use crate::game::actions::{finish_turn, ActionFinishedEvent, ActionKind};
use crate::game::combat::{
    DamageEvent, DamageSource, DamageType, GameRng, HealEvent, Health,
};
use crate::game::enchantment::Enchantment;
use crate::game::magic::{GameStatusEffectsExt, StatusEffectKind, StatusEffects};
use crate::game::turns::TurnEndEvent;
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
    /// Deal fire damage in a 3x3 area centered on target.
    Fire,
    /// Heal the user (self-only, no target selection).
    Healing,
    /// Knockback target 3 tiles + deal physical damage.
    Force,
}

impl StaffEffect {
    pub fn name(&self) -> &'static str {
        match self {
            StaffEffect::Lightning => "Lightning",
            StaffEffect::Poison => "Poison",
            StaffEffect::Blinking => "Blinking",
            StaffEffect::Fire => "Fire",
            StaffEffect::Healing => "Healing",
            StaffEffect::Force => "Force",
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
            StaffEffect::Fire => {
                let (low, high) = fire_damage(enchant);
                format!("Fire blast in 3x3 area ({}-{} damage)", low, high)
            }
            StaffEffect::Healing => {
                let (low, high) = healing_amount(enchant);
                format!("Heal self for {}-{} HP", low, high)
            }
            StaffEffect::Force => {
                let (low, high) = force_damage(enchant);
                format!("Knockback 3 tiles + {}-{} damage", low, high)
            }
        }
    }

    /// Targeting range for this staff effect.
    pub fn range(&self, enchant: i32) -> i32 {
        match self {
            StaffEffect::Lightning | StaffEffect::Poison => 8,
            StaffEffect::Blinking => blink_distance(enchant),
            StaffEffect::Fire | StaffEffect::Force => 6,
            StaffEffect::Healing => 0, // self-only
        }
    }

    /// Whether this staff effect requires target selection.
    pub fn needs_target(&self) -> bool {
        match self {
            StaffEffect::Healing => false,
            _ => true,
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

/// Fire staff damage range (low, high) — 2d6 base, scales with enchantment.
pub fn fire_damage(enchant: i32) -> (i32, i32) {
    let e = enchant.max(0);
    let low = (2 + e).max(2);
    let high = (12 + e * 3).max(low);
    (low, high)
}

/// Healing staff amount range (low, high) — 3d6 base, scales with enchantment.
pub fn healing_amount(enchant: i32) -> (i32, i32) {
    let e = enchant.max(0);
    let low = (3 + e * 2).max(3);
    let high = (18 + e * 4).max(low);
    (low, high)
}

/// Force staff damage range (low, high) — 1d6 base, scales with enchantment.
pub fn force_damage(enchant: i32) -> (i32, i32) {
    let e = enchant.max(0);
    let low = (1 + e / 2).max(1);
    let high = (6 + e * 2).max(low);
    (low, high)
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
// Staff Zap Initiation
// =====================================================================

/// Result of attempting to begin a staff zap.
pub enum ZapResult {
    /// Staff has no charges.
    NoCharges,
    /// Self-targeting staff (e.g. Healing) — action is ready, no targeting needed.
    SelfTarget {
        action: crate::game::actions::Action,
    },
    /// Needs targeting — caller should set InGameState::Targeting.
    NeedsTargeting,
}

/// Set up targeting context for a staff zap and return what the caller should do.
/// Caller is responsible for state transitions based on the result.
pub fn begin_staff_zap(
    staff_entity: Entity,
    player_entity: Entity,
    staff_data: &StaffData,
    rech: &Rechargeable,
    enchant: Option<&crate::game::enchantment::Enchantment>,
    targeting_context: &mut crate::game::targeting::TargetingContext,
) -> ZapResult {
    if rech.charges <= 0 {
        return ZapResult::NoCharges;
    }

    let enchant_level = enchant.map(|e| e.level).unwrap_or(0);
    let range = staff_data.effect.range(enchant_level);

    // Self-targeting staves skip targeting screen
    if !staff_data.effect.needs_target() {
        return ZapResult::SelfTarget {
            action: crate::game::actions::Action::ZapStaff {
                staff_entity,
                target: player_entity,
                target_pos: None,
            },
        };
    }

    // Set targeting mode based on staff effect
    match staff_data.effect {
        StaffEffect::Blinking | StaffEffect::Fire => {
            targeting_context.mode = crate::game::targeting::TargetingMode::Tile {
                slot: 0,
                range,
                radius: if staff_data.effect == StaffEffect::Fire { 1 } else { 0 },
            };
        }
        _ => {
            targeting_context.mode = crate::game::targeting::TargetingMode::Staff {
                staff_entity,
            };
        }
    }
    targeting_context.staff_entity = Some(staff_entity);
    ZapResult::NeedsTargeting
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
    mut damage_writer: MessageWriter<DamageEvent>,
    mut heal_writer: MessageWriter<HealEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut tile_writers: crate::map::tile::TileMutationWriters,
    collider_query: Query<&Position, With<Collider>>,
    map: Res<Map>,
) {
    for msg in messages.read() {
        let Ok((staff_data, mut rech, enchant)) = staff_query.get_mut(msg.staff_entity) else { continue; };

        if rech.charges <= 0 {
            log_writer.write(GameLogMessage("The staff has no charges!".to_string()));
            finish_turn(&mut commands, &mut finish_writer, msg.zapper, BASE_ACTION_COST, ActionKind::Attack);
            continue;
        }

        // Deduct charge and reset recharge timer
        rech.charges -= 1;
        rech.recharge_timer = rech.recharge_rate;

        let enchant_level = enchant.map(|e| e.level).unwrap_or(0);
        let Ok((_zapper_name, zapper_pos)) = zapper_query.get(msg.zapper) else { continue; };

        // Submerged targets cannot be hit by staff zaps (except self-targeting effects).
        if staff_data.effect != StaffEffect::Blinking && staff_data.effect != StaffEffect::Healing {
            if let Ok((_, _, _, _, is_submerged)) = target_query.get(msg.target) {
                if is_submerged {
                    // Refund the charge since the zap failed
                    rech.charges += 1;
                    log_writer.write(GameLogMessage("The target is submerged and cannot be hit!".to_string()));
                    finish_turn(&mut commands, &mut finish_writer, msg.zapper, BASE_ACTION_COST, ActionKind::Attack);
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
                            damage_writer.write(DamageEvent {
                                attacker: Some(msg.zapper),
                                target: entity,
                                amount: dmg,
                                damage_type: DamageType::Lightning,
                                source: DamageSource::Environment,
                                armor: 0,
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
                    effects.add_effect_with_magnitude(
                        StatusEffectKind::Poisoned,
                        duration,
                        dmg,
                        Some(msg.zapper),
                    );
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
            StaffEffect::Fire => {
                // AoE fire damage in 3x3 area centered on target tile position.
                // Uses target_pos (tile coordinates) since fire can target ground.
                let (center_x, center_y) = if let Some((tx, ty)) = msg.target_pos {
                    (tx, ty)
                } else if let Ok((_, _, tp, _, _)) = target_query.get(msg.target) {
                    (tp.x, tp.y)
                } else {
                    continue;
                };
                let (low, high) = fire_damage(enchant_level);
                let mut hit_count = 0;

                for (entity, pos) in all_positions.iter() {
                    let dx = (pos.x - center_x).abs();
                    let dy = (pos.y - center_y).abs();
                    if dx <= 1 && dy <= 1 {
                        let dmg = game_rng.0.range(low, high + 1);
                        damage_writer.write(DamageEvent {
                            attacker: Some(msg.zapper),
                            target: entity,
                            amount: dmg,
                            damage_type: DamageType::Fire,
                            source: DamageSource::Environment,
                            armor: 0,
                        });
                        if let Ok((_, name, _, _, _)) = target_query.get(entity) {
                            log_writer.write(GameLogMessage(format!(
                                "{} is engulfed in flames for {} damage!",
                                name.0, dmg
                            )));
                        }
                        // Apply burning status for 3 turns
                        if let Ok(mut effects) = status_query.get_mut(entity) {
                            effects.add_effect_with_magnitude(
                                StatusEffectKind::Burning,
                                3,
                                2,
                                Some(msg.zapper),
                            );
                        }
                        hit_count += 1;
                    }
                }

                if hit_count == 0 {
                    log_writer.write(GameLogMessage("The fire blast hits nothing.".to_string()));
                }

                // Ignite flammable tiles in the 3x3 area
                for dy in -1..=1i32 {
                    for dx in -1..=1i32 {
                        let tx = center_x + dx;
                        let ty = center_y + dy;
                        crate::game::fire::ignite_tile_at(
                            &mut commands, tx, ty, &map,
                            &mut tile_writers.fire_tiles,
                            &mut tile_writers.decoration,
                            &mut tile_writers.terrain,
                            &mut tile_writers.light_sources,
                            &mut tile_writers.gas_tiles,
                        );
                    }
                }
            }
            StaffEffect::Healing => {
                // Self-heal, no target needed
                let (low, high) = healing_amount(enchant_level);
                let heal = game_rng.0.range(low, high + 1);

                heal_writer.write(HealEvent {
                    target: msg.zapper,
                    amount: heal,
                    source: None,
                });
                log_writer.write(GameLogMessage(format!(
                    "The staff glows warmly. You recover {} HP.",
                    heal
                )));
            }
            StaffEffect::Force => {
                // Knockback target 3 tiles + physical damage
                let Ok((_, target_name, target_pos, _, _)) = target_query.get(msg.target) else { continue; };
                let (low, high) = force_damage(enchant_level);
                let dmg = game_rng.0.range(low, high + 1);

                // Apply damage
                damage_writer.write(DamageEvent {
                    attacker: Some(msg.zapper),
                    target: msg.target,
                    amount: dmg,
                    damage_type: DamageType::Physical,
                    source: DamageSource::Environment,
                    armor: 0,
                });

                // Calculate knockback direction (away from zapper)
                let kb_dx = (target_pos.x - zapper_pos.x).signum();
                let kb_dy = (target_pos.y - zapper_pos.y).signum();

                if kb_dx != 0 || kb_dy != 0 {
                    let occupied: std::collections::HashSet<(i32, i32)> = collider_query
                        .iter()
                        .filter(|p| !(p.x == target_pos.x && p.y == target_pos.y))
                        .map(|p| (p.x, p.y))
                        .collect();

                    let mut final_x = target_pos.x;
                    let mut final_y = target_pos.y;
                    for _ in 0..3 {
                        let nx = final_x + kb_dx;
                        let ny = final_y + kb_dy;
                        let idx = map.xy_idx(nx, ny);
                        if idx >= map.tiles.len() || !is_walkable(map.tiles[idx]) || occupied.contains(&(nx, ny)) {
                            break;
                        }
                        final_x = nx;
                        final_y = ny;
                    }

                    if final_x != target_pos.x || final_y != target_pos.y {
                        commands.entity(msg.target).insert(Position { x: final_x, y: final_y });
                        let dist = (final_x - target_pos.x).abs() + (final_y - target_pos.y).abs();
                        log_writer.write(GameLogMessage(format!(
                            "A blast of force hits {} for {} damage and knocks it back {} tiles!",
                            target_name.0, dmg, dist
                        )));
                    } else {
                        log_writer.write(GameLogMessage(format!(
                            "A blast of force hits {} for {} damage!",
                            target_name.0, dmg
                        )));
                    }
                } else {
                    log_writer.write(GameLogMessage(format!(
                        "A blast of force hits {} for {} damage!",
                        target_name.0, dmg
                    )));
                }
            }
        }

        finish_turn(&mut commands, &mut finish_writer, msg.zapper, BASE_ACTION_COST, ActionKind::Attack);
    }
}

// =====================================================================
// Dice Helpers
// =====================================================================

/// Parse a dice expression and roll it, returning the total.
///
/// Delegates to [`roguelike_engine::dice::roll_dice_string`], which
/// accepts the full `"NdM+B"` notation via bracket-lib's parser
/// (previously this helper only handled `"NdM"`). Malformed input
/// falls back to `1` — note this is a behavior change from the old
/// `0` fallback, but no current caller distinguishes the two.
pub fn roll_dice_expr(rng: &mut bracket_lib::prelude::RandomNumberGenerator, expr: &str) -> i32 {
    roguelike_engine::dice::roll_dice_string(rng, expr)
}

// =====================================================================
// Monster Abilities (simplified spell replacement)
// =====================================================================

// Note: does not derive Reflect because it contains `StatusEffectKind`,
// which is engine-owned and does not implement `Reflect`. Save/load goes
// through Serde, which is what's actually required.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Summon a creature from a weighted list, respecting a maximum active count.
    SummonCapped {
        weights: Vec<(String, u32)>,
        max_summons: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonsterAbilityDef {
    pub kind: MonsterAbilityKind,
    pub cooldown: u32,
    pub current_cooldown: u32,
    pub range: u32,
    pub name: String,
}

#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
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
        use crate::game::turns::ProcessingPhase;
        app.register_type::<StaffData>()
            .register_type::<Rechargeable>()
            // MonsterAbilities no longer registers for reflection because it
            // transitively contains the engine's `StatusEffectKind`, which
            // does not derive `Reflect`. Save/load is Serde-based and works
            // without reflection.
            .add_message::<ZapStaffMessage>()
            .add_systems(
                Update,
                handle_zap_staff.in_set(ProcessingPhase::ResolveActions),
            )
            .add_systems(
                Update,
                (
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
