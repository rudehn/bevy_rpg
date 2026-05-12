//! Phase 3 skill system. See `docs/design/SKILLS.md` for the canonical spec.
//!
//! This commit (Task 1) lands the foundational enum types. Subsequent
//! tasks add pure helpers, ECS components, runtime systems, and UI.

#![allow(dead_code, unused_imports)]

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One of eight player skills. Tracks a float 0.0..=27.0 of progress;
/// effects unlock at integer breakpoints via `floor(level/4)`.
#[derive(
    Serialize, Deserialize, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash,
)]
pub enum Skill {
    Fighting,
    Axes,
    ShortBlades,
    LongBlades,
    RangedWeapons,
    Armor,
    Dodging,
    Evocations,
}

impl Skill {
    pub const ALL: [Skill; 8] = [
        Skill::Fighting,
        Skill::Axes,
        Skill::ShortBlades,
        Skill::LongBlades,
        Skill::RangedWeapons,
        Skill::Armor,
        Skill::Dodging,
        Skill::Evocations,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Skill::Fighting => "Fighting",
            Skill::Axes => "Axes",
            Skill::ShortBlades => "Short Blades",
            Skill::LongBlades => "Long Blades",
            Skill::RangedWeapons => "Ranged Weapons",
            Skill::Armor => "Armor",
            Skill::Dodging => "Dodging",
            Skill::Evocations => "Evocations",
        }
    }
}

impl std::fmt::Display for Skill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Weapon family — declared on `ItemAsset` for weapons. Drives which
/// skill `weapon_skill_bonus` returns for a given equipped weapon.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaponSkill {
    Axes,
    ShortBlades,
    LongBlades,
    Ranged,
}

impl WeaponSkill {
    /// Map a weapon-skill tag to its corresponding `Skill` enum value.
    pub const fn as_skill(self) -> Skill {
        match self {
            WeaponSkill::Axes => Skill::Axes,
            WeaponSkill::ShortBlades => Skill::ShortBlades,
            WeaponSkill::LongBlades => Skill::LongBlades,
            WeaponSkill::Ranged => Skill::RangedWeapons,
        }
    }
}

/// Per-skill training selection. Cycle order in the UI:
/// Normal → Focused → Disabled → Normal.
#[derive(Serialize, Deserialize, Reflect, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkillState {
    #[default]
    Normal,
    Focused,
    Disabled,
}

/// Global training mode — flipped from the skill screen.
#[derive(Serialize, Deserialize, Reflect, Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrainingMode {
    /// Skills selected (Normal or Focused) divide XP weighted by recent use.
    #[default]
    Auto,
    /// Skills selected divide XP evenly (Focused gets 2× share).
    Manual,
}

// ---------------------------------------------------------------------
// Components and resources
// ---------------------------------------------------------------------

/// Per-skill current level on a character entity (player only today).
/// Values clamp to `[0.0, 27.0]`.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Default)]
#[reflect(Component)]
pub struct Skills {
    pub levels: HashMap<Skill, f32>,
}

impl Skills {
    pub fn new() -> Self {
        Self {
            levels: Skill::ALL.iter().map(|&s| (s, 0.0)).collect(),
        }
    }

    pub fn get(&self, skill: Skill) -> f32 {
        self.levels.get(&skill).copied().unwrap_or(0.0)
    }

    pub fn set(&mut self, skill: Skill, level: f32) {
        self.levels.insert(skill, level.clamp(0.0, 27.0));
    }
}

/// Per-skill training selection on the player entity.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Default)]
#[reflect(Component)]
pub struct SkillTraining {
    pub states: HashMap<Skill, SkillState>,
}

impl SkillTraining {
    pub fn new() -> Self {
        Self {
            states: Skill::ALL.iter().map(|&s| (s, SkillState::Normal)).collect(),
        }
    }

    pub fn get(&self, skill: Skill) -> SkillState {
        self.states.get(&skill).copied().unwrap_or_default()
    }

    pub fn cycle(&mut self, skill: Skill) {
        let next = match self.get(skill) {
            SkillState::Normal => SkillState::Focused,
            SkillState::Focused => SkillState::Disabled,
            SkillState::Disabled => SkillState::Normal,
        };
        self.states.insert(skill, next);
    }
}

/// Per-skill **raw** cumulative XP totals (the curve input for
/// `xp_to_level`). Kept separate from `Skills::levels` so the level
/// is always a pure function of accumulated XP — no drift.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Default)]
#[reflect(Component)]
pub struct SkillXp {
    pub totals: HashMap<Skill, u64>,
}

impl SkillXp {
    pub fn new() -> Self {
        Self {
            totals: Skill::ALL.iter().map(|&s| (s, 0)).collect(),
        }
    }

    pub fn get(&self, skill: Skill) -> u64 {
        self.totals.get(&skill).copied().unwrap_or(0)
    }

    pub fn add(&mut self, skill: Skill, xp: u64) {
        *self.totals.entry(skill).or_insert(0) += xp;
    }
}

/// Unallocated skill XP, accumulating from gameplay events. Drained
/// by `allocate_skill_xp` into per-skill `SkillXp`.
#[derive(Resource, Reflect, Serialize, Deserialize, Debug, Default)]
pub struct SkillXpPool {
    pub raw: u64,
}

/// Per-skill use counters for Auto-mode allocation weighting.
#[derive(Resource, Reflect, Serialize, Deserialize, Debug, Default)]
pub struct SkillUseCounters {
    pub counts: HashMap<Skill, u32>,
}

impl SkillUseCounters {
    pub fn bump(&mut self, skill: Skill) {
        *self.counts.entry(skill).or_insert(0) += 1;
    }
}

// ---------------------------------------------------------------------
// Pure helpers — XP curve, aptitudes, weapon-skill / fighting bonuses
// ---------------------------------------------------------------------

/// Total skill XP required to reach each integer level (1..=27),
/// taken verbatim from the DCSS skill XP table. Index 0 = level 1,
/// index 26 = level 27.
const XP_THRESHOLDS: [u64; 27] = [
    50, 150, 300, 500, 750, 1_050, 1_400, 1_800, 2_250, 2_775,
    3_375, 4_050, 4_800, 5_625, 6_525, 7_500, 8_550, 9_675, 10_900,
    12_225, 13_650, 15_175, 16_800, 18_525, 20_350, 22_275, 24_325,
];

/// Convert a cumulative skill-XP count to a fractional skill level
/// in `[0.0, 27.0]`. Linearly interpolates between integer thresholds.
pub fn xp_to_level(xp: u64) -> f32 {
    if xp < XP_THRESHOLDS[0] {
        return xp as f32 / XP_THRESHOLDS[0] as f32;
    }
    for (i, &threshold) in XP_THRESHOLDS.iter().enumerate() {
        if xp < threshold {
            let prev = if i == 0 { 0 } else { XP_THRESHOLDS[i - 1] };
            let span = threshold - prev;
            let into = xp - prev;
            return (i as f32) + (into as f32 / span as f32);
        }
    }
    27.0
}

/// XP cost multiplier from a DCSS aptitude value. `2^(-apt/4)`.
/// Aptitude +4 halves XP cost, −4 doubles it.
pub fn aptitude_multiplier(apt: i32) -> f32 {
    let apt_f = apt as f32;
    2.0_f32.powf(-apt_f / 4.0)
}

/// The skill bonus that applies to hit/damage for the given weapon and
/// attack source. Returns 0 for monsters (no `Skills` component) or
/// for weapons with no `weapon_skill` tag.
pub fn weapon_skill_bonus(
    weapon_skill: Option<WeaponSkill>,
    source: roguelike_engine::combat::DamageSource,
    skills: Option<&Skills>,
) -> i32 {
    use roguelike_engine::combat::DamageSource;
    let Some(skills) = skills else { return 0 };
    let target_skill = match source {
        DamageSource::Ranged => Skill::RangedWeapons,
        DamageSource::Melee => match weapon_skill {
            Some(ws) => ws.as_skill(),
            None => return 0,
        },
        _ => return 0,
    };
    let level = skills.get(target_skill);
    (level / 4.0).floor() as i32
}

/// Fighting bonus on **melee** hit and damage rolls. 0 for non-melee
/// or for entities without `Skills`.
pub fn fighting_melee_bonus(
    source: roguelike_engine::combat::DamageSource,
    skills: Option<&Skills>,
) -> i32 {
    use roguelike_engine::combat::DamageSource;
    if source != DamageSource::Melee {
        return 0;
    }
    let Some(skills) = skills else { return 0 };
    (skills.get(Skill::Fighting) / 4.0).floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_skills_have_unique_names() {
        let names: std::collections::HashSet<_> =
            Skill::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names.len(), 8);
    }

    #[test]
    fn weapon_skill_maps_to_skill() {
        assert_eq!(WeaponSkill::Axes.as_skill(), Skill::Axes);
        assert_eq!(WeaponSkill::ShortBlades.as_skill(), Skill::ShortBlades);
        assert_eq!(WeaponSkill::LongBlades.as_skill(), Skill::LongBlades);
        assert_eq!(WeaponSkill::Ranged.as_skill(), Skill::RangedWeapons);
    }

    #[test]
    fn skill_deserializes_from_unit_variant_name() {
        // Each Skill variant round-trips through RON as its Debug name.
        for skill in Skill::ALL {
            let ron_str = format!("{:?}", skill);
            let parsed: Skill = ron::from_str(&ron_str)
                .unwrap_or_else(|e| panic!("failed to parse {ron_str}: {e}"));
            assert_eq!(parsed, skill);
        }
    }

    // --- Component / resource tests ---

    #[test]
    fn skills_get_returns_zero_for_missing_skill() {
        let s = Skills::default();
        for skill in Skill::ALL {
            assert_eq!(s.get(skill), 0.0);
        }
    }

    #[test]
    fn skills_new_initializes_all_to_zero() {
        let s = Skills::new();
        assert_eq!(s.levels.len(), 8);
        for skill in Skill::ALL {
            assert_eq!(s.get(skill), 0.0);
        }
    }

    #[test]
    fn skills_set_clamps_to_valid_range() {
        let mut s = Skills::new();
        s.set(Skill::Fighting, 30.0);
        assert_eq!(s.get(Skill::Fighting), 27.0);
        s.set(Skill::Fighting, -5.0);
        assert_eq!(s.get(Skill::Fighting), 0.0);
        s.set(Skill::Fighting, 4.5);
        assert_eq!(s.get(Skill::Fighting), 4.5);
    }

    #[test]
    fn training_cycle_walks_normal_focused_disabled_normal() {
        let mut t = SkillTraining::new();
        assert_eq!(t.get(Skill::Fighting), SkillState::Normal);
        t.cycle(Skill::Fighting);
        assert_eq!(t.get(Skill::Fighting), SkillState::Focused);
        t.cycle(Skill::Fighting);
        assert_eq!(t.get(Skill::Fighting), SkillState::Disabled);
        t.cycle(Skill::Fighting);
        assert_eq!(t.get(Skill::Fighting), SkillState::Normal);
    }

    #[test]
    fn use_counters_bump_increments() {
        let mut c = SkillUseCounters::default();
        c.bump(Skill::Fighting);
        c.bump(Skill::Fighting);
        c.bump(Skill::Axes);
        assert_eq!(c.counts.get(&Skill::Fighting), Some(&2));
        assert_eq!(c.counts.get(&Skill::Axes), Some(&1));
    }

    #[test]
    fn skill_xp_add_accumulates() {
        let mut x = SkillXp::new();
        x.add(Skill::Fighting, 100);
        x.add(Skill::Fighting, 50);
        x.add(Skill::Axes, 25);
        assert_eq!(x.get(Skill::Fighting), 150);
        assert_eq!(x.get(Skill::Axes), 25);
        assert_eq!(x.get(Skill::Dodging), 0);
    }

    // --- Pure-helper tests ---

    #[test]
    fn xp_to_level_zero_xp_is_zero_level() {
        assert_eq!(xp_to_level(0), 0.0);
    }

    #[test]
    fn xp_to_level_hits_integer_thresholds() {
        // Each threshold value = exactly that level reached
        assert!((xp_to_level(50) - 1.0).abs() < 0.001);
        assert!((xp_to_level(150) - 2.0).abs() < 0.001);
        assert!((xp_to_level(2_775) - 10.0).abs() < 0.001);
        assert!((xp_to_level(24_325) - 27.0).abs() < 0.001);
    }

    #[test]
    fn xp_to_level_caps_at_27() {
        assert_eq!(xp_to_level(50_000), 27.0);
        assert_eq!(xp_to_level(u64::MAX), 27.0);
    }

    #[test]
    fn xp_to_level_interpolates_between_thresholds() {
        // Halfway between level 1 (50) and level 2 (150):
        // span = 100, into = 50, so level = 1 + 0.5 = 1.5
        assert!((xp_to_level(100) - 1.5).abs() < 0.001);
    }

    #[test]
    fn aptitude_multiplier_matches_dcss_values() {
        let cases = [
            (5, 0.4204_f32),
            (4, 0.5_f32),
            (2, 0.7071_f32),
            (0, 1.0_f32),
            (-2, 1.4142_f32),
            (-4, 2.0_f32),
            (-5, 2.3784_f32),
        ];
        for (apt, expected) in cases {
            let got = aptitude_multiplier(apt);
            assert!(
                (got - expected).abs() < 0.001,
                "aptitude_multiplier({apt}) = {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn weapon_skill_bonus_zero_for_no_skills() {
        use roguelike_engine::combat::DamageSource;
        assert_eq!(
            weapon_skill_bonus(Some(WeaponSkill::LongBlades), DamageSource::Melee, None),
            0
        );
    }

    #[test]
    fn weapon_skill_bonus_picks_correct_skill_per_source() {
        use roguelike_engine::combat::DamageSource;
        let mut s = Skills::new();
        s.set(Skill::LongBlades, 16.0); // floor(16/4) = 4
        s.set(Skill::RangedWeapons, 8.0); // floor(8/4) = 2
        // Melee with a Long-Blade weapon: +4
        assert_eq!(
            weapon_skill_bonus(Some(WeaponSkill::LongBlades), DamageSource::Melee, Some(&s)),
            4
        );
        // Ranged source ignores the held-weapon tag — always RangedWeapons skill: +2
        assert_eq!(
            weapon_skill_bonus(Some(WeaponSkill::ShortBlades), DamageSource::Ranged, Some(&s)),
            2
        );
        // Melee with no weapon skill (staff bash): 0
        assert_eq!(
            weapon_skill_bonus(None, DamageSource::Melee, Some(&s)),
            0
        );
    }

    #[test]
    fn fighting_melee_bonus_applies_only_to_melee() {
        use roguelike_engine::combat::DamageSource;
        let mut s = Skills::new();
        s.set(Skill::Fighting, 12.0); // floor(12/4) = 3
        assert_eq!(fighting_melee_bonus(DamageSource::Melee, Some(&s)), 3);
        assert_eq!(fighting_melee_bonus(DamageSource::Ranged, Some(&s)), 0);
        assert_eq!(fighting_melee_bonus(DamageSource::Spell, Some(&s)), 0);
    }
}
