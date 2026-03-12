use std::collections::HashMap;

use crate::components::{Faction, Player, Position};
use crate::core::{Health, MonsterAI, Viewshed, get_entities_in_vision};
use crate::events::{
    Effect, Effects, EventType, MultiTarget, SingleTarget, Targets, add_event, aoe_tiles,
};
use crate::map::Map;
use crate::raws::{Reaction, faction_reaction};
use bracket_lib::prelude::{BaseMap, DistanceAlg, Point};
use serde::Deserialize;
use shipyard::{AllStoragesViewMut, Component, EntityId, Get, IntoIter, UniqueView, View, ViewMut};

#[derive(Deserialize, Debug, Clone, Default)]
pub enum AbilityTarget {
    Castor, // Only can be casted on the castor, like a Heal Self ability
    Tile,
    Entity, // Any entity besides the castor
    #[default]
    EntityOrSelf, // Entity { max_range: i32, requires_los: bool },
            // Tile { radius: i32, affects_allies: bool, requires_los: bool },
}

#[derive(Deserialize, Debug, Clone)]
pub struct KnownAbility {
    pub name: String,
    pub cooldown: i32,
    #[serde(default)]
    pub target: AbilityTarget,
    #[serde(default)] // Defaults to 0
    pub current_cd: i32,
    pub range: i32,
    pub min_range: Option<i32>,
    #[serde(default)]
    pub radius: i32,

    pub effects: Effects,
}

#[derive(Component, Debug, Clone)]
pub struct KnownAbilities {
    pub abilities: Vec<KnownAbility>,
}

/// Chooses the most effective ability and target for a monster AI entity.
///
/// This function evaluates all known abilities that are off cooldown and visible targets,
/// scoring each potential use case to determine which ability would have the highest overall impact.
///
/// Caching is used to avoid recalculating ability impact for the same (monster, ability, target) combinations.
///
/// Returns the chosen ability name and target if any ability is worth using.
pub fn choose_ability(
    monster: &MonsterAI,
    all_storages: &AllStoragesViewMut,
) -> Option<(KnownAbility, SingleTarget)> {
    let mut known_abilities = all_storages.borrow::<ViewMut<KnownAbilities>>().unwrap();
    let viewsheds = all_storages.borrow::<View<Viewshed>>().unwrap();
    let factions = all_storages.borrow::<View<Faction>>().unwrap();
    let positions = all_storages.borrow::<View<Position>>().unwrap();
    let map = all_storages.borrow::<UniqueView<Map>>().unwrap();

    let monster_entity = monster.get_entity();
    let monster_pos = positions.get(monster_entity).unwrap();

    if let Ok(mut abilities) = (&mut known_abilities).get(monster_entity) {
        let visible_targets = get_entities_in_vision(
            &viewsheds,
            &factions,
            &positions,
            &map,
            monster_entity,
            true,
        )
        .into_iter()
        .map(|(entity, dist)| (entity, dist.round() as usize))
        .collect();
        let visible_tiles = if let Ok(v) = viewsheds.get(monster.get_entity()) {
            &v.visible_tiles
        } else {
            &vec![]
        };

        // Cache to prevent re-evaluating ability effects for the same entity
        let mut impact_cache: HashMap<(EntityId, String, EntityId), i32> = HashMap::new();
        let mut best: Option<(KnownAbility, SingleTarget, i32)> = None;

        for ability in abilities.abilities.iter_mut().filter(|a| a.current_cd == 0) {
            println!("Evaluating {}", ability.name);
            let candidate = evaluate_best_target_for_ability(
                monster,
                ability,
                &positions,
                &factions,
                &map,
                monster_entity,
                monster_pos,
                &visible_targets,
                visible_tiles,
                all_storages,
                &mut impact_cache,
            );

            if let Some((target, score)) = candidate {
                println!("got score {}", score);
                if score > 0 && (best.is_none() || score > best.as_ref().unwrap().2) {
                    best = Some((ability.clone(), target, score));
                }
            }
        }

        if let Some((ability, target, _)) = best {
            if let Some(a) = abilities
                .abilities
                .iter_mut()
                .find(|a| a.name == ability.name)
            {
                a.current_cd = a.cooldown;
            }
            return Some((ability, target));
        }
    }

    None
}

/// Determines the best possible target for a given ability.
///
/// Dispatches logic to specialized evaluators based on the ability's target type (caster, entity, or tile).
fn evaluate_best_target_for_ability(
    monster: &MonsterAI,
    ability: &KnownAbility,
    positions: &View<Position>,
    factions: &View<Faction>,
    map: &UniqueView<Map>,
    monster_entity: EntityId,
    monster_pos: &Position,
    visible_targets: &Vec<(EntityId, usize)>,
    visible_tiles: &Vec<Point>,
    all_storages: &AllStoragesViewMut,
    impact_cache: &mut HashMap<(EntityId, String, EntityId), i32>,
) -> Option<(SingleTarget, i32)> {
    match ability.target {
        AbilityTarget::Castor => {
            let total_score = evaluate_with_aoe(
                monster,
                ability,
                monster_entity,
                monster_pos,
                positions,
                factions,
                all_storages,
                impact_cache,
            );
            if total_score > 0 {
                Some((
                    SingleTarget::Entity {
                        target: monster_entity,
                    },
                    total_score,
                ))
            } else {
                None
            }
        }
        AbilityTarget::Entity => evaluate_entity_targets(
            monster,
            ability,
            positions,
            factions,
            visible_targets,
            all_storages,
            impact_cache,
            false,
        ),
        AbilityTarget::Tile => evaluate_tile_targets(
            monster,
            ability,
            positions,
            factions,
            map,
            monster_pos,
            visible_tiles,
            all_storages,
            impact_cache,
        ),
        AbilityTarget::EntityOrSelf => evaluate_entity_targets(
            monster,
            ability,
            positions,
            factions,
            visible_targets,
            all_storages,
            impact_cache,
            true,
        ),
    }
}

/// Evaluates the impact of an ability on a target entity, including AOE splash effects.
///
/// Returns the total impact score for casting this ability at the specified target.
fn evaluate_with_aoe(
    monster: &MonsterAI,
    ability: &KnownAbility,
    target_entity: EntityId,
    target_pos: &Position,
    positions: &View<Position>,
    factions: &View<Faction>,
    all_storages: &AllStoragesViewMut,
    impact_cache: &mut HashMap<(EntityId, String, EntityId), i32>,
) -> i32 {
    let mut total = evaluate_cached(monster, ability, target_entity, all_storages, impact_cache);

    if ability.radius > 0 {
        for e in entities_in_radius(positions, factions, target_pos, ability.radius) {
            total += evaluate_cached(monster, ability, e, all_storages, impact_cache);
        }
    }

    total
}

/// Evaluates and scores all visible entities to determine the best one to target with an entity-based ability.
///
/// Considers both direct impact and AOE splash around each potential target.
fn evaluate_entity_targets(
    monster: &MonsterAI,
    ability: &KnownAbility,
    positions: &View<Position>,
    factions: &View<Faction>,
    visible_targets: &Vec<(EntityId, usize)>,
    all_storages: &AllStoragesViewMut,
    impact_cache: &mut HashMap<(EntityId, String, EntityId), i32>,
    can_target_self: bool,
) -> Option<(SingleTarget, i32)> {
    let mut best: Option<(EntityId, i32)> = None;

    for (target_entity, dist) in visible_targets {
        if monster.get_entity() == *target_entity && !can_target_self {
            continue;
        }
        let dist = *dist as i32;
        if dist < ability.min_range.unwrap_or(0) || dist > ability.range {
            continue;
        }

        let mut total_score =
            evaluate_cached(monster, ability, *target_entity, all_storages, impact_cache);

        if ability.radius > 0 {
            if let Ok(target_pos) = positions.get(*target_entity) {
                for e in entities_in_radius(positions, factions, target_pos, ability.radius) {
                    total_score += evaluate_cached(monster, ability, e, all_storages, impact_cache);
                }
            }
        }

        if total_score > 0 && (best.is_none() || total_score > best.as_ref().unwrap().1) {
            best = Some((*target_entity, total_score));
        }
    }

    best.map(|(e, s)| (SingleTarget::Entity { target: e }, s))
}

/// Evaluates and scores all visible tiles to determine the best location for a tile-targeted ability.
///
/// Calculates total impact for entities affected within the AOE radius of each visible tile.
fn evaluate_tile_targets(
    monster: &MonsterAI,
    ability: &KnownAbility,
    positions: &View<Position>,
    factions: &View<Faction>,
    map: &UniqueView<Map>,
    monster_pos: &Position,
    visible_tiles: &Vec<Point>,
    all_storages: &AllStoragesViewMut,
    impact_cache: &mut HashMap<(EntityId, String, EntityId), i32>,
) -> Option<(SingleTarget, i32)> {
    let mut best: Option<(usize, i32)> = None;

    for tile in visible_tiles {
        let tile_idx = map.xy_idx(tile.x, tile.y);
        let dist =
            map.get_pathing_distance(map.xy_idx(monster_pos.x, monster_pos.y), tile_idx) as i32;
        if dist < ability.min_range.unwrap_or(0) || dist > ability.range {
            continue;
        }

        let entities = entities_in_radius(
            positions,
            factions,
            &Position {
                x: tile.x,
                y: tile.y,
            },
            ability.radius,
        );
        let mut score = score_single_tile(ability, tile_idx, all_storages);

        for e in entities {
            score += evaluate_cached(monster, ability, e, all_storages, impact_cache);
        }

        if score > 0 && (best.is_none() || score > best.as_ref().unwrap().1) {
            best = Some((tile_idx, score));
        }
    }

    best.map(|(idx, s)| (SingleTarget::Tile { tile_idx: idx }, s))
}

/// Evaluates and caches the impact score of an ability on a specific entity.
///
/// Uses a cache key of `(monster_id, ability_name, target_id)` to avoid redundant evaluations.
fn evaluate_cached(
    monster: &MonsterAI,
    ability: &KnownAbility,
    entity: EntityId,
    all_storages: &AllStoragesViewMut,
    cache: &mut HashMap<(EntityId, String, EntityId), i32>,
) -> i32 {
    let key = (monster.get_entity(), ability.name.clone(), entity);
    if let Some(&cached) = cache.get(&key) {
        cached
    } else {
        let score = evaluate_ability_impact(monster, ability, entity, all_storages);
        cache.insert(key, score);
        score
    }
}

fn entities_in_radius(
    positions: &View<Position>,
    factions: &View<Faction>,
    center: &Position,
    radius: i32,
) -> Vec<EntityId> {
    (positions, factions)
        .iter()
        .with_id()
        .filter_map(|(entity, (pos, _faction))| {
            let dist = (pos.x - center.x).abs() + (pos.y - center.y).abs();
            if dist <= radius { Some(entity) } else { None }
        })
        .collect()
}

/// Evaluate total impact of a ability on a target (including AoE and friendly fire)
fn evaluate_ability_impact(
    monster: &MonsterAI,
    ability: &KnownAbility,
    target_entity: EntityId,
    all_storages: &AllStoragesViewMut,
) -> i32 {
    let factions = all_storages.borrow::<View<Faction>>().unwrap();
    let positions = all_storages.borrow::<View<Position>>().unwrap();

    let monster_faction = factions.get(monster.get_entity()).unwrap();
    let target_pos = positions.get(target_entity).unwrap();

    let mut total_score = 0;

    if ability.radius > 0 {
        // AoE ability: sum score across all entities within radius
        for (entity, (_faction, pos)) in (&factions, &positions).iter().with_id() {
            let dist = DistanceAlg::Pythagoras.distance2d(
                Point::new(target_pos.x, target_pos.y),
                Point::new(pos.x, pos.y),
            ) as i32;

            if dist <= ability.radius {
                total_score += score_single_target(
                    ability,
                    monster.get_entity(),
                    monster_faction,
                    entity,
                    all_storages,
                );
            }
        }
    } else {
        // Single-target ability
        total_score = score_single_target(
            ability,
            monster.get_entity(),
            monster_faction,
            target_entity,
            all_storages,
        );
    }

    total_score
}

/// Score impact on a single target, considering allies/enemies
fn score_single_target(
    ability: &KnownAbility,
    monster_entity: EntityId,
    monster_faction: &Faction,
    target_entity: EntityId,
    all_storages: &AllStoragesViewMut,
) -> i32 {
    let healths = all_storages.borrow::<View<Health>>().unwrap();
    let factions = all_storages.borrow::<View<Faction>>().unwrap();

    let target_faction = factions.get(target_entity).unwrap();
    let reaction = faction_reaction(&monster_faction.name, &target_faction.name);
    let mut score: i32 = 0;
    for effect in ability.effects.effects.iter() {
        match effect {
            Effect::Heal { amount } => {
                match reaction {
                    Reaction::Ally => {
                        if let Ok(target_hp) = healths.get(target_entity) {
                            // High weight for healing so it's prioritized
                            score += ((target_hp.max - target_hp.current).min(*amount) * 50) as i32;
                        }
                    }
                    Reaction::Attack | Reaction::Flee => score -= 50,
                    _ => {}
                }
            }
            Effect::Damage { amount, kind } => {
                // TODO - consider damage vulnerability / resistances
                match reaction {
                    Reaction::Ally => score -= *amount,
                    Reaction::Attack | Reaction::Flee => {
                        if let Ok(target_hp) = healths.get(target_entity) {
                            score += target_hp.current.min(*amount) as i32;
                        }
                    }
                    _ => {}
                }
            }
            Effect::Haste => match reaction {
                Reaction::Ally => {
                    score += 10;
                }
                Reaction::Attack | Reaction::Flee => score -= 50,
                _ => {}
            },
            Effect::Slow => match reaction {
                Reaction::Ally => {
                    score -= 50;
                }
                Reaction::Attack | Reaction::Flee => score += 10,
                _ => {}
            },
            Effect::Poison { .. } => {}
            Effect::MagicMapping => {}
            Effect::Enchant => {}
            Effect::Summon { .. } => {}
            Effect::Reanimate { .. } => {}
            Effect::SplitEntity => {}
            Effect::Lifesteal { amount } => match reaction {
                Reaction::Ally => {
                    score -= 50;
                }
                Reaction::Attack | Reaction::Flee => {
                    if let Ok(target_hp) = healths.get(target_entity) {
                        score += target_hp.current.min(*amount) as i32;
                    }
                    if let Ok(target_hp) = healths.get(monster_entity) {
                        score += (target_hp.max - target_hp.current).min(*amount) as i32;
                    }
                }
                _ => {}
            },
            Effect::AttributeIncrease { .. } => {}
            Effect::Particle { .. } => {}
            Effect::ParticleLine { .. } => {}
            Effect::ParticleProjectile { .. } => {}
        }
    }

    score
}

/// Score impact on a single tile, considering allies/enemies
/// Effects that hit an entity are considered with the score_single_target function
/// This only concerns tile activity, like summoning creatures
fn score_single_tile(
    ability: &KnownAbility,
    target_idx: usize,
    all_storages: &AllStoragesViewMut,
) -> i32 {
    let mut score: i32 = 0;
    for effect in ability.effects.effects.iter() {
        match effect {
            Effect::Heal { .. } => {}
            Effect::Damage { .. } => {}
            Effect::Haste => {}
            Effect::Slow => {}
            Effect::Poison { .. } => {}
            Effect::MagicMapping => {}
            Effect::Enchant => {}
            Effect::Summon { name, amount } => {
                // TODO - weight by proximity to enemy - prefer summoning near enemy?
                // TODO - weight by what is being summoned
                score += 10 * amount
            }
            Effect::Reanimate { .. } => {}
            Effect::SplitEntity => {}
            Effect::Lifesteal { .. } => {}
            Effect::AttributeIncrease { .. } => {}
            Effect::Particle { .. } => {}
            Effect::ParticleLine { .. } => {}
            Effect::ParticleProjectile { .. } => {}
        }
    }

    score
}
