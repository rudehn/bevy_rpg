//! Stealth system — game-side modifier composition + per-turn systems.
//! See docs/design/STEALTH.md for the canonical writeup.

use bevy::prelude::*;
use bracket_lib::prelude::{Point, RandomNumberGenerator};

use crate::character::Attributes;
use crate::components::{Monster, Position, Viewshed};
use crate::game::ai::{MonsterAI, MonsterAIMode};
use crate::game::items::{Equipment, ItemProperties};
use crate::game::skills::{Skill, Skills, SkillUseCounters};
use crate::map::Map;
use crate::map::light::LightMap;
use crate::player::Player;
use roguelike_engine::combat::events::DamageEvent;
use roguelike_engine::squad::SquadId;
use roguelike_engine::stealth::{
    noise_modifier, Awareness, AwarenessAlertEvent, AwarenessRecord, AwarenessState, NoiseMap,
};
use roguelike_engine::turn::TurnManager;

/// Per-monster species perception modifier, copied from MonsterAsset
/// at spawn time. Read by perception_tick_system to build
/// PerceptionComponents.base. Inserted on every monster in Task F1.
#[derive(Component, Debug, Clone, Copy)]
pub struct MonsterPerception(pub i32);

/// Tile-light → stealth modifier. Bright = penalty, dark = bonus.
/// Thresholds are placeholders — expect post-implementation tuning.
pub fn light_modifier(intensity: f32) -> i32 {
    if intensity >= 0.75 {
        -3
    } else if intensity >= 0.40 {
        -1
    } else if intensity > 0.0 {
        2
    } else {
        3
    }
}

/// Distance → perception bonus. Closer = easier to see.
/// Chebyshev distance (matches 8-way movement).
pub fn close_range_bonus(chebyshev_distance: i32) -> i32 {
    match chebyshev_distance {
        d if d <= 1 => 2,
        2..=3 => 1,
        _ => 0,
    }
}

/// Component breakdown for the stealth side of the opposed roll.
/// Returned by `compute_stealth_components` for UI display.
#[derive(Debug, Clone, Copy)]
pub struct StealthComponents {
    pub skill_half: i32,
    pub dex_mod: i32,
    pub armor_penalty: i32,
    pub light_mod: i32,
    pub noise_mod: i32,
}

impl StealthComponents {
    pub fn total(&self) -> i32 {
        self.skill_half + self.dex_mod - self.armor_penalty + self.light_mod + self.noise_mod
    }
}

/// Component breakdown for the perception side.
#[derive(Debug, Clone, Copy)]
pub struct PerceptionComponents {
    pub base: i32,
    /// -10 if the monster is asleep, 0 otherwise.
    pub asleep_penalty: i32,
    pub close_range_bonus: i32,
}

impl PerceptionComponents {
    pub fn total(&self) -> i32 {
        self.base + self.asleep_penalty + self.close_range_bonus
    }
}

/// Build the stealth breakdown for `target_pos`. Callers resolve
/// `light_intensity` from `LightMap` (which has no `intensity_at`
/// helper — index via `map.xy_idx(pos.x, pos.y)` into
/// `light_map.values`) and `equipped_armor_penalty` via
/// [`equipped_armor_stealth_penalty`].
pub fn compute_stealth_components(
    skills: Option<&Skills>,
    attrs: Option<&Attributes>,
    equipped_armor_penalty: i32,
    target_pos: Point,
    light_intensity: f32,
    noise_map: &NoiseMap,
) -> StealthComponents {
    let stealth_level = skills.map(|s| s.get(Skill::Stealth)).unwrap_or(0.0) as i32;
    let dex_mod = attrs.map(|a| a.dex_mod()).unwrap_or(0);
    StealthComponents {
        skill_half: stealth_level / 2,
        dex_mod,
        armor_penalty: equipped_armor_penalty,
        light_mod: light_modifier(light_intensity),
        noise_mod: noise_modifier(target_pos, noise_map),
    }
}

pub fn compute_perception_components(
    monster_base_perception: i32,
    is_asleep: bool,
    chebyshev_distance: i32,
) -> PerceptionComponents {
    PerceptionComponents {
        base: monster_base_perception,
        asleep_penalty: if is_asleep { -10 } else { 0 },
        close_range_bonus: close_range_bonus(chebyshev_distance),
    }
}

/// Public-API convenience: total stealth mod without breakdown. The
/// internal callsite (`perception_tick_system`) uses
/// `compute_stealth_components` to avoid recomputing the breakdown for
/// the UI; external consumers that only need the i32 sum can call this.
#[allow(dead_code)]
pub fn compute_stealth_mod(
    skills: Option<&Skills>,
    attrs: Option<&Attributes>,
    equipped_armor_penalty: i32,
    target_pos: Point,
    light_intensity: f32,
    noise_map: &NoiseMap,
) -> i32 {
    compute_stealth_components(
        skills,
        attrs,
        equipped_armor_penalty,
        target_pos,
        light_intensity,
        noise_map,
    )
    .total()
}

pub fn compute_perception_mod(
    monster_base_perception: i32,
    is_asleep: bool,
    chebyshev_distance: i32,
) -> i32 {
    compute_perception_components(monster_base_perception, is_asleep, chebyshev_distance).total()
}

/// Sum the stealth penalty across the wearer's currently equipped
/// armor slots (helm, chest, gloves, boots, offhand). Weapons, rings,
/// and amulets are ignored — they all carry `armor_stealth_penalty: 0`
/// in the asset schema but we skip them defensively to make the intent
/// obvious. Returns 0 if the wearer has no armor equipped or none of
/// the slot entities resolve to an `ItemProperties` component.
///
/// Used by `perception_tick_system` to feed `compute_stealth_mod`.
pub fn equipped_armor_stealth_penalty(
    equipment: &Equipment,
    item_query: &Query<&ItemProperties>,
) -> i32 {
    [
        equipment.helm,
        equipment.chest,
        equipment.gloves,
        equipment.boots,
        equipment.offhand,
    ]
    .into_iter()
    .flatten()
    .filter_map(|e| item_query.get(e).ok())
    .map(|props| props.armor_stealth_penalty)
    .sum()
}

/// Read the tile light intensity at `pos` out of [`LightMap`].
///
/// The engine `LightMap` exposes `values: Vec<f32>` but no width/height
/// of its own — the buffer is sized by the active [`Map`], so the
/// caller has to do the indexing. Out-of-bounds positions return 0.0
/// (treated as fully dark by [`light_modifier`]).
fn light_intensity_at(light_map: &LightMap, map: &Map, pos: Point) -> f32 {
    if pos.x < 0 || pos.y < 0 || pos.x >= map.width || pos.y >= map.height {
        return 0.0;
    }
    let idx = map.xy_idx(pos.x, pos.y);
    light_map.values.get(idx).copied().unwrap_or(0.0)
}

/// Per-perceiver opposed roll system. Runs once per turn inside
/// `ProcessingPhase::Brain` (scheduled by `StealthPlugin` in Task E8)
/// before monster AI mode dispatch, so the AI sees fresh awareness
/// when it picks a target.
///
/// For each perceiver with an [`Awareness`] component, for each entity
/// in their viewshed, roll opposed d20s and transition non-Aware
/// records to [`AwarenessState::Aware`] on a perception win. `Aware`
/// is sticky — it's never demoted by this system (the
/// `awareness_tick_system` handles `Searching`/`Suspicious` decay).
/// Emits [`AwarenessAlertEvent`] for squad propagation (Task E5).
///
/// Targets without a [`Skills`]/[`Attributes`]/[`Equipment`] component
/// fall back to a 0 contribution from that source — the system still
/// runs, it just produces a (skill-less, attr-less, gear-less)
/// stealth total. This keeps NPCs and other non-player entities legal
/// targets without forcing them to be fully kitted out.
pub fn perception_tick_system(
    mut perceivers: Query<(
        Entity,
        &mut Awareness,
        &Viewshed,
        Option<&MonsterAI>,
        &Position,
        &MonsterPerception,
    )>,
    targets: Query<(
        Entity,
        &Position,
        Option<&Skills>,
        Option<&Attributes>,
        Option<&Equipment>,
    )>,
    equipment_items: Query<&ItemProperties>,
    light_map: Res<LightMap>,
    map: Res<Map>,
    noise_map: Res<NoiseMap>,
    turn_manager: Res<TurnManager>,
    mut alerts: MessageWriter<AwarenessAlertEvent>,
) {
    let now = turn_manager.current_time;
    let mut rng = RandomNumberGenerator::new();

    for (seeker, mut awareness, vs, ai, seeker_pos, monster_perception) in &mut perceivers {
        let is_asleep = ai
            .map(|a| a.mode == MonsterAIMode::Asleep)
            .unwrap_or(false);
        let monster_base_perception = monster_perception.0;
        let seeker_point = seeker_pos.to_point();

        for (target, target_pos, target_skills, target_attrs, target_equipment) in &targets {
            if seeker == target {
                continue;
            }
            let target_point = target_pos.to_point();
            if !vs.visible_tiles.contains(&target_point) {
                continue;
            }

            // Sticky Aware: skip the roll entirely.
            if let Some(rec) = awareness.get(target) {
                if matches!(rec.state, AwarenessState::Aware) {
                    continue;
                }
            }

            // Chebyshev distance — matches 8-way movement.
            let dist = (seeker_point.x - target_point.x)
                .abs()
                .max((seeker_point.y - target_point.y).abs());

            let perc_components =
                compute_perception_components(monster_base_perception, is_asleep, dist);

            let light_intensity = light_intensity_at(&light_map, &map, target_point);

            let armor_pen = target_equipment
                .map(|eq| equipped_armor_stealth_penalty(eq, &equipment_items))
                .unwrap_or(0);

            let stealth_components = compute_stealth_components(
                target_skills,
                target_attrs,
                armor_pen,
                target_point,
                light_intensity,
                &noise_map,
            );

            let perc_roll = rng.roll_dice(1, 20);
            let stealth_roll = rng.roll_dice(1, 20);
            let perc_total = perc_roll + perc_components.total();
            let stealth_total = stealth_roll + stealth_components.total();

            if perc_total > stealth_total {
                let entry = awareness.records.entry(target).or_insert(AwarenessRecord {
                    state: AwarenessState::Hidden,
                    last_update_turn: now,
                    last_seen_pos: None,
                });
                entry.state = AwarenessState::Aware;
                entry.last_update_turn = now;
                entry.last_seen_pos = Some(target_point);
                alerts.write(AwarenessAlertEvent { seeker, target });
            }
        }
    }
}

/// Squad propagation: when one squadmate becomes Aware of a target,
/// upgrade every other member of the same [`SquadId`] to (at least)
/// [`AwarenessState::Searching`] anchored at the target's current
/// position. Members already at [`AwarenessState::Aware`] are left
/// alone — Aware is sticky and outranks Searching.
///
/// Reads [`AwarenessAlertEvent`] (emitted by `perception_tick_system`)
/// and runs in `ProcessingPhase::ResolveActions` so the upgraded state
/// is visible to the *next* turn's Brain phase.
///
/// Squadmates that lack an [`Awareness`] component are silently skipped
/// — `Awareness` is opt-in per perceiver.
pub fn squad_propagate_awareness(
    mut alerts: MessageReader<AwarenessAlertEvent>,
    squad_lookup: Query<(Entity, &SquadId)>,
    target_positions: Query<&Position>,
    mut perceivers: Query<&mut Awareness>,
    turn_manager: Res<TurnManager>,
) {
    let now = turn_manager.current_time;
    let giveup_at = now + 20;
    for ev in alerts.read() {
        let Ok((_, seeker_squad)) = squad_lookup.get(ev.seeker) else {
            continue;
        };
        let Ok(target_pos_comp) = target_positions.get(ev.target) else {
            continue;
        };
        let target_pt = Point::new(target_pos_comp.x, target_pos_comp.y);

        for (squadmate, sq) in &squad_lookup {
            if squadmate == ev.seeker {
                continue;
            }
            if sq != seeker_squad {
                continue;
            }

            let Ok(mut awareness) = perceivers.get_mut(squadmate) else {
                continue;
            };
            let cur = awareness.get(ev.target).map(|r| r.state);
            // Don't downgrade Aware; otherwise upgrade-or-refresh to Searching.
            if matches!(cur, Some(AwarenessState::Aware)) {
                continue;
            }
            awareness.set(
                ev.target,
                AwarenessState::Searching {
                    last_known_pos: target_pt,
                    giveup_at_turn: giveup_at,
                },
                now,
            );
        }
    }
}

/// Attack-reveal: any time an entity takes damage from a known
/// attacker, the victim immediately becomes [`AwarenessState::Aware`]
/// of that attacker. This guarantees that a backstabbed monster wakes
/// up regardless of light, distance, or skill — there's no "stealthy
/// hit that fails to register" gotcha.
///
/// Filters:
/// - Environmental damage (`attacker: None`) is ignored.
/// - Self-damage (e.g. an explode-on-death blasting its own corpse) is
///   ignored — `attacker == target` should not flag the victim as
///   aware of itself.
/// - Victims without an [`Awareness`] component are silently skipped
///   (the player today has none — that's fine, this system runs only
///   on entities that opt in via the component).
///
/// Scheduled in `ProcessingPhase::ResolveActions` so it runs after the
/// damage pipeline has emitted [`DamageEvent`]s for this turn but
/// before Cleanup ticks down `Searching`/`Suspicious` timers.
pub fn attack_reveals_attacker(
    mut events: MessageReader<DamageEvent>,
    mut awareness_query: Query<&mut Awareness>,
    turn_manager: Res<TurnManager>,
) {
    let now = turn_manager.current_time;
    for ev in events.read() {
        let Some(attacker) = ev.attacker else {
            continue;
        };
        if attacker == ev.target {
            continue;
        }
        let Ok(mut victim_awareness) = awareness_query.get_mut(ev.target) else {
            continue;
        };
        victim_awareness.set(attacker, AwarenessState::Aware, now);
    }
}

/// Bumps `SkillUseCounters::Stealth` once per turn when the player is
/// successfully unseen by at least one nearby hostile. The "use" is
/// defined as: there exists a [`Monster`] with an [`Awareness`] record
/// of the player whose state is **not** [`AwarenessState::Aware`].
/// Aware monsters don't count — once they've spotted the player,
/// continuing to hide doesn't train stealth.
///
/// Counts at most once per turn regardless of how many monsters meet
/// the condition — the `return` after the first bump caps it.
/// Scheduled in `ProcessingPhase::Cleanup` so it sees the post-
/// `awareness_tick_system` state (decayed Searching → Hidden).
///
/// If there is no player entity (e.g. the player is dead and despawned
/// mid-frame), the system early-outs without bumping.
pub fn bump_stealth_use_counter(
    player_query: Query<Entity, With<Player>>,
    hostile_awareness: Query<&Awareness, With<Monster>>,
    mut counters: ResMut<SkillUseCounters>,
) {
    let Ok(player_entity) = player_query.single() else {
        return;
    };
    for awareness in &hostile_awareness {
        let is_aware = awareness
            .get(player_entity)
            .map(|r| matches!(r.state, AwarenessState::Aware))
            .unwrap_or(false);
        if !is_aware {
            counters.bump(Skill::Stealth);
            return;
        }
    }
}

/// Game-side stealth plugin. Wires the perception/awareness pipeline
/// into the turn loop:
///
/// - `ProcessingPhase::Brain` runs `perception_tick_system` before
///   AI mode dispatch, so monsters see fresh awareness this turn.
/// - `ProcessingPhase::ResolveActions` runs the propagation +
///   attack-reveal handlers after damage events have landed.
/// - `ProcessingPhase::Cleanup` ticks down timers, decays noise, and
///   bumps the stealth use counter.
///
/// Also adds the engine's `StealthPlugin` (registers
/// `AwarenessAlertEvent`) and inserts a default-sized `NoiseMap`
/// matching the game's 80×60 `MAP_SIZE`. Future floor-materialization
/// work may replace this with a per-floor resized map.
pub struct StealthPlugin;

impl Plugin for StealthPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(roguelike_engine::stealth::StealthPlugin)
            // 80x60 matches MAP_SIZE — keep in sync if MAP_SIZE moves.
            .insert_resource(NoiseMap::new(80, 60))
            .add_systems(
                Update,
                perception_tick_system
                    .in_set(crate::game::turns::ProcessingPhase::Brain)
                    .before(crate::game::turns::monster_ai_dispatch)
                    .run_if(in_state(crate::game::AppState::InGame)),
            )
            .add_systems(
                Update,
                (squad_propagate_awareness, attack_reveals_attacker)
                    .in_set(crate::game::turns::ProcessingPhase::ResolveActions)
                    .run_if(in_state(crate::game::AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    roguelike_engine::stealth::awareness_tick_system,
                    roguelike_engine::stealth::noise_decay_system,
                    bump_stealth_use_counter,
                )
                    .in_set(crate::game::turns::ProcessingPhase::Cleanup)
                    .run_if(in_state(crate::game::AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_buckets() {
        assert_eq!(light_modifier(1.0), -3);
        assert_eq!(light_modifier(0.75), -3);
        assert_eq!(light_modifier(0.74), -1);
        assert_eq!(light_modifier(0.40), -1);
        assert_eq!(light_modifier(0.39), 2);
        assert_eq!(light_modifier(0.01), 2);
        assert_eq!(light_modifier(0.0), 3);
    }

    #[test]
    fn close_range_buckets() {
        assert_eq!(close_range_bonus(0), 2);
        assert_eq!(close_range_bonus(1), 2);
        assert_eq!(close_range_bonus(2), 1);
        assert_eq!(close_range_bonus(3), 1);
        assert_eq!(close_range_bonus(4), 0);
        assert_eq!(close_range_bonus(99), 0);
    }

    #[test]
    fn stealth_components_total_subtracts_armor() {
        let parts = StealthComponents {
            skill_half: 6,
            dex_mod: 4,
            armor_penalty: 1,
            light_mod: 2,
            noise_mod: 0,
        };
        assert_eq!(parts.total(), 11); // 6 + 4 - 1 + 2 + 0
    }

    #[test]
    fn perception_components_total_adds_all() {
        let parts = PerceptionComponents {
            base: 3,
            asleep_penalty: 0,
            close_range_bonus: 2,
        };
        assert_eq!(parts.total(), 5);
    }

    #[test]
    fn asleep_monster_carries_minus_ten() {
        let parts = PerceptionComponents {
            base: 0,
            asleep_penalty: -10,
            close_range_bonus: 0,
        };
        assert_eq!(parts.total(), -10);
    }
}
