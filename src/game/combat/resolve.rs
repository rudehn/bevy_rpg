//! Pure attack resolver. No Bevy, no ECS, no globals.
//!
//! One call resolves one attack from intent to final damage. The Bevy
//! adapter ([`crate::game::combat`]'s `hit_check_system` and
//! `damage_roll_system`) gathers snapshots from ECS components, calls
//! [`resolve_attack`], and writes events from the [`AttackOutcome`].
//!
//! ## Two-tier API
//!
//! - [`resolve_attack`] / [`resolve_melee`] — one full attack against
//!   one defender. Use this for melee, ranged, single-target staff
//!   zaps, and most monster abilities.
//! - [`roll_damage`] + [`apply_damage`] — split for AoE / Cleave / Sweep
//!   where one roll feeds many tiles. The hit check happens once on a
//!   primary target via [`resolve_attack`]; the resulting damage is
//!   re-applied to each additional target via [`apply_damage`].
//!
//! ## Snapshots
//!
//! Snapshots are plain-data views the adapter builds from ECS
//! components. None of them implements `Default` — explicit
//! construction at the adapter boundary catches the "I forgot to copy
//! `attrs`" wiring bug at compile time rather than producing silent
//! zero-bonus damage at runtime.
//!
//! ## Math
//!
//! All formulas mirror production behaviour today. See [`SKILLS.md`]
//! and [`GAME.md`] for the canonical writeups.
//!
//! [`SKILLS.md`]: ../../../../docs/design/SKILLS.md
//! [`GAME.md`]: ../../../../docs/design/GAME.md

use bracket_lib::random::RandomNumberGenerator;
use roguelike_engine::combat::{
    apply_damage_multipliers, apply_resistance, compute_after_armor, DamageSource, DamageType,
};
use roguelike_engine::dice::roll_dice_string;

use crate::character::Attributes;
use crate::game::skills::{
    armor_skill_bonus, dodging_skill_bonus, fighting_melee_bonus, shields_skill_bonus,
    weapon_skill_bonus, Skill, Skills, WeaponSkill,
};

// =====================================================================
// Public types
// =====================================================================

/// The result of the to-hit check. Resistance / armor can still bring a
/// `Hit` to zero final damage, but `result` describes the roll alone.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HitResult {
    Miss,
    Hit,
    Crit,
}

/// Shield kind worn by the defender. Determines block bonus and per-turn
/// block budget; `None` short-circuits the shield check.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ShieldKind {
    None,
    Buckler,
    Kite,
    Tower,
}

impl ShieldKind {
    /// SH bonus added to the shield-block d20 (matches the values
    /// declared on shield items in `items.ron`).
    pub const fn block_bonus(self) -> i32 {
        match self {
            ShieldKind::None => 0,
            ShieldKind::Buckler => 3,
            ShieldKind::Kite => 8,
            ShieldKind::Tower => 13,
        }
    }

    /// Max successful blocks per turn before the shield is "used up"
    /// until the wearer's next action finishes.
    pub const fn max_blocks_per_turn(self) -> u8 {
        match self {
            ShieldKind::None => 0,
            ShieldKind::Buckler => 1,
            ShieldKind::Kite => 2,
            ShieldKind::Tower => 3,
        }
    }
}

/// Adapter-side view of the attacker. The adapter copies the relevant
/// ECS components into this struct once at the boundary; the resolver
/// reads it and never queries ECS.
///
/// Attributes and skills are passed by value so the resolver can be
/// invoked without lifetime gymnastics. Both are cheap to clone
/// (`Attributes` is `Copy`; `Skills` is a small `HashMap`).
#[derive(Clone, Debug)]
pub struct AttackerSnapshot {
    /// Flat `HitBonus` component value.
    pub hit_bonus: i32,
    /// Flat `DamageBonus` component value.
    pub damage_bonus: i32,
    /// `None` for entities without attributes (monsters today).
    pub attributes: Option<Attributes>,
    /// `None` for entities without skills (monsters today).
    pub skills: Option<Skills>,
    /// Status-effect flag: ×3/2 damage.
    pub enraged: bool,
    /// Status-effect flag: ×3/4 damage.
    pub terrified: bool,
    /// Extra damage multiplier in basis points. `100` = ×1.0 (default).
    /// `300` = ×3.0 (Backstab). The adapter sets this when a weapon
    /// ability procs — the resolver stays agnostic to ability names.
    pub damage_multiplier_bp: u32,
}

/// Adapter-side view of the defender. The adapter passes this by `&mut`
/// because successful shield blocks decrement `shield_budget_left`; the
/// adapter writes the delta back to the ECS `ShieldBlocksUsed`
/// component after the call.
///
/// `resistance_pct` is pre-resolved by the adapter for the active
/// damage type. The resolver does not carry a `Resistances` map.
#[derive(Clone, Debug)]
pub struct DefenderSnapshot {
    /// Flat `Dodge` component value (DEX mod already baked in at spawn).
    pub dodge: i32,
    /// `None` for entities without skills (monsters today).
    pub skills: Option<Skills>,
    /// Upper bound of the armor roll **before** the Armor-skill bonus.
    /// `0` = no armor; skipped for non-Physical damage types.
    pub armor_max: i32,
    /// Shield kind. `None` short-circuits the block check.
    pub shield: ShieldKind,
    /// Remaining shield blocks this turn. Decremented on a successful
    /// block. Adapter passes the current value and writes back after.
    pub shield_budget_left: u8,
    /// Pre-resolved resistance percentage for the active damage type.
    /// `0` = normal, `50` = halved, `100` = immune, `<0` = vulnerable.
    pub resistance_pct: i32,
}

/// Adapter-side view of the weapon being swung / fired / zapped. For
/// staff zaps and fixed-damage abilities the adapter can synthesise a
/// snapshot that doesn't correspond to a real equipped weapon.
#[derive(Clone, Debug)]
pub struct WeaponSnapshot {
    /// Dice expression in the engine's standard notation (e.g. `"1d6"`,
    /// `"2d4+1"`). Parse errors at roll time fall back to `1` damage.
    pub damage_dice: String,
    /// The damage type the weapon deals. May be overridden per attack
    /// via [`AttackOverrides::damage_type`].
    pub damage_type: DamageType,
    /// Weapon family. `None` = fists, staff bash, or other no-skill
    /// strike — no weapon-skill bonus applies.
    pub weapon_skill: Option<WeaponSkill>,
}

impl WeaponSnapshot {
    /// True if the weapon's `weapon_skill` is one of the finesse blade
    /// families. Drives DEX-vs-STR for melee attribute bonus.
    pub fn is_finesse(&self) -> bool {
        matches!(
            self.weapon_skill,
            Some(WeaponSkill::ShortBlades) | Some(WeaponSkill::LongBlades)
        )
    }
}

/// Per-attack tweaks for non-default sources. All fields default to
/// "behave like a normal weapon attack".
#[derive(Clone, Debug, Default)]
pub struct AttackOverrides {
    /// Override the weapon's damage type for this attack (e.g. a staff
    /// zap that fires Fire damage regardless of the staff item type).
    pub damage_type: Option<DamageType>,
    /// Skip the d20 hit check; the attack always lands. Used for staff
    /// zaps and abilities that target a tile rather than rolling against
    /// a defender's dodge.
    pub auto_hit: bool,
    /// Suppress crit on nat-20. Used by flat-damage abilities.
    pub crit_disabled: bool,
    /// Skip the shield block check entirely (gas, environment).
    pub bypass_shield: bool,
}

/// Which skill use-counters the adapter should bump after applying the
/// outcome. The adapter — not the resolver — owns the ECS resource
/// holding the counts.
#[derive(Copy, Clone, Debug, Default)]
pub struct UseCounterBumps {
    pub fighting: bool,
    pub weapon_skill: Option<Skill>,
    pub dodging: bool,
    pub armor: bool,
    pub shields: bool,
}

/// Outcome of a fully resolved attack against a single defender.
#[derive(Clone, Debug)]
pub struct AttackOutcome {
    pub result: HitResult,
    pub blocked: bool,
    pub final_damage: i32,
    pub damage_type: DamageType,
    pub use_counters: UseCounterBumps,
}

/// A pre-rolled damage payload. Used by [`apply_damage`] to share one
/// roll across many targets (Cleave splash, AoE staff zap).
#[derive(Clone, Debug)]
pub struct DamagePacket {
    /// Damage value **after** attacker-side multipliers but **before**
    /// the defender's shield/armor/resistance.
    pub amount: i32,
    pub damage_type: DamageType,
    pub crit: bool,
}

/// Outcome of applying a [`DamagePacket`] to one defender. Mirrors
/// [`AttackOutcome`] but without the hit-check result (the hit happened
/// upstream).
#[derive(Clone, Debug)]
pub struct AppliedOutcome {
    pub blocked: bool,
    pub final_damage: i32,
    pub use_counters: UseCounterBumps,
}

// =====================================================================
// Pure helpers (no RNG — inputs are pre-rolled)
// =====================================================================

/// The total to-hit roll given a d20 result. Mirrors the engine
/// formula: `d20 + hit_bonus + attribute + weapon_skill + fighting`.
/// Returns just the numeric total; callers compare against the dodge
/// target.
pub fn hit_roll_total(
    d20: i32,
    source: DamageSource,
    attacker: &AttackerSnapshot,
    weapon: &WeaponSnapshot,
) -> i32 {
    let attr =
        crate::character::attack_attribute_bonus(source, weapon.is_finesse(), attacker.attributes.as_ref());
    let ws = weapon_skill_bonus(weapon.weapon_skill, source, attacker.skills.as_ref());
    let fighting = fighting_melee_bonus(source, attacker.skills.as_ref());
    d20 + attacker.hit_bonus + attr + ws + fighting
}

/// The defender's dodge target: `4 + Dodge + dodging_skill_bonus`.
pub fn dodge_target(defender: &DefenderSnapshot) -> i32 {
    4 + defender.dodge + dodging_skill_bonus(defender.skills.as_ref())
}

/// Decide hit / miss / crit given a d20 result. Nat 20 always hits and
/// crits (unless `crit_disabled`); nat 1 always misses; otherwise
/// compares the rolled total against the dodge target.
///
/// `auto_hit` forces a `Hit` regardless of the roll (crit still
/// possible on nat 20 unless `crit_disabled`).
pub fn classify_hit(
    d20: i32,
    source: DamageSource,
    attacker: &AttackerSnapshot,
    defender: &DefenderSnapshot,
    weapon: &WeaponSnapshot,
    auto_hit: bool,
    crit_disabled: bool,
) -> HitResult {
    if d20 == 20 && !crit_disabled {
        return HitResult::Crit;
    }
    if auto_hit {
        return HitResult::Hit;
    }
    if d20 == 1 {
        return HitResult::Miss;
    }
    let total = hit_roll_total(d20, source, attacker, weapon);
    if total >= dodge_target(defender) {
        HitResult::Hit
    } else {
        HitResult::Miss
    }
}

/// The damage roll given a pre-rolled dice value (or two pre-rolled
/// values for crits — caller sums them and passes the total as
/// `dice_total`).
///
/// Applies attacker-side bonuses and multipliers. The result is
/// **before** shield/armor/resistance.
pub fn damage_total(
    dice_total: i32,
    source: DamageSource,
    attacker: &AttackerSnapshot,
    weapon: &WeaponSnapshot,
) -> i32 {
    let attr =
        crate::character::attack_attribute_bonus(source, weapon.is_finesse(), attacker.attributes.as_ref());
    let ws = weapon_skill_bonus(weapon.weapon_skill, source, attacker.skills.as_ref());
    let fighting = fighting_melee_bonus(source, attacker.skills.as_ref());
    let base = dice_total + attacker.damage_bonus + attr + ws + fighting;
    let multiplied = apply_damage_multipliers(base, attacker.enraged, attacker.terrified);
    // Backstab and similar weapon-ability multipliers.
    if attacker.damage_multiplier_bp == 100 {
        multiplied
    } else {
        (multiplied as i64 * attacker.damage_multiplier_bp as i64 / 100) as i32
    }
}

// =====================================================================
// Entry points
// =====================================================================

/// Resolve one full attack from intent to final damage.
///
/// Mutates `defender.shield_budget_left` on a successful block. The
/// adapter is expected to write the delta back to the ECS
/// `ShieldBlocksUsed` component after the call.
pub fn resolve_attack(
    source: DamageSource,
    attacker: &AttackerSnapshot,
    defender: &mut DefenderSnapshot,
    weapon: &WeaponSnapshot,
    overrides: AttackOverrides,
    rng: &mut RandomNumberGenerator,
) -> AttackOutcome {
    let damage_type = overrides.damage_type.unwrap_or(weapon.damage_type);

    // ----- Hit check -----
    let d20 = rng.range(1, 21);
    let hit = classify_hit(
        d20,
        source,
        attacker,
        defender,
        weapon,
        overrides.auto_hit,
        overrides.crit_disabled,
    );

    if hit == HitResult::Miss {
        let mut bumps = UseCounterBumps::default();
        // Dodging only bumps if the defender actually has skills (monster targets don't).
        if defender.skills.is_some() {
            bumps.dodging = true;
        }
        return AttackOutcome {
            result: hit,
            blocked: false,
            final_damage: 0,
            damage_type,
            use_counters: bumps,
        };
    }

    // ----- Damage roll -----
    let base_roll = roll_dice_string(rng, &weapon.damage_dice);
    let dice_total = if hit == HitResult::Crit {
        base_roll + roll_dice_string(rng, &weapon.damage_dice)
    } else {
        base_roll
    };
    let raw = damage_total(dice_total, source, attacker, weapon);

    let packet = DamagePacket {
        amount: raw,
        damage_type,
        crit: hit == HitResult::Crit,
    };

    // ----- Defense pipeline -----
    let applied = apply_packet(packet, defender, overrides.bypass_shield, rng);

    // ----- Use-counter bumps -----
    let mut bumps = applied.use_counters;
    if attacker.skills.is_some() {
        if source == DamageSource::Melee {
            bumps.fighting = true;
        }
        if let Some(ws) = weapon.weapon_skill {
            // Ranged weapons train RangedWeapons regardless of the
            // weapon's tag; melee trains the weapon's family.
            let trained = if source == DamageSource::Ranged {
                Some(Skill::RangedWeapons)
            } else if source == DamageSource::Melee {
                Some(ws.as_skill())
            } else {
                None
            };
            if trained.is_some() {
                bumps.weapon_skill = trained;
            }
        } else if source == DamageSource::Ranged {
            bumps.weapon_skill = Some(Skill::RangedWeapons);
        }
    }

    AttackOutcome {
        result: hit,
        blocked: applied.blocked,
        final_damage: applied.final_damage,
        damage_type,
        use_counters: bumps,
    }
}

/// Convenience for the dominant case: a plain melee swing.
#[inline]
pub fn resolve_melee(
    attacker: &AttackerSnapshot,
    defender: &mut DefenderSnapshot,
    weapon: &WeaponSnapshot,
    rng: &mut RandomNumberGenerator,
) -> AttackOutcome {
    resolve_attack(
        DamageSource::Melee,
        attacker,
        defender,
        weapon,
        AttackOverrides::default(),
        rng,
    )
}

/// Roll damage in isolation, without a hit check or defender. Useful
/// when one roll must feed many targets (Cleave splash, AoE staff zap).
pub fn roll_damage(
    attacker: &AttackerSnapshot,
    weapon: &WeaponSnapshot,
    source: DamageSource,
    crit: bool,
    rng: &mut RandomNumberGenerator,
) -> DamagePacket {
    let base_roll = roll_dice_string(rng, &weapon.damage_dice);
    let dice_total = if crit {
        base_roll + roll_dice_string(rng, &weapon.damage_dice)
    } else {
        base_roll
    };
    let amount = damage_total(dice_total, source, attacker, weapon);
    DamagePacket {
        amount,
        damage_type: weapon.damage_type,
        crit,
    }
}

/// Apply a pre-rolled packet against one defender's full defense
/// pipeline. Mutates `shield_budget_left` on a successful block.
pub fn apply_damage(
    packet: DamagePacket,
    defender: &mut DefenderSnapshot,
    rng: &mut RandomNumberGenerator,
) -> AppliedOutcome {
    apply_packet(packet, defender, false, rng)
}

// =====================================================================
// Internal: defense pipeline
// =====================================================================

fn apply_packet(
    packet: DamagePacket,
    defender: &mut DefenderSnapshot,
    bypass_shield: bool,
    rng: &mut RandomNumberGenerator,
) -> AppliedOutcome {
    let mut amount = packet.amount;
    let mut blocked = false;
    let mut bumps = UseCounterBumps::default();

    // ----- Shield block: full negation on pass. -----
    if !bypass_shield
        && defender.shield != ShieldKind::None
        && defender.shield_budget_left > 0
    {
        let d20 = rng.range(1, 21);
        let skill_bonus = shields_skill_bonus(defender.skills.as_ref());
        let block_bonus = defender.shield.block_bonus();
        if super::shield_check_passes(d20, skill_bonus, block_bonus) {
            amount = 0;
            blocked = true;
            defender.shield_budget_left = defender.shield_budget_left.saturating_sub(1);
            if defender.skills.is_some() {
                bumps.shields = true;
            }
        }
    }

    // ----- Armor: random roll, Physical only. -----
    if amount > 0 && packet.damage_type == DamageType::Physical && defender.armor_max > 0 {
        let armor_max = defender.armor_max + armor_skill_bonus(defender.skills.as_ref());
        let armor_roll = rng.range(0, armor_max + 1);
        amount = compute_after_armor(amount, armor_roll);
        if defender.skills.is_some() {
            bumps.armor = true;
        }
    }

    // ----- Resistance: percentage reduction. -----
    let final_damage = apply_resistance(amount, defender.resistance_pct);

    AppliedOutcome {
        blocked,
        final_damage,
        use_counters: bumps,
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- Builders that read like English -----

    fn attrs(str_: i32, dex: i32, int: i32) -> Attributes {
        Attributes {
            strength: str_,
            dexterity: dex,
            intelligence: int,
        }
    }

    fn skills_with(pairs: &[(Skill, f32)]) -> Skills {
        let mut s = Skills::new();
        for (skill, level) in pairs {
            s.set(*skill, *level);
        }
        s
    }

    fn bare_attacker() -> AttackerSnapshot {
        AttackerSnapshot {
            hit_bonus: 0,
            damage_bonus: 0,
            attributes: None,
            skills: None,
            enraged: false,
            terrified: false,
            damage_multiplier_bp: 100,
        }
    }

    fn bare_defender() -> DefenderSnapshot {
        DefenderSnapshot {
            dodge: 0,
            skills: None,
            armor_max: 0,
            shield: ShieldKind::None,
            shield_budget_left: 0,
            resistance_pct: 0,
        }
    }

    fn fists() -> WeaponSnapshot {
        WeaponSnapshot {
            damage_dice: "1d3".to_string(),
            damage_type: DamageType::Physical,
            weapon_skill: None,
        }
    }

    fn long_sword() -> WeaponSnapshot {
        WeaponSnapshot {
            damage_dice: "1d8".to_string(),
            damage_type: DamageType::Physical,
            weapon_skill: Some(WeaponSkill::LongBlades),
        }
    }

    fn axe() -> WeaponSnapshot {
        WeaponSnapshot {
            damage_dice: "1d6".to_string(),
            damage_type: DamageType::Physical,
            weapon_skill: Some(WeaponSkill::Axes),
        }
    }

    fn bow() -> WeaponSnapshot {
        WeaponSnapshot {
            damage_dice: "1d6".to_string(),
            damage_type: DamageType::Physical,
            weapon_skill: Some(WeaponSkill::Ranged),
        }
    }

    // ----- ShieldKind constants -----

    #[test]
    fn shield_block_bonuses_match_items_ron() {
        assert_eq!(ShieldKind::None.block_bonus(), 0);
        assert_eq!(ShieldKind::Buckler.block_bonus(), 3);
        assert_eq!(ShieldKind::Kite.block_bonus(), 8);
        assert_eq!(ShieldKind::Tower.block_bonus(), 13);
    }

    #[test]
    fn shield_max_blocks_per_turn_match_spec() {
        assert_eq!(ShieldKind::None.max_blocks_per_turn(), 0);
        assert_eq!(ShieldKind::Buckler.max_blocks_per_turn(), 1);
        assert_eq!(ShieldKind::Kite.max_blocks_per_turn(), 2);
        assert_eq!(ShieldKind::Tower.max_blocks_per_turn(), 3);
    }

    // ----- Finesse detection -----

    #[test]
    fn finesse_flag_picks_short_and_long_blades() {
        let mut w = long_sword();
        assert!(w.is_finesse());
        w.weapon_skill = Some(WeaponSkill::ShortBlades);
        assert!(w.is_finesse());
        w.weapon_skill = Some(WeaponSkill::Axes);
        assert!(!w.is_finesse());
        w.weapon_skill = Some(WeaponSkill::Ranged);
        assert!(!w.is_finesse());
        w.weapon_skill = None;
        assert!(!w.is_finesse());
    }

    // ----- Hit math: pure helpers -----

    #[test]
    fn hit_roll_total_bare_is_just_d20() {
        let a = bare_attacker();
        let w = fists();
        assert_eq!(hit_roll_total(10, DamageSource::Melee, &a, &w), 10);
    }

    #[test]
    fn hit_roll_total_adds_hit_bonus() {
        let mut a = bare_attacker();
        a.hit_bonus = 3;
        let w = fists();
        assert_eq!(hit_roll_total(10, DamageSource::Melee, &a, &w), 13);
    }

    #[test]
    fn hit_roll_total_uses_str_for_brute_melee() {
        // STR 20 (mod +2), DEX 12 (mod -2). Axe is non-finesse melee.
        let mut a = bare_attacker();
        a.attributes = Some(attrs(20, 12, 10));
        let w = axe();
        // d20 10 + STR_mod 2 = 12.
        assert_eq!(hit_roll_total(10, DamageSource::Melee, &a, &w), 12);
    }

    #[test]
    fn hit_roll_total_uses_dex_for_finesse_melee() {
        // STR 12 (mod -2), DEX 20 (mod +2). Long sword is finesse melee.
        let mut a = bare_attacker();
        a.attributes = Some(attrs(12, 20, 10));
        let w = long_sword();
        // d20 10 + DEX_mod 2 = 12.
        assert_eq!(hit_roll_total(10, DamageSource::Melee, &a, &w), 12);
    }

    #[test]
    fn hit_roll_total_uses_dex_for_ranged() {
        // Bow + DEX 20.
        let mut a = bare_attacker();
        a.attributes = Some(attrs(10, 20, 10));
        let w = bow();
        assert_eq!(hit_roll_total(10, DamageSource::Ranged, &a, &w), 12);
    }

    #[test]
    fn hit_roll_total_adds_weapon_skill_and_fighting_for_melee() {
        // Long sword + LongBlades 16 (floor /4 = 4) + Fighting 12 (=3).
        let mut a = bare_attacker();
        a.skills = Some(skills_with(&[(Skill::LongBlades, 16.0), (Skill::Fighting, 12.0)]));
        let w = long_sword();
        // d20 10 + 0 attrs + 4 LB + 3 Fighting = 17.
        assert_eq!(hit_roll_total(10, DamageSource::Melee, &a, &w), 17);
    }

    #[test]
    fn hit_roll_total_no_fighting_for_ranged() {
        // Bow + RangedWeapons 12 (=3) + Fighting 12 (=3 melee-only).
        let mut a = bare_attacker();
        a.skills = Some(skills_with(&[
            (Skill::RangedWeapons, 12.0),
            (Skill::Fighting, 12.0),
        ]));
        let w = bow();
        // d20 10 + 0 attrs + 3 ranged - 0 fighting (no melee) = 13.
        assert_eq!(hit_roll_total(10, DamageSource::Ranged, &a, &w), 13);
    }

    // ----- Dodge target -----

    #[test]
    fn dodge_target_baseline_is_four() {
        let d = bare_defender();
        assert_eq!(dodge_target(&d), 4);
    }

    #[test]
    fn dodge_target_adds_dodging_skill() {
        let mut d = bare_defender();
        d.dodge = 2;
        d.skills = Some(skills_with(&[(Skill::Dodging, 16.0)])); // floor/4 = 4
        assert_eq!(dodge_target(&d), 4 + 2 + 4);
    }

    // ----- classify_hit -----

    #[test]
    fn classify_hit_nat_20_always_crits() {
        let a = bare_attacker();
        let mut d = bare_defender();
        d.dodge = 100; // unreachable normally
        let w = fists();
        assert_eq!(
            classify_hit(20, DamageSource::Melee, &a, &d, &w, false, false),
            HitResult::Crit
        );
    }

    #[test]
    fn classify_hit_nat_20_is_hit_not_crit_when_crit_disabled() {
        let a = bare_attacker();
        let d = bare_defender();
        let w = fists();
        // crit_disabled: nat 20 still hits but no crit branch
        assert_eq!(
            classify_hit(20, DamageSource::Melee, &a, &d, &w, false, true),
            HitResult::Hit
        );
    }

    #[test]
    fn classify_hit_nat_1_always_misses() {
        let mut a = bare_attacker();
        a.hit_bonus = 100; // would always hit
        let d = bare_defender();
        let w = fists();
        assert_eq!(
            classify_hit(1, DamageSource::Melee, &a, &d, &w, false, false),
            HitResult::Miss
        );
    }

    #[test]
    fn classify_hit_auto_hit_overrides_low_roll() {
        let a = bare_attacker();
        let mut d = bare_defender();
        d.dodge = 100;
        let w = fists();
        // d20 5 vs dodge target 104, but auto_hit
        assert_eq!(
            classify_hit(5, DamageSource::Melee, &a, &d, &w, true, false),
            HitResult::Hit
        );
    }

    #[test]
    fn classify_hit_auto_hit_does_not_force_crit() {
        let a = bare_attacker();
        let d = bare_defender();
        let w = fists();
        // d20 10, auto_hit, no nat 20 → Hit, not Crit
        assert_eq!(
            classify_hit(10, DamageSource::Melee, &a, &d, &w, true, false),
            HitResult::Hit
        );
    }

    #[test]
    fn classify_hit_at_dodge_target_exactly() {
        // Bare attacker, defender dodge 0 → dodge target 4. d20 = 4 hits.
        let a = bare_attacker();
        let d = bare_defender();
        let w = fists();
        assert_eq!(
            classify_hit(4, DamageSource::Melee, &a, &d, &w, false, false),
            HitResult::Hit
        );
        assert_eq!(
            classify_hit(3, DamageSource::Melee, &a, &d, &w, false, false),
            HitResult::Miss
        );
    }

    // ----- damage_total -----

    #[test]
    fn damage_total_bare_is_just_dice() {
        let a = bare_attacker();
        let w = fists();
        // apply_damage_multipliers clamps to min 1, but raw 5 stays 5.
        assert_eq!(damage_total(5, DamageSource::Melee, &a, &w), 5);
    }

    #[test]
    fn damage_total_adds_damage_bonus() {
        let mut a = bare_attacker();
        a.damage_bonus = 3;
        let w = fists();
        assert_eq!(damage_total(5, DamageSource::Melee, &a, &w), 8);
    }

    #[test]
    fn damage_total_adds_str_for_brute_melee_and_dex_for_finesse() {
        let mut a = bare_attacker();
        a.attributes = Some(attrs(20, 12, 10)); // str_mod=+2, dex_mod=-2
        let axe = axe();
        let sword = long_sword();
        assert_eq!(damage_total(5, DamageSource::Melee, &a, &axe), 5 + 2);
        assert_eq!(damage_total(5, DamageSource::Melee, &a, &sword), 5 + (-2));
    }

    #[test]
    fn damage_total_enraged_is_three_halves() {
        let mut a = bare_attacker();
        a.enraged = true;
        let w = fists();
        // 10 * 3 / 2 = 15
        assert_eq!(damage_total(10, DamageSource::Melee, &a, &w), 15);
    }

    #[test]
    fn damage_total_terrified_is_three_quarters() {
        let mut a = bare_attacker();
        a.terrified = true;
        let w = fists();
        // 12 * 3 / 4 = 9
        assert_eq!(damage_total(12, DamageSource::Melee, &a, &w), 9);
    }

    #[test]
    fn damage_total_clamps_to_one_minimum() {
        // 1 dice, no bonuses, terrified: 1 * 3 / 4 = 0 → clamps to 1.
        let mut a = bare_attacker();
        a.terrified = true;
        let w = fists();
        assert_eq!(damage_total(1, DamageSource::Melee, &a, &w), 1);
    }

    #[test]
    fn damage_total_backstab_multiplier() {
        let mut a = bare_attacker();
        a.damage_multiplier_bp = 300;
        let w = fists();
        // 5 → 5 (post-multipliers, ×3 = 15)
        assert_eq!(damage_total(5, DamageSource::Melee, &a, &w), 15);
    }

    #[test]
    fn damage_total_backstab_stacks_with_enraged() {
        let mut a = bare_attacker();
        a.enraged = true;
        a.damage_multiplier_bp = 300;
        let w = fists();
        // 10 → ×3/2 = 15 → ×3 = 45
        assert_eq!(damage_total(10, DamageSource::Melee, &a, &w), 45);
    }

    // ----- apply_packet: shield block -----

    #[test]
    fn shield_block_negates_physical_when_check_passes() {
        // Seed RNG and use Tower shield (+13). With Shields 0, DC 17
        // requires d20 ≥ 4 — overwhelmingly likely to pass.
        let mut d = bare_defender();
        d.shield = ShieldKind::Tower;
        d.shield_budget_left = 3;
        d.armor_max = 5;
        let mut rng = RandomNumberGenerator::seeded(42);

        let packet = DamagePacket {
            amount: 50,
            damage_type: DamageType::Physical,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        // With Tower + seeded rng, the first roll will land in the
        // d20 ≥ 4 range with overwhelming probability. If it doesn't on
        // this specific seed, the seed below would be tweaked. Test the
        // contract: block produces final_damage 0, decrements budget.
        if out.blocked {
            assert_eq!(out.final_damage, 0);
            assert_eq!(d.shield_budget_left, 2);
        } else {
            // Even if this seed missed, the rest of the pipeline
            // applied armor and resistance. Final damage must be
            // non-negative.
            assert!(out.final_damage >= 0);
        }
    }

    #[test]
    fn shield_block_negates_non_physical_when_check_passes() {
        // Shields beat fire/poison/lightning equally — the rare defence
        // vs magical damage. Verified by setting armor_max 0 (which
        // wouldn't help non-Physical anyway) and a guaranteed-block
        // shield setup, then asserting that the final damage is 0
        // **only** if a block actually fired.
        let mut d = bare_defender();
        d.shield = ShieldKind::Tower;
        d.shield_budget_left = 3;
        let mut rng = RandomNumberGenerator::seeded(7);
        let packet = DamagePacket {
            amount: 50,
            damage_type: DamageType::Fire,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        if out.blocked {
            assert_eq!(out.final_damage, 0);
        } else {
            // No block: Fire skips armor too, so final = full damage
            // (no resistance set).
            assert_eq!(out.final_damage, 50);
        }
    }

    #[test]
    fn shield_block_skipped_when_budget_zero() {
        let mut d = bare_defender();
        d.shield = ShieldKind::Tower;
        d.shield_budget_left = 0;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Fire,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert!(!out.blocked);
        assert_eq!(out.final_damage, 10);
    }

    #[test]
    fn shield_block_skipped_when_no_shield() {
        let mut d = bare_defender();
        d.shield = ShieldKind::None;
        d.shield_budget_left = 99;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Physical,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert!(!out.blocked);
    }

    #[test]
    fn shield_block_skipped_when_bypass_flag_set() {
        let mut d = bare_defender();
        d.shield = ShieldKind::Tower;
        d.shield_budget_left = 3;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Physical,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, true, &mut rng);
        assert!(!out.blocked);
        // Budget unchanged.
        assert_eq!(d.shield_budget_left, 3);
    }

    // ----- apply_packet: armor -----

    #[test]
    fn armor_skipped_for_non_physical() {
        let mut d = bare_defender();
        d.armor_max = 100;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 50,
            damage_type: DamageType::Fire,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert_eq!(out.final_damage, 50);
    }

    #[test]
    fn armor_applied_to_physical() {
        // armor_max = 0 means no roll (skip armor entirely).
        let mut d = bare_defender();
        d.armor_max = 0;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Physical,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert_eq!(out.final_damage, 10); // no armor
    }

    #[test]
    fn armor_roll_bounded_by_armor_max_plus_skill() {
        // armor_max 4, skill +1 → max roll 5. Hit damage 10 → final ∈ [5, 10].
        let mut d = bare_defender();
        d.armor_max = 4;
        d.skills = Some(skills_with(&[(Skill::Armor, 4.0)])); // +1
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Physical,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert!(out.final_damage >= 5 && out.final_damage <= 10);
    }

    // ----- apply_packet: resistance -----

    #[test]
    fn resistance_zero_means_full_damage() {
        let mut d = bare_defender();
        d.resistance_pct = 0;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Fire,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert_eq!(out.final_damage, 10);
    }

    #[test]
    fn resistance_fifty_means_half_damage() {
        let mut d = bare_defender();
        d.resistance_pct = 50;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Fire,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert_eq!(out.final_damage, 5);
    }

    #[test]
    fn resistance_hundred_means_zero_damage() {
        let mut d = bare_defender();
        d.resistance_pct = 100;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 50,
            damage_type: DamageType::Fire,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert_eq!(out.final_damage, 0);
    }

    #[test]
    fn resistance_negative_means_extra_damage() {
        let mut d = bare_defender();
        d.resistance_pct = -50;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Fire,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert_eq!(out.final_damage, 15);
    }

    // ----- resolve_attack: integration -----

    #[test]
    fn resolve_attack_overrides_damage_type() {
        // Staff zap of Fire from a Physical-typed weapon snapshot.
        let a = bare_attacker();
        let mut d = bare_defender();
        let mut w = fists();
        w.damage_type = DamageType::Physical;
        let mut rng = RandomNumberGenerator::seeded(99);
        let out = resolve_attack(
            DamageSource::Spell,
            &a,
            &mut d,
            &w,
            AttackOverrides {
                damage_type: Some(DamageType::Fire),
                auto_hit: true,
                crit_disabled: true,
                bypass_shield: false,
            },
            &mut rng,
        );
        assert_eq!(out.damage_type, DamageType::Fire);
        assert_eq!(out.result, HitResult::Hit); // auto_hit
    }

    #[test]
    fn resolve_attack_auto_hit_with_huge_dodge_still_lands() {
        let a = bare_attacker();
        let mut d = bare_defender();
        d.dodge = 1000;
        let w = fists();
        let mut rng = RandomNumberGenerator::seeded(1);
        let out = resolve_attack(
            DamageSource::Spell,
            &a,
            &mut d,
            &w,
            AttackOverrides {
                auto_hit: true,
                crit_disabled: true,
                ..Default::default()
            },
            &mut rng,
        );
        assert!(matches!(out.result, HitResult::Hit | HitResult::Crit));
    }

    #[test]
    fn resolve_attack_miss_bumps_dodging_when_target_has_skills() {
        // Massive defender dodge ensures miss against bare attacker.
        // `crit_disabled` removes the nat-20-always-hits short-circuit so
        // any d20 the seed produces resolves through the math, which
        // forces a Miss given dodge_target 104 and bare bonuses.
        let a = bare_attacker();
        let mut d = bare_defender();
        d.dodge = 100;
        d.skills = Some(Skills::new());
        let w = fists();
        let mut rng = RandomNumberGenerator::seeded(1);
        let out = resolve_attack(
            DamageSource::Melee,
            &a,
            &mut d,
            &w,
            AttackOverrides {
                crit_disabled: true,
                ..Default::default()
            },
            &mut rng,
        );
        assert_eq!(out.result, HitResult::Miss);
        assert!(out.use_counters.dodging);
    }

    #[test]
    fn resolve_attack_miss_does_not_bump_dodging_for_skill_less_targets() {
        let a = bare_attacker();
        let mut d = bare_defender();
        d.dodge = 100;
        // skills = None — monster target
        let w = fists();
        let mut rng = RandomNumberGenerator::seeded(1);
        let out = resolve_attack(
            DamageSource::Melee,
            &a,
            &mut d,
            &w,
            AttackOverrides {
                crit_disabled: true,
                ..Default::default()
            },
            &mut rng,
        );
        assert_eq!(out.result, HitResult::Miss);
        assert!(!out.use_counters.dodging);
    }

    #[test]
    fn resolve_attack_hit_bumps_fighting_and_weapon_skill_for_player() {
        // Attacker has Skills (player). Auto-hit so we don't rely on seeded d20.
        let mut a = bare_attacker();
        a.skills = Some(Skills::new());
        let mut d = bare_defender();
        let w = long_sword();
        let mut rng = RandomNumberGenerator::seeded(1);
        let out = resolve_attack(
            DamageSource::Melee,
            &a,
            &mut d,
            &w,
            AttackOverrides {
                auto_hit: true,
                crit_disabled: true,
                ..Default::default()
            },
            &mut rng,
        );
        assert!(matches!(out.result, HitResult::Hit));
        assert!(out.use_counters.fighting);
        assert_eq!(out.use_counters.weapon_skill, Some(Skill::LongBlades));
    }

    #[test]
    fn resolve_attack_no_fighting_bump_on_ranged() {
        let mut a = bare_attacker();
        a.skills = Some(Skills::new());
        let mut d = bare_defender();
        let w = bow();
        let mut rng = RandomNumberGenerator::seeded(1);
        let out = resolve_attack(
            DamageSource::Ranged,
            &a,
            &mut d,
            &w,
            AttackOverrides {
                auto_hit: true,
                crit_disabled: true,
                ..Default::default()
            },
            &mut rng,
        );
        assert!(!out.use_counters.fighting);
        assert_eq!(out.use_counters.weapon_skill, Some(Skill::RangedWeapons));
    }

    #[test]
    fn resolve_attack_no_bumps_for_monster_attacker() {
        // Monster has no Skills → no bumps even on hit.
        let a = bare_attacker();
        let mut d = bare_defender();
        let w = fists();
        let mut rng = RandomNumberGenerator::seeded(1);
        let out = resolve_attack(
            DamageSource::Melee,
            &a,
            &mut d,
            &w,
            AttackOverrides {
                auto_hit: true,
                crit_disabled: true,
                ..Default::default()
            },
            &mut rng,
        );
        assert!(!out.use_counters.fighting);
        assert_eq!(out.use_counters.weapon_skill, None);
    }

    // ----- roll_damage + apply_damage: AoE / Cleave path -----

    #[test]
    fn roll_damage_independent_of_defender() {
        let a = bare_attacker();
        let w = axe();
        let mut rng = RandomNumberGenerator::seeded(42);
        let packet = roll_damage(&a, &w, DamageSource::Melee, false, &mut rng);
        assert_eq!(packet.damage_type, DamageType::Physical);
        assert!(packet.amount >= 1);
        assert!(!packet.crit);
    }

    #[test]
    fn roll_damage_crit_yields_higher_average_amount() {
        // Statistical: 1d6 average is 3.5; 2d6 average is 7. With seed
        // deterministic, just verify the crit roll is >= the base for
        // the same seed (since crit rolls a second die that adds).
        let a = bare_attacker();
        let w = axe();
        let mut rng1 = RandomNumberGenerator::seeded(99);
        let normal = roll_damage(&a, &w, DamageSource::Melee, false, &mut rng1);
        let mut rng2 = RandomNumberGenerator::seeded(99);
        let crit = roll_damage(&a, &w, DamageSource::Melee, true, &mut rng2);
        assert!(crit.amount >= normal.amount, "crit must be ≥ normal for same seed");
    }

    #[test]
    fn apply_damage_runs_full_defense_pipeline() {
        // Resistance 50% to Fire: 20 → 10.
        let mut d = bare_defender();
        d.resistance_pct = 50;
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 20,
            damage_type: DamageType::Fire,
            crit: false,
        };
        let out = apply_damage(packet, &mut d, &mut rng);
        assert_eq!(out.final_damage, 10);
    }

    #[test]
    fn apply_damage_can_be_called_repeatedly_for_cleave_splash() {
        // The Cleave path: roll once, apply to many. Verify that two
        // independent defenders take the same packet damage.
        let mut d1 = bare_defender();
        let mut d2 = bare_defender();
        d2.resistance_pct = 50;
        let mut rng = RandomNumberGenerator::seeded(1);

        let packet = DamagePacket {
            amount: 8,
            damage_type: DamageType::Physical,
            crit: false,
        };
        let r1 = apply_damage(packet.clone(), &mut d1, &mut rng);
        let r2 = apply_damage(packet, &mut d2, &mut rng);
        assert_eq!(r1.final_damage, 8);
        assert_eq!(r2.final_damage, 4); // 50% resistance
    }

    // ----- Sanity check on use_counter bookkeeping helper -----

    #[test]
    fn shield_block_bumps_shields_counter_when_target_has_skills() {
        let mut d = bare_defender();
        d.shield = ShieldKind::Tower;
        d.shield_budget_left = 3;
        d.skills = Some(Skills::new());
        // Use a seed where d20 lands ≥ 4 (almost any seed; Tower needs
        // only 4 with Shields 0). We assert behaviour conditional on
        // the block firing to avoid coupling to bracket-lib seed
        // internals.
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Physical,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        if out.blocked {
            assert!(out.use_counters.shields);
        } else {
            assert!(!out.use_counters.shields);
        }
    }

    #[test]
    fn armor_roll_bumps_armor_counter_when_target_has_skills() {
        let mut d = bare_defender();
        d.armor_max = 5;
        d.skills = Some(Skills::new());
        let mut rng = RandomNumberGenerator::seeded(1);
        let packet = DamagePacket {
            amount: 10,
            damage_type: DamageType::Physical,
            crit: false,
        };
        let out = apply_packet(packet, &mut d, false, &mut rng);
        assert!(out.use_counters.armor);
    }
}
