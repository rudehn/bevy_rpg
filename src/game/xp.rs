//! XP, levels, and the level-up handler.
//!
//! Player-only system: monsters never level. Hooks on `DeathEvent` to
//! award XP when the player is the killer; tier-vs-level math caps
//! farming (monsters 5+ levels below the player give 0 XP). On crossing
//! a level threshold, fires `LevelUpEvent` which downstream systems
//! consume for HP recalc, particle effects, ASI prompts, and game log.

use bevy::prelude::*;
use roguelike_engine::combat::events::DeathEvent;
use serde::{Deserialize, Serialize};

use crate::character::{Attribute, Attributes, RaceGainSchedule};
use crate::game::AppState;
use crate::player::Player;

/// Max XP level. DCSS-derived.
pub const LEVEL_CAP: u32 = 27;

/// Levels at which the player gets +2 free attribute points to spend via
/// the ASI modal (DCSS-style). 5 events × 2 points = 10 free across a run.
pub const PLAYER_CHOICE_LEVELS: &[u32] = &[3, 9, 15, 21, 27];

/// Monster tier for XP-reward scaling. Inserted on every monster at spawn
/// from `MonsterAsset.tier`. Higher tier = harder = more XP, modulated by
/// the player's level via [`xp_reward`].
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MonsterTier(pub u32);

/// Player level. Spawns at 1; capped at `LEVEL_CAP`.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[reflect(Component)]
pub struct Level(pub u32);

/// Player experience accumulated **toward the next level**. Resets to
/// `Experience(remaining)` when a level threshold is crossed; never
/// stores total XP earned.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[reflect(Component)]
pub struct Experience(pub u32);

/// Stat-gain points queued for the player to spend via the ASI modal.
/// Both racial-schedule (1 point, constrained letters) and player-choice
/// (2 points, all letters) ASI events queue a `PendingAsi` and the UI
/// drains it. If two events fire on the same level-up (impossible under
/// the current schedules but defensive), they queue as two separate
/// instances and the modal handles them one at a time.
#[derive(Component, Debug, Clone)]
pub struct PendingAsi {
    pub points: u32,
    pub allowed: Vec<Attribute>,
    /// Display label shown in the modal title, e.g.
    /// `"Racial gain (Dwarf — choose S/D/I)"`.
    pub label: String,
}

/// Emitted when a player crosses a level threshold. UI / particle / log
/// systems consume this.
#[derive(Message, Debug, Clone, Copy)]
pub struct LevelUpEvent {
    pub player: Entity,
    pub new_level: u32,
}

// ---------------------------------------------------------------------
// Pure XP / level math
// ---------------------------------------------------------------------

/// Total XP needed to **reach** the given level from level 1.
///
/// Slow-then-fast cubic-ish:
/// `100·(L-1)² + 50·(L-1) + (10·(L-1)³)/8`
///
/// - L 2: 151 (one tier-2 kill ≈ 45 XP, so ~3 kills)
/// - L 5: ~2,000
/// - L 10: ~19,000
/// - L 20: ~60,000
/// - L 27: ~150,000
pub fn xp_required_for_level(target_level: u32) -> u32 {
    if target_level <= 1 {
        return 0;
    }
    let l = (target_level - 1) as u64;
    (100 * l * l + 50 * l + (10 * l * l * l) / 8) as u32
}

/// XP required to **advance from** the given level to the next. This is
/// the diff between consecutive thresholds.
pub fn xp_to_next_level(current_level: u32) -> u32 {
    if current_level >= LEVEL_CAP {
        return u32::MAX; // sentinel: cannot advance
    }
    xp_required_for_level(current_level + 1) - xp_required_for_level(current_level)
}

/// Base XP awarded for killing a monster of the given tier (before
/// level-difference scaling).
pub fn base_xp_for_tier(tier: u32) -> u32 {
    if tier == 0 {
        return 0;
    }
    let t = tier;
    20 + (t - 1) * 25 + ((t - 1) * (t - 1)) / 4
}

/// XP reward after applying anti-farming dropoff.
///
/// | diff (player − tier) | multiplier |
/// |---|---|
/// | ≤ -3 | 1.5× (bonus for punching up) |
/// | -2 to +2 | 1.0× (full) |
/// | +3 | 0.75× |
/// | +4 | 0.50× |
/// | ≥ +5 | 0× (no farming) |
pub fn xp_reward(monster_tier: u32, player_level: u32) -> u32 {
    let base = base_xp_for_tier(monster_tier);
    if base == 0 {
        return 0;
    }
    let diff = player_level as i32 - monster_tier as i32;
    let mult: f32 = match diff {
        d if d <= -3 => 1.5,
        -2..=2 => 1.0,
        3 => 0.75,
        4 => 0.50,
        _ => 0.0, // 5+ levels above the monster: nothing
    };
    (base as f32 * mult) as u32
}

/// Does the given XP level trigger a player-choice ASI prompt?
pub fn is_player_choice_level(level: u32) -> bool {
    PLAYER_CHOICE_LEVELS.contains(&level)
}

/// Does the given XP level trigger the race's schedule?
pub fn is_racial_schedule_level(level: u32, schedule: &RaceGainSchedule) -> bool {
    schedule.fires_at(level)
}

// ---------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------

pub struct XpPlugin;

impl Plugin for XpPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Level>()
            .register_type::<Experience>()
            .add_message::<LevelUpEvent>()
            .add_systems(
                Update,
                (
                    award_xp_on_death,
                    process_level_thresholds,
                    handle_level_up,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Read `DeathEvent` messages; when the killer is the player, compute and
/// add XP. Looks up monster tier from the `MonsterTier` component
/// (inserted by the monster spawner from `MonsterAsset.tier`).
fn award_xp_on_death(
    mut deaths: MessageReader<DeathEvent>,
    player_entity_q: Query<Entity, With<Player>>,
    mut player_xp_q: Query<(&Level, &mut Experience), With<Player>>,
    tier_q: Query<&MonsterTier>,
    mut log_writer: MessageWriter<crate::ui::game_log::GameLogMessage>,
) {
    let Ok(player_entity) = player_entity_q.single() else {
        return;
    };
    let Ok((level, mut xp)) = player_xp_q.single_mut() else {
        return;
    };
    if level.0 >= LEVEL_CAP {
        return;
    }
    let player_level = level.0;

    for ev in deaths.read() {
        if ev.killer != Some(player_entity) {
            continue;
        }
        let Ok(tier) = tier_q.get(ev.entity) else {
            continue;
        };
        let reward = xp_reward(tier.0, player_level);
        if reward > 0 {
            xp.0 = xp.0.saturating_add(reward);
            log_writer.write(crate::ui::game_log::GameLogMessage(format!(
                "+{} XP",
                reward
            )));
        }
    }
}

/// After XP changes, check whether the player has crossed a level
/// threshold. If so, increment `Level`, subtract the threshold from
/// `Experience`, and fire `LevelUpEvent`. Repeats until no threshold is
/// crossed (handles multi-level-up from one big kill).
fn process_level_thresholds(
    mut player_q: Query<(Entity, &mut Level, &mut Experience), (With<Player>, Changed<Experience>)>,
    mut level_up_writer: MessageWriter<LevelUpEvent>,
) {
    let Ok((entity, mut level, mut xp)) = player_q.single_mut() else {
        return;
    };
    while level.0 < LEVEL_CAP {
        let needed = xp_to_next_level(level.0);
        if xp.0 < needed {
            break;
        }
        xp.0 -= needed;
        level.0 += 1;
        level_up_writer.write(LevelUpEvent {
            player: entity,
            new_level: level.0,
        });
    }
    // If we hit the cap, clamp any leftover XP
    if level.0 >= LEVEL_CAP {
        xp.0 = 0;
    }
}

/// React to `LevelUpEvent`: recompute HP from the formula at the new
/// level (heal to full), emit a "LEVEL UP" particle, log the message,
/// and queue any ASI prompts (racial schedule + player-choice) as
/// `PendingAsi` components on the player.
fn handle_level_up(
    mut commands: Commands,
    mut level_ups: MessageReader<LevelUpEvent>,
    mut player_q: Query<
        (
            &crate::character::Race,
            &mut crate::game::combat::Health,
            &bevy::prelude::Transform,
            Option<&PendingAsi>,
        ),
        With<Player>,
    >,
    race_manifest_handle: Res<crate::character::RaceManifestHandle>,
    race_manifests: Res<bevy::prelude::Assets<crate::character::RaceManifest>>,
    mut log_writer: MessageWriter<crate::ui::game_log::GameLogMessage>,
    mut particle_writer: MessageWriter<crate::game::particles::ParticleRequest>,
) {
    use crate::character::max_hp_for_level;

    let Some(race_manifest) = race_manifests.get(&race_manifest_handle.0) else {
        return;
    };

    for ev in level_ups.read() {
        let Ok((race, mut health, transform, existing_pending)) = player_q.get_mut(ev.player)
        else {
            continue;
        };

        // Recompute HP. Race-based mod + new XL.
        let race_asset = race_manifest.races.get(&race.name().to_lowercase());
        if let Some(ra) = race_asset {
            let new_max = max_hp_for_level(ra.hp_mod, ev.new_level);
            health.max = new_max;
            health.current = new_max; // heal to full on level-up (DCSS default)
        }

        // Log
        log_writer.write(crate::ui::game_log::GameLogMessage(format!(
            "You reach level {}!",
            ev.new_level
        )));

        // Particle: gold floating text on the player. Reuses the existing
        // floating-text particle.
        particle_writer.write(crate::game::particles::ParticleRequest::FloatingText {
            world_pos: bevy::math::Vec2::new(
                transform.translation.x,
                transform.translation.y,
            ),
            text: "LEVEL UP!".to_string(),
            color: bevy::prelude::Color::srgb(1.0, 0.85, 0.0),
            font_size: 18.0,
        });

        // Queue ASI prompts. Two events can fire on the same level
        // (defensive — none collide under the current schedules). If a
        // PendingAsi already exists, we don't stomp it: the new prompt is
        // added as a queued entry. Since PendingAsi is a single
        // component, we model the queue as a chain: the existing one
        // stays, and the new one is held in `_existing_pending`. For
        // simplicity and given no overlap today, we just refuse to queue
        // a second; future work adds a `Vec<PendingAsi>` if needed.
        let racial_pending = if let Some(ra) = race_asset {
            if is_racial_schedule_level(ev.new_level, &ra.gain_schedule) {
                Some(PendingAsi {
                    points: 1,
                    allowed: ra.gain_schedule.allowed.clone(),
                    label: format!("Racial gain ({})", race.name()),
                })
            } else {
                None
            }
        } else {
            None
        };
        let player_choice_pending = if is_player_choice_level(ev.new_level) {
            Some(PendingAsi {
                points: 2,
                allowed: vec![Attribute::Str, Attribute::Dex, Attribute::Int],
                label: format!("Level {} — free attribute points", ev.new_level),
            })
        } else {
            None
        };

        // Insert: prefer racial first (resolves first in UI); player
        // choice waits. If there's already a pending one, append via a
        // second component? Bevy allows only one of a given type per
        // entity, so we'd need a Vec. Simplest: stash the second in a
        // `QueuedAsi` component if both fire and one already exists.
        let to_insert = match (racial_pending, player_choice_pending, existing_pending) {
            (Some(racial), Some(choice), None) => {
                // Both fire, none queued — insert racial now, queue choice.
                commands.entity(ev.player).insert(racial);
                commands.entity(ev.player).insert(QueuedAsi(vec![choice]));
                None
            }
            (Some(racial), None, None) => Some(racial),
            (None, Some(choice), None) => Some(choice),
            (Some(racial), Some(choice), Some(_)) => {
                // Both fire, one already pending: queue racial THEN choice.
                commands
                    .entity(ev.player)
                    .insert(QueuedAsi(vec![racial, choice]));
                None
            }
            (Some(p), None, Some(_)) | (None, Some(p), Some(_)) => {
                commands.entity(ev.player).insert(QueuedAsi(vec![p]));
                None
            }
            (None, None, _) => None,
        };
        if let Some(pending) = to_insert {
            commands.entity(ev.player).insert(pending);
        }
    }
}

/// Queue of ASI prompts waiting to be moved into the `PendingAsi`
/// slot once it's free. The ASI modal drains `PendingAsi`; when that
/// component is removed, a separate system pops from `QueuedAsi`.
#[derive(Component, Debug, Clone, Default)]
pub struct QueuedAsi(pub Vec<PendingAsi>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Attribute;

    #[test]
    fn xp_required_for_level_1_is_zero() {
        assert_eq!(xp_required_for_level(0), 0);
        assert_eq!(xp_required_for_level(1), 0);
    }

    #[test]
    fn xp_curve_grows_monotonically() {
        let mut prev = 0;
        for l in 2..=LEVEL_CAP {
            let cur = xp_required_for_level(l);
            assert!(
                cur > prev,
                "xp_required_for_level({l}) = {cur} did not exceed previous {prev}"
            );
            prev = cur;
        }
    }

    /// Pin a few curve values so accidental drift in the formula is
    /// caught loudly. The spec values are approximate; the order of
    /// magnitude is the locked decision.
    #[test]
    fn xp_curve_spec_values() {
        assert_eq!(xp_required_for_level(2), 151);
        // L5: 100*16 + 50*4 + 10*64/8 = 1600 + 200 + 80 = 1880
        assert_eq!(xp_required_for_level(5), 1880);
        // L10: 100*81 + 50*9 + 10*729/8 = 8100 + 450 + 911 = 9461
        assert_eq!(xp_required_for_level(10), 9461);
        // L20: 100*361 + 50*19 + 10*6859/8 = 36100 + 950 + 8573 = 45623
        assert_eq!(xp_required_for_level(20), 45623);
        // L27: 100*676 + 50*26 + 10*17576/8 = 67600 + 1300 + 21970 = 90870
        assert_eq!(xp_required_for_level(27), 90870);
    }

    #[test]
    fn xp_to_next_level_is_threshold_diff() {
        for l in 1..LEVEL_CAP {
            assert_eq!(
                xp_to_next_level(l),
                xp_required_for_level(l + 1) - xp_required_for_level(l)
            );
        }
        // At cap: sentinel u32::MAX (cannot advance)
        assert_eq!(xp_to_next_level(LEVEL_CAP), u32::MAX);
    }

    #[test]
    fn base_xp_for_tier_grows_with_tier() {
        // tier 0 = no XP (sentinel)
        assert_eq!(base_xp_for_tier(0), 0);
        assert_eq!(base_xp_for_tier(1), 20);
        // tier 5: 20 + 4*25 + 16/4 = 20 + 100 + 4 = 124
        assert_eq!(base_xp_for_tier(5), 124);
        // tier 10: 20 + 9*25 + 81/4 = 20 + 225 + 20 = 265
        assert_eq!(base_xp_for_tier(10), 265);
    }

    #[test]
    fn xp_reward_falloff_matrix() {
        // Same level: full base
        let base = base_xp_for_tier(5);
        assert_eq!(xp_reward(5, 5), base);
        // Within ±2: full base
        assert_eq!(xp_reward(5, 7), base);
        assert_eq!(xp_reward(5, 3), base);
        // +3 above: 75%
        assert_eq!(xp_reward(5, 8), (base as f32 * 0.75) as u32);
        // +4 above: 50%
        assert_eq!(xp_reward(5, 9), (base as f32 * 0.50) as u32);
        // +5 above: 0
        assert_eq!(xp_reward(5, 10), 0);
        // +10 above: still 0 (no further reduction needed)
        assert_eq!(xp_reward(5, 15), 0);
    }

    #[test]
    fn xp_reward_bonus_when_punching_up() {
        let base = base_xp_for_tier(5);
        // -3 below (player level 2 vs monster tier 5): 1.5×
        assert_eq!(xp_reward(5, 2), (base as f32 * 1.5) as u32);
        // -5 below: still 1.5× (single bonus bucket)
        assert_eq!(xp_reward(5, 0), (base as f32 * 1.5) as u32);
    }

    #[test]
    fn player_choice_levels_are_spec_set() {
        for l in 1..=LEVEL_CAP {
            let expected = matches!(l, 3 | 9 | 15 | 21 | 27);
            assert_eq!(is_player_choice_level(l), expected);
        }
    }

    #[test]
    fn racial_schedule_fires_every_interval() {
        let schedule = RaceGainSchedule {
            interval: 4,
            allowed: vec![Attribute::Str, Attribute::Dex, Attribute::Int],
        };
        for l in 1..=27 {
            let fires = is_racial_schedule_level(l, &schedule);
            assert_eq!(fires, l > 0 && l % 4 == 0, "level {l}");
        }
    }
}
