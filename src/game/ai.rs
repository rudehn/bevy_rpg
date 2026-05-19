use crate::{
    components::{Faction, FactionKind, MovementMode, Position, Viewshed},
    game::factions::{faction_hostile_to_player, FactionMatrix},
    game::magic::GameStatusEffectsExt,
    game::{
        actions::{Direction, MovementIntent, RangedAttackIntent, WaitIntent},
        combat::{DamageEvent, DamageSource, Health, HealEvent},
        magic::StatusEffects,
        ranged::RangedCapable,
        staves::{MonsterAbilities, MonsterAbilityKind},
    },
    map::{Map, tile::can_entity_enter_tile},
    player::Player,
};
use bevy::prelude::*;
use bracket_lib::prelude::{DistanceAlg, Point};
use roguelike_engine::ai::pathfinding::next_step_toward_with_mode;
use rand::rng;
use rand::seq::SliceRandom;

#[allow(dead_code)]
#[derive(Component)]
pub struct Actor {
    pub ai: Box<dyn ActorAI>,
}

#[allow(dead_code)]
pub trait ActorAI: Send + Sync {
    /// AI now directly sends events to the world instead of returning an Action enum.
    fn execute(&mut self, entity: Entity, world: &mut World);
}

// `PatrolRoute` and `PatrolState` now live in the engine crate. Re-exported
// here so existing imports like `use crate::game::ai::{PatrolRoute, PatrolState}`
// (save/mod.rs, floor_materializer.rs) and same-module references in this
// file keep working unchanged.
pub use roguelike_engine::components::{PatrolRoute, PatrolState};

// `MonsterAI`, `MonsterAIMode`, and `GUARD_PATROL_RADIUS` now live in
// the engine crate. Re-exported so existing imports like
// `use crate::game::ai::MonsterAI` continue to resolve.
pub use roguelike_engine::ai::{MonsterAI, MonsterAIMode, GUARD_PATROL_RADIUS};

/// Run the mode-update transitions for a monster. Wraps the legacy
/// `update_mode` body — gathers `AIContext` then applies the
/// visibility-driven FSM transitions (Asleep/Idle → Hunting, chase
/// tracking, chase_leash give-up, waypoint snapback).
///
/// Called by [`refresh_monster_modes_system`] every turn before any
/// AI dispatcher runs, so both GOAP and TacticBrain entities see an
/// up-to-date `MonsterAI.mode`.
pub fn refresh_monster_mode(monster_ai: &mut MonsterAI, entity: Entity, world: &mut World) {
    if let Some(ctx) = AIContext::gather(entity, world) {
        update_mode(monster_ai, entity, &ctx, world);
    }
}

/// Exclusive Bevy system: refresh every (`MonsterAI`, `MyTurn`)
/// entity's mode. Runs in `ProcessingPhase::Brain` before any AI
/// dispatcher (`goap_ai_dispatch`, `tactic_dispatch_system`) so
/// dispatchers always read the fresh mode.
///
/// This replaces the per-turn mode-update call that used to live
/// inside `execute_monster_ai` (deleted in Phase 4 of the tactic
/// registry migration).
pub fn refresh_monster_modes_system(world: &mut World) {
    let entities: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, (With<MonsterAI>, With<crate::game::turns::MyTurn>)>();
        q.iter(world).collect()
    };
    for entity in entities {
        if let Some(mut ai) = world.entity_mut(entity).take::<MonsterAI>() {
            refresh_monster_mode(&mut ai, entity, world);
            world.entity_mut(entity).insert(ai);
        }
    }
}

/// Update AI mode transitions. Awareness-driven with a viewshed
/// fast path:
///
/// 1. LOS to a hostile player → force `Aware` + `Hunting`. This
///    short-circuits the awareness tick so a monster on first sight
///    transitions in the same turn instead of one turn late.
/// 2. No LOS → mode follows `Awareness::highest()`:
///    - `Aware` → keep `Hunting` (sticky pursuit); chase-leash + reach-
///      last-known logic applies and demotes to `Idle` when the chase
///      times out or the monster reaches `last_known_player_position`.
///    - `Searching { last_known_pos }` → wake `Asleep` → `Idle`; sync
///      `last_known_player_position` from the awareness record.
///    - `Hidden` → preserve current mode (so `Asleep` keeps sleeping).
///
/// Friendly NPCs (Townsfolk, future allies) see the player but don't
/// pursue them — escalation is gated on the faction relation being
/// Hostile (`faction_hostile_to_player`).
fn update_mode(monster_ai: &mut MonsterAI, entity: Entity, ctx: &AIContext, world: &mut World) {
    use roguelike_engine::stealth::{Awareness, AwarenessState};

    let player_is_hostile_target = faction_hostile_to_player(
        world.get::<Faction>(entity),
        world.resource::<FactionMatrix>(),
    );
    if !player_is_hostile_target {
        return;
    }

    // Fast path: LOS to player → force Aware + Hunting. Writes the
    // awareness record so squad propagation + downstream systems read
    // the same source of truth.
    if ctx.is_player_visible {
        let now = world.resource::<crate::game::TurnManager>().current_time;
        if let Some(player_entity) = ctx.player_entity
            && let Some(mut awareness) = world.get_mut::<Awareness>(entity)
        {
            awareness.set(player_entity, AwarenessState::Aware, now);
        }
        monster_ai.mode = MonsterAIMode::Hunting;
        monster_ai.last_known_player_position = Some(ctx.player_point);
        monster_ai.chase_distance = 0;
        return;
    }

    // No LOS: drive mode from awareness state.
    let awareness_state = world
        .get::<Awareness>(entity)
        .map(|a| a.highest())
        .unwrap_or(AwarenessState::Hidden);

    match (monster_ai.mode, awareness_state) {
        // Hunting + still Aware after losing LOS — sticky pursuit with
        // the chase-leash + reach-last-known terminations.
        (MonsterAIMode::Hunting, AwarenessState::Aware)
        | (MonsterAIMode::Hunting, AwarenessState::Searching { .. }) => {
            // Sync last_known from the awareness record if Searching has
            // a fresher position than what we're tracking.
            if let AwarenessState::Searching { last_known_pos, .. } = awareness_state {
                monster_ai.last_known_player_position = Some(last_known_pos);
            }

            monster_ai.chase_distance += 1;

            // Land monsters give up faster on deep water last-known.
            if let Some(last_pos) = monster_ai.last_known_player_position {
                let movement_mode = world.get::<MovementMode>(entity).copied().unwrap_or_default();
                if movement_mode == MovementMode::Land {
                    let map = world.resource::<Map>();
                    let idx = map.xy_idx(last_pos.x, last_pos.y);
                    if map.tiles[idx].liquid == crate::map::tile::LiquidType::Water {
                        monster_ai.chase_distance += 2;
                    }
                }
            }

            if should_give_up_chase(monster_ai.chase_distance, monster_ai.chase_leash) {
                monster_ai.mode = MonsterAIMode::Idle;
                monster_ai.last_known_player_position = None;
                monster_ai.chase_distance = 0;
                snap_to_nearest_waypoint(entity, ctx.monster_pos, world);
                return;
            }

            if Some(ctx.monster_pos) == monster_ai.last_known_player_position {
                monster_ai.mode = MonsterAIMode::Idle;
                monster_ai.last_known_player_position = None;
                monster_ai.chase_distance = 0;
                snap_to_nearest_waypoint(entity, ctx.monster_pos, world);
            }
        }
        // Hunting decayed to Hidden (awareness timer expired) — give up.
        (MonsterAIMode::Hunting, AwarenessState::Hidden) => {
            monster_ai.mode = MonsterAIMode::Idle;
            monster_ai.last_known_player_position = None;
            monster_ai.chase_distance = 0;
            snap_to_nearest_waypoint(entity, ctx.monster_pos, world);
        }
        // Asleep + Searching → wake to Idle, sync last_known.
        (MonsterAIMode::Asleep, AwarenessState::Searching { last_known_pos, .. }) => {
            monster_ai.mode = MonsterAIMode::Idle;
            monster_ai.last_known_player_position = Some(last_known_pos);
        }
        // Asleep + Aware (e.g. just attacked) → straight to Hunting.
        (MonsterAIMode::Asleep, AwarenessState::Aware) => {
            monster_ai.mode = MonsterAIMode::Hunting;
            monster_ai.chase_distance = 0;
        }
        // Idle + Searching → keep wandering, sync last_known.
        (MonsterAIMode::Idle, AwarenessState::Searching { last_known_pos, .. }) => {
            monster_ai.last_known_player_position = Some(last_known_pos);
        }
        // Idle + Aware → escalate to Hunting (no LOS but something
        // upgraded awareness; e.g. squad alert from a sighting).
        (MonsterAIMode::Idle, AwarenessState::Aware) => {
            monster_ai.mode = MonsterAIMode::Hunting;
            monster_ai.chase_distance = 0;
        }
        // Everything else (Asleep|Idle + Hidden, future modes): preserve.
        _ => {}
    }
}

/// Returns `true` when the entity's faction relation to "Player" is
/// `Hostile`. Used to gate Asleep/Idle→Hunting transitions so allied
/// or neutral NPCs (Townsfolk drunks, fishermen, future vendors)
/// never pursue the player even when the player is in their FOV.
///
// ---------------------------------------------------------------------------
// AI helper functions — extracted from MonsterAI::execute() for readability
// ---------------------------------------------------------------------------

/// Read-only snapshot of the world state needed for AI decisions.
struct AIContext {
    monster_pos: Point,
    player_point: Point,
    player_entity: Option<Entity>,
    is_player_visible: bool,
}

impl AIContext {
    fn gather(entity: Entity, world: &mut World) -> Option<Self> {
        let monster_pos = world.get::<Position>(entity)?.to_point();
        let mut viewshed = world.get::<Viewshed>(entity).cloned().unwrap_or_default();

        // If the viewshed hasn't been computed yet, calculate it now so the AI
        // doesn't skip its first turn due to an empty visible_tiles.
        // NOTE: We only check `dirty` here, NOT `visible_tiles.is_empty()`.
        // A monster in a sealed room may legitimately have an empty viewshed
        // after computation; re-checking is_empty() would force an expensive
        // field_of_view() call every single turn for such monsters.
        if viewshed.dirty {
            let map = world.resource::<Map>();
            viewshed.visible_tiles =
                bracket_lib::prelude::field_of_view(monster_pos, viewshed.range, map)
                    .into_iter()
                    .collect();
            viewshed.dirty = false;
            // Write the computed viewshed back to the entity.
            if let Some(mut vs) = world.get_mut::<Viewshed>(entity) {
                vs.visible_tiles = viewshed.visible_tiles.clone();
                vs.dirty = false;
            }
        }

        let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
        let (player_entity, player_point) = match player_query.iter(world).next() {
            Some((e, p)) => (Some(e), p.to_point()),
            None => return None,
        };
        let is_player_visible = viewshed.visible_tiles.contains(&player_point);

        Some(AIContext { monster_pos, player_point, player_entity, is_player_visible })
    }
}

/// Snap waypoint patrols to the nearest waypoint after a hunt ends.
fn snap_to_nearest_waypoint(entity: Entity, monster_pos: Point, world: &mut World) {
    if let Some(mut patrol) = world.get_mut::<PatrolRoute>(entity)
        && let PatrolState::Waypoint { ref points, ref mut current_index } = patrol.state
            && !points.is_empty() {
                *current_index = points.iter().enumerate()
                    .min_by_key(|(_, p)| (p.0 - monster_pos.x).abs() + (p.1 - monster_pos.y).abs())
                    .map(|(i, _)| i)
                    .unwrap_or(0);
            }
}

/// Try to use a monster ability. Returns `true` if one was fired.
/// Public entry point for the tactic adapter's `UseAbility` handler.
pub fn try_use_ability_world(entity: Entity, world: &mut World) -> bool {
    let pos = world.get::<Position>(entity).map(|p| p.to_point()).unwrap_or(Point::new(0, 0));
    let player_entity = {
        let mut q = world.query_filtered::<Entity, With<Player>>();
        q.iter(world).next()
    };
    try_use_ability(entity, pos, player_entity, world)
}

fn try_use_ability(entity: Entity, monster_pos: Point, player_entity: Option<Entity>, world: &mut World) -> bool {
    // Read abilities
    let abilities = world.get::<MonsterAbilities>(entity).cloned();
    let Some(abilities) = abilities else { return false; };

    let caster_faction = world.get::<Faction>(entity).map(|f| f.0.clone());
    let caster_health = world.get::<Health>(entity).map(|h| (h.current, h.max));
    let caster_effects = world.get::<StatusEffects>(entity).cloned();
    let caster_name = world.get::<crate::components::Name>(entity)
        .map(|n| n.0.clone())
        .unwrap_or_else(|| "Something".to_string());

    let faction_matrix = world.resource::<FactionMatrix>().clone();

    // Find nearest enemy
    let nearest_enemy: Option<(Entity, Point)> = {
        let mut query = world.query::<(Entity, &Position, &Faction, &Health)>();
        let mut best: Option<(Entity, Point, i32)> = None;
        for (ent, pos, faction, _health) in query.iter(world) {
            if ent == entity { continue; }
            if let Some(ref cf) = caster_faction {
                if !faction_matrix.is_hostile_to(&cf.0, &faction.0.0) { continue; }
            }
            let dist = (pos.x - monster_pos.x).abs() + (pos.y - monster_pos.y).abs();
            if best.is_none() || dist < best.as_ref().unwrap().2 {
                best = Some((ent, pos.to_point(), dist));
            }
        }
        best.map(|(e, p, _)| (e, p))
    };

    // Find most-wounded ally for heals
    let most_wounded_ally: Option<Entity> = {
        let mut query = world.query::<(Entity, &Position, &Faction, &Health)>();
        let mut best: Option<(Entity, i32)> = None;
        for (ent, _pos, faction, health) in query.iter(world) {
            if ent == entity { continue; }
            if let Some(ref cf) = caster_faction {
                if !faction_matrix.is_allied_to(&cf.0, &faction.0.0) { continue; }
            }
            let missing = health.max - health.current;
            if missing > 0 && (best.is_none() || missing > best.as_ref().unwrap().1) {
                best = Some((ent, missing));
            }
        }
        best.map(|(e, _)| e)
    };

    let hp_pct = caster_health.map(|(c, m)| c as f32 / m.max(1) as f32).unwrap_or(1.0);

    for (idx, ability) in abilities.0.iter().enumerate() {
        if ability.current_cooldown > 0 { continue; }

        match &ability.kind {
            MonsterAbilityKind::Bolt { dice, damage_type } => {
                let Some((target_entity, target_pos)) = nearest_enemy else { continue; };
                let dist = (target_pos.x - monster_pos.x).abs() + (target_pos.y - monster_pos.y).abs();
                if dist > ability.range as i32 { continue; }

                let mut rng = bracket_lib::random::RandomNumberGenerator::new();
                let damage = crate::game::staves::roll_dice_expr(&mut rng, dice).max(1);
                world.write_message(DamageEvent {
                    attacker: Some(entity),
                    target: target_entity,
                    amount: damage,
                    damage_type: *damage_type,
                    source: DamageSource::Spell,
                    armor: 0,
                });
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                // Set cooldown
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
            MonsterAbilityKind::Heal { dice } => {
                // Heal self if low HP, or heal ally
                let heal_self = hp_pct < 0.6;
                let target = if heal_self {
                    Some(entity)
                } else {
                    most_wounded_ally
                };
                let Some(target) = target else { continue; };

                let mut rng = bracket_lib::random::RandomNumberGenerator::new();
                let amount = crate::game::staves::roll_dice_expr(&mut rng, dice).max(1);
                world.write_message(HealEvent { target, amount, source: Some(entity) });
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
            MonsterAbilityKind::ApplyStatus { effect, duration } => {
                let Some((target_entity, target_pos)) = nearest_enemy else { continue; };
                let dist = (target_pos.x - monster_pos.x).abs() + (target_pos.y - monster_pos.y).abs();
                if dist > ability.range as i32 { continue; }

                if let Some(mut effects) = world.get_mut::<StatusEffects>(target_entity) {
                    effects.add_effect(*effect, *duration);
                }
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
            MonsterAbilityKind::SelfBuff { effect, duration } => {
                // Check if already have this effect
                let already_has = caster_effects.as_ref().map(|e| {
                    e.effects.iter().any(|ae| std::mem::discriminant(&ae.kind) == std::mem::discriminant(effect))
                }).unwrap_or(false);
                if already_has { continue; }

                if let Some(mut effects) = world.get_mut::<StatusEffects>(entity) {
                    effects.add_effect(*effect, *duration);
                }
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
            MonsterAbilityKind::Summon { monster, count } => {
                // Summon allied monsters
                let caster_pos_comp = world.get::<Position>(entity).cloned();
                let Some(caster_pos_comp) = caster_pos_comp else { continue; };
                world.insert_resource(crate::game::magic::PendingSummon {
                    caster_pos: caster_pos_comp,
                    caster_label: caster_name.clone(),
                    monster_name: monster.clone(),
                    count: *count,
                    caster_entity: None,
                    squad_id: None,
                });
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
            MonsterAbilityKind::SummonCapped { weights, max_summons } => {
                let current_count = crate::game::magic::count_active_summons(entity, world);
                if current_count >= *max_summons { continue; }

                let caster_pos_comp = world.get::<Position>(entity).cloned();
                let Some(caster_pos_comp) = caster_pos_comp else { continue; };

                let squad_id = world.get::<crate::game::squad::SquadId>(entity).copied();

                let mut rng = bracket_lib::random::RandomNumberGenerator::new();
                let monster_name = crate::game::magic::pick_weighted_monster(weights, &mut rng);

                world.insert_resource(crate::game::magic::PendingSummon {
                    caster_pos: caster_pos_comp,
                    caster_label: caster_name.clone(),
                    monster_name,
                    count: 1,
                    caster_entity: Some(entity),
                    squad_id,
                });
                world.write_message(crate::ui::game_log::GameLogMessage(format!(
                    "{} casts {}!", caster_name, ability.name
                )));
                if let Some(mut ma) = world.get_mut::<MonsterAbilities>(entity) {
                    ma.0[idx].current_cooldown = ability.cooldown;
                }
                return true;
            }
        }
    }
    false
}

// FSM-era helpers (`try_ranged_attack`, `resolve_squad_leash`,
// `resolve_movement`, `try_flee_movement`, `try_stun_skip`,
// `pathfind_toward`, `has_adjacent_enemy`, `idle_movement`) were
// deleted in Phases 4–5. Their behaviour moved into tactics:
// `RangedAttack`, `SquadLeash`, `MeleeAdjacent`/`HuntVisibleTarget`/
// `PursueLastKnownPosition`/`FreeWander`, `FleePanicked`/`FleeAtLowHp`/
// `KiteRetreat`, `SubmergeOrSurface`. The stun/entangle short-circuit
// is now `resolve::maybe_skip_turn`. See `docs/design/TACTICS.md`.

// =====================================================================
// Pure decision helpers (re-exported)
// =====================================================================
//
// `roguelike_engine::ai::decisions` is the canonical home for the pure
// "should I flee / kite / give up chase / move erratically" helpers.
// Re-exported here so tactic implementations can `use crate::game::ai::*`
// and grab the named decisions. Mode-update logic still calls
// `should_give_up_chase` and `flee_direction` via these re-exports.
pub use roguelike_engine::ai::decisions::{
    flee_direction, should_flee, should_give_up_chase, should_kite_retreat,
    should_move_erratically,
};
