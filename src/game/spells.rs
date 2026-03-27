use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::game::combat::DamageType;

// --- Spell Effect Types ---

/// How a spell selects its target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpellTarget {
    /// Affects only the caster.
    Castor,
    /// Targets a visible enemy.
    Enemy,
    /// Targets the most-wounded visible ally (not self).
    Ally,
    /// Targets the most-wounded visible ally or self.
    AllyOrSelf,
}

/// A single effect applied when the spell resolves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpellEffect {
    /// Deal damage to the target. `dice` is "NdM" format; optionally scaled by INT bonus.
    Damage { dice: String, int_scaling: bool },
    /// Restore HP to the resolved target. `dice` is "NdM" format; optionally scaled by INT bonus.
    /// Target is determined by SpellTarget (Castor = self-heal, Ally = ally heal, etc.)
    Heal { dice: String, int_scaling: bool },
    /// Damage all entities within `radius` Manhattan distance of the target tile.
    AoeDamage { dice: String, radius: i32, int_scaling: bool },
    /// Hit primary target, then jump to nearby enemies.
    ChainDamage { dice: String, max_jumps: i32, jump_range: i32, int_scaling: bool },
    /// Grant +50% speed for N turns.
    ApplyHaste { duration: u32 },
    /// Inflict -50% speed for N turns.
    ApplySlow { duration: u32 },
    /// Remove mana from target, add to caster.
    DrainMana { amount: i32, int_scaling: bool },
    /// Damage taken from mana instead of HP for N turns.
    SpiritShield { duration: u32 },
    /// Move caster to a tile. range=0 → random, range>0 → controlled.
    Teleport { range: i32 },
    /// Apply +50% damage multiplier for N turns.
    ApplyEnrage { duration: u32 },
    /// Summon allied monsters at adjacent tiles.
    SummonAlly { monster: String, count: u32 },
}

// --- Spell Data ---

/// Full spell definition loaded from `spells.ron`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpellData {
    pub name: String,
    pub mana_cost: i32,
    /// Turns the caster must wait before using this spell again (0 = no cooldown).
    pub cooldown: u32,
    pub description: String,
    pub target: SpellTarget,
    /// Maximum tile range of the spell (0 means self/unlimited for Caster spells).
    pub range: u32,
    pub effects: Vec<SpellEffect>,
    /// The damage type for this spell's damage effects. Defaults to Physical.
    #[serde(default)]
    pub damage_type: DamageType,
}

// --- Registry Asset ---

/// Loaded from `assets/spells.ron`. Holds all spell definitions keyed by ID.
#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct SpellRegistry {
    pub spells: HashMap<String, SpellData>,
}

// --- Helpers ---

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

/// Returns the average roll for a "NdM" dice expression.
/// E.g. "2d6" → 2 * (6+1) / 2 = 7.
pub fn avg_dice(expr: &str) -> i32 {
    let parts: Vec<&str> = expr.split('d').collect();
    if parts.len() != 2 {
        return 0;
    }
    let n = parts[0].parse::<i32>().unwrap_or(1);
    let m = parts[1].parse::<i32>().unwrap_or(6);
    n * (m + 1) / 2
}

// ---------------------------------------------------------------------------
// Spell scoring — pure functions used by AI to evaluate spell value.
// ---------------------------------------------------------------------------

/// A nearby entity's info for AoE/chain scoring. No ECS coupling.
pub struct ScoringNearby {
    pub pos: (i32, i32),
    pub is_enemy: bool,
}

/// Context for scoring a single spell's effects. Fully decoupled from ECS.
pub struct EffectScoringCtx<'a> {
    pub caster_pos: (i32, i32),
    pub caster_hp_pct: f32,
    pub caster_has_haste: bool,
    /// Primary target's stats (already resolved by the caller).
    pub target_hp: i32,
    pub target_hp_max: i32,
    pub target_mana: i32,
    pub target_has_slow: bool,
    pub target_has_haste: bool,
    /// True if the resolved target is the caster itself.
    pub target_is_self: bool,
    pub target_pos: (i32, i32),
    /// Nearby entities for AoE/chain calculations (excludes primary target).
    pub nearby: &'a [ScoringNearby],
}

/// Score a single spell effect for AI decision-making.
/// Returns the raw score contribution (0 = skip/not worthwhile).
pub fn score_effect(effect: &SpellEffect, ctx: &EffectScoringCtx) -> i32 {
    match effect {
        SpellEffect::Damage { dice, .. } => {
            let avg = avg_dice(dice);
            avg.max(1).min(ctx.target_hp)
        }
        SpellEffect::Heal { dice, .. } => {
            let missing = if ctx.target_is_self {
                ((1.0 - ctx.caster_hp_pct) * ctx.target_hp_max as f32) as i32
            } else {
                ctx.target_hp_max - ctx.target_hp
            };
            if missing <= 0 {
                return 0;
            }
            let avg = avg_dice(dice);
            avg.max(1).min(missing) * 2 // Heals weighted 2x
        }
        SpellEffect::AoeDamage { dice, radius, .. } => {
            let avg = avg_dice(dice).max(1);
            let (cx, cy) = ctx.target_pos;

            // Count primary target as 1 enemy hit.
            let mut enemy_count = 1i32;
            let mut ally_count = 0i32;
            for n in ctx.nearby {
                let dist = (n.pos.0 - cx).abs() + (n.pos.1 - cy).abs();
                if dist <= *radius {
                    if n.is_enemy {
                        enemy_count += 1;
                    } else {
                        ally_count += 1;
                    }
                }
            }
            // Check if caster is in the blast.
            let caster_dist = (ctx.caster_pos.0 - cx).abs() + (ctx.caster_pos.1 - cy).abs();
            if caster_dist <= *radius {
                ally_count += 1;
            }

            let score = avg * enemy_count - avg * ally_count;
            score.max(0)
        }
        SpellEffect::ChainDamage { dice, max_jumps, .. } => {
            let avg = avg_dice(dice).max(1).min(ctx.target_hp);
            let jump_damage = 4; // ~1d6 average
            let (tx, ty) = ctx.target_pos;
            let jump_targets: i32 = ctx.nearby.iter()
                .filter(|n| n.is_enemy)
                .filter(|n| {
                    let dist = (n.pos.0 - tx).abs() + (n.pos.1 - ty).abs();
                    dist <= 3
                })
                .count() as i32;
            let actual_jumps = jump_targets.min(*max_jumps);
            avg + jump_damage * actual_jumps
        }
        SpellEffect::ApplyHaste { .. } => {
            if ctx.target_is_self {
                if ctx.caster_has_haste { return 0; }
            } else if ctx.target_has_haste {
                return 0;
            }
            15
        }
        SpellEffect::ApplySlow { .. } => {
            if ctx.target_has_slow { return 0; }
            12
        }
        SpellEffect::DrainMana { amount, .. } => {
            if ctx.target_mana <= 0 { return 0; }
            (*amount).max(0).min(ctx.target_mana)
        }
        SpellEffect::SpiritShield { .. } => {
            if ctx.caster_hp_pct < 0.5 { 10 } else { 3 }
        }
        SpellEffect::Teleport { .. } => {
            0 // Monsters generally shouldn't teleport
        }
        SpellEffect::ApplyEnrage { .. } => {
            20 // Strong self-buff
        }
        SpellEffect::SummonAlly { count, .. } => {
            15 * (*count as i32)
        }
    }
}

/// Normalize a raw spell score by mana cost and cooldown to prevent nova-ing.
///   effective_score = raw / (sqrt(mana_cost) * ln(cooldown + 1))
pub fn normalize_spell_score(raw: i32, mana_cost: i32, cooldown: u32) -> f32 {
    if raw <= 0 {
        return 0.0;
    }
    let mana_weight = (mana_cost as f32).sqrt().max(1.0);
    let cd_weight = ((cooldown as f32) + 1.0).ln().max(1.0);
    raw as f32 / (mana_weight * cd_weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- avg_dice ---

    #[test]
    fn avg_dice_2d6() {
        assert_eq!(avg_dice("2d6"), 7); // 2 * 7/2 = 7
    }

    #[test]
    fn avg_dice_1d8() {
        assert_eq!(avg_dice("1d8"), 4); // 1 * 9/2 = 4 (integer)
    }

    #[test]
    fn avg_dice_3d4() {
        assert_eq!(avg_dice("3d4"), 7); // 3 * 5/2 = 7
    }

    #[test]
    fn avg_dice_invalid() {
        assert_eq!(avg_dice("garbage"), 0);
        assert_eq!(avg_dice(""), 0);
    }

    // --- score_effect: Damage ---

    fn base_ctx() -> EffectScoringCtx<'static> {
        EffectScoringCtx {
            caster_pos: (0, 0),
            caster_hp_pct: 1.0,
            caster_has_haste: false,
            target_hp: 20,
            target_hp_max: 20,
            target_mana: 0,
            target_has_slow: false,
            target_has_haste: false,
            target_is_self: false,
            target_pos: (3, 3),
            nearby: &[],
        }
    }

    #[test]
    fn damage_capped_by_target_hp() {
        let ctx = EffectScoringCtx { target_hp: 3, ..base_ctx() };
        let effect = SpellEffect::Damage { dice: "2d6".to_string(), int_scaling: false };
        assert_eq!(score_effect(&effect, &ctx), 3); // avg 7, capped to 3
    }

    #[test]
    fn damage_uses_avg() {
        let effect = SpellEffect::Damage { dice: "2d6".to_string(), int_scaling: false };
        assert_eq!(score_effect(&effect, &base_ctx()), 7);
    }

    // --- score_effect: Heal ---

    #[test]
    fn heal_scores_zero_at_full_hp() {
        let ctx = EffectScoringCtx { target_hp: 20, target_hp_max: 20, target_is_self: false, ..base_ctx() };
        let effect = SpellEffect::Heal { dice: "2d6".to_string(), int_scaling: false };
        assert_eq!(score_effect(&effect, &ctx), 0);
    }

    #[test]
    fn heal_scores_double_missing_hp() {
        let ctx = EffectScoringCtx { target_hp: 10, target_hp_max: 20, ..base_ctx() };
        let effect = SpellEffect::Heal { dice: "2d6".to_string(), int_scaling: false };
        // avg 7, missing 10, min(7,10)=7, *2 = 14
        assert_eq!(score_effect(&effect, &ctx), 14);
    }

    #[test]
    fn self_heal_uses_caster_hp_pct() {
        let ctx = EffectScoringCtx {
            caster_hp_pct: 0.5,
            target_hp: 10, // doesn't matter for self
            target_hp_max: 20,
            target_is_self: true,
            ..base_ctx()
        };
        let effect = SpellEffect::Heal { dice: "2d6".to_string(), int_scaling: false };
        // missing = (1.0-0.5)*20 = 10, avg 7, min(7,10)=7, *2 = 14
        assert_eq!(score_effect(&effect, &ctx), 14);
    }

    // --- score_effect: AoE ---

    #[test]
    fn aoe_scores_enemies_minus_allies() {
        let nearby = vec![
            ScoringNearby { pos: (3, 4), is_enemy: true },  // dist 1 from (3,3)
            ScoringNearby { pos: (3, 2), is_enemy: true },  // dist 1
            ScoringNearby { pos: (4, 3), is_enemy: false }, // dist 1, ally
        ];
        let ctx = EffectScoringCtx {
            caster_pos: (0, 0), // far from blast
            target_pos: (3, 3),
            nearby: &nearby,
            ..base_ctx()
        };
        let effect = SpellEffect::AoeDamage { dice: "2d6".to_string(), radius: 2, int_scaling: false };
        // avg 7, enemies=3 (primary+2), allies=1, score = 7*3 - 7*1 = 14
        assert_eq!(score_effect(&effect, &ctx), 14);
    }

    #[test]
    fn aoe_penalizes_caster_in_blast() {
        let ctx = EffectScoringCtx {
            caster_pos: (3, 3), // ON the target
            target_pos: (3, 3),
            nearby: &[],
            ..base_ctx()
        };
        let effect = SpellEffect::AoeDamage { dice: "2d6".to_string(), radius: 1, int_scaling: false };
        // avg 7, enemies=1 (primary), allies=1 (caster), score = 7-7 = 0
        assert_eq!(score_effect(&effect, &ctx), 0);
    }

    // --- score_effect: Chain ---

    #[test]
    fn chain_scores_primary_plus_jumps() {
        let nearby = vec![
            ScoringNearby { pos: (4, 3), is_enemy: true }, // dist 1 from target
            ScoringNearby { pos: (5, 3), is_enemy: true }, // dist 2
        ];
        let ctx = EffectScoringCtx {
            target_pos: (3, 3),
            nearby: &nearby,
            ..base_ctx()
        };
        let effect = SpellEffect::ChainDamage {
            dice: "2d6".to_string(), max_jumps: 3, jump_range: 3, int_scaling: false,
        };
        // primary avg 7, 2 jump targets within range 3, actual_jumps=min(2,3)=2, score=7+4*2=15
        assert_eq!(score_effect(&effect, &ctx), 15);
    }

    // --- score_effect: Status effects ---

    #[test]
    fn haste_scores_zero_if_already_hasted() {
        let ctx = EffectScoringCtx { target_has_haste: true, ..base_ctx() };
        assert_eq!(score_effect(&SpellEffect::ApplyHaste { duration: 3 }, &ctx), 0);
    }

    #[test]
    fn haste_scores_15_when_not_hasted() {
        assert_eq!(score_effect(&SpellEffect::ApplyHaste { duration: 3 }, &base_ctx()), 15);
    }

    #[test]
    fn slow_scores_zero_if_already_slowed() {
        let ctx = EffectScoringCtx { target_has_slow: true, ..base_ctx() };
        assert_eq!(score_effect(&SpellEffect::ApplySlow { duration: 3 }, &ctx), 0);
    }

    #[test]
    fn drain_mana_capped_by_target_mana() {
        let ctx = EffectScoringCtx { target_mana: 5, ..base_ctx() };
        assert_eq!(score_effect(&SpellEffect::DrainMana { amount: 10, int_scaling: false }, &ctx), 5);
    }

    #[test]
    fn drain_mana_zero_if_no_mana() {
        assert_eq!(score_effect(&SpellEffect::DrainMana { amount: 10, int_scaling: false }, &base_ctx()), 0);
    }

    #[test]
    fn spirit_shield_higher_when_low_hp() {
        let low_hp = EffectScoringCtx { caster_hp_pct: 0.3, ..base_ctx() };
        let full_hp = base_ctx();
        assert_eq!(score_effect(&SpellEffect::SpiritShield { duration: 3 }, &low_hp), 10);
        assert_eq!(score_effect(&SpellEffect::SpiritShield { duration: 3 }, &full_hp), 3);
    }

    #[test]
    fn teleport_scores_zero() {
        assert_eq!(score_effect(&SpellEffect::Teleport { range: 5 }, &base_ctx()), 0);
    }

    #[test]
    fn enrage_scores_20() {
        assert_eq!(score_effect(&SpellEffect::ApplyEnrage { duration: 3 }, &base_ctx()), 20);
    }

    #[test]
    fn summon_scales_with_count() {
        assert_eq!(score_effect(&SpellEffect::SummonAlly { monster: "rat".to_string(), count: 3 }, &base_ctx()), 45);
    }

    // --- normalize_spell_score ---

    #[test]
    fn normalize_zero_raw_is_zero() {
        assert_eq!(normalize_spell_score(0, 5, 3), 0.0);
    }

    #[test]
    fn normalize_higher_cost_lowers_score() {
        let cheap = normalize_spell_score(10, 1, 0);
        let expensive = normalize_spell_score(10, 16, 0);
        assert!(cheap > expensive, "cheap={cheap} should be > expensive={expensive}");
    }

    #[test]
    fn normalize_higher_cooldown_lowers_score() {
        let no_cd = normalize_spell_score(10, 5, 0);
        let long_cd = normalize_spell_score(10, 5, 10);
        assert!(no_cd > long_cd, "no_cd={no_cd} should be > long_cd={long_cd}");
    }
}
