use bevy::prelude::*;
use bracket_lib::prelude::{DistanceAlg, Point};

use crate::{
    components::{Chest, Faction, FloorEntityMarker, InInventory, Inventory, Item, Monster, Position, Viewshed},
    game::{
        actions::{ActionFinishedEvent, MeleeIntent, MovementIntent, OpenChestIntent, PickUpIntent, WaitIntent},
        ai::{idle_movement, pathfind_toward, try_flee_movement},
        combat::Health,
        factions::FactionMatrix,
    },
    map::Map,
    player::Player,
};

use super::GoapAI;
use super::planner::WorldState;

pub(super) fn gather_world_state(entity: Entity, ai: &GoapAI, world: &mut World) -> WorldState {
    // Snapshot all entity data, then snapshot resources, releasing borrows before queries.
    let pos = world.get::<Position>(entity).map(|p| p.to_point()).unwrap_or(Point::new(0, 0));
    let visible_tiles: std::collections::HashSet<Point> = world.get::<Viewshed>(entity)
        .map(|v| v.visible_tiles.clone())
        .unwrap_or_default();
    let actor_faction = world.get::<Faction>(entity).cloned();
    let hp_low = world.get::<Health>(entity)
        .is_some_and(|h| h.max > 0 && (h.current as f32 / h.max as f32) < 0.3);
    let carrying_items = world.get::<Inventory>(entity)
        .is_some_and(|inv| !inv.items.is_empty());
    // Clone the faction matrix into a fully-owned value so world is not borrowed.
    let faction_matrix: FactionMatrix = world.resource::<FactionMatrix>().clone();

    let player_pos: Option<Point> = {
        let mut q = world.query_filtered::<&Position, With<Player>>();
        q.iter(world).next().map(|p| p.to_point())
    };
    let player_visible = player_pos
        .map(|pp| visible_tiles.contains(&pp))
        .unwrap_or(false);

    let adjacent_to_threat = player_pos
        .map(|pp| DistanceAlg::Chebyshev.distance2d(pos, pp) <= 1.5)
        .unwrap_or(false)
        && player_visible;

    // Has escape route (needs &mut World for pathfinding)
    let has_escape_route = if adjacent_to_threat {
        player_pos
            .map(|pp| try_flee_movement(entity, pos, pp, world).is_some())
            .unwrap_or(true)
    } else {
        true
    };

    let at_hoard = ai.hoard_position
        .map(|hp| pos == hp)
        .unwrap_or(false);

    // Scan for visible floor items and chests.
    // Exclude items at the hoard position to prevent pick-up/drop feedback loops.
    let hoard_pt = ai.hoard_position;
    let mut item_visible = false;
    let mut adjacent_to_item = false;
    let mut adjacent_to_chest = false;

    {
        let mut item_query = world.query_filtered::<&Position, (With<Item>, Without<InInventory>)>();
        for item_pos in item_query.iter(world) {
            let ipt = item_pos.to_point();
            // Skip items sitting at the hoard — those are already "delivered."
            if hoard_pt.is_some_and(|hp| ipt == hp) {
                continue;
            }
            if visible_tiles.contains(&ipt) {
                item_visible = true;
                if pos == ipt {
                    adjacent_to_item = true;
                }
            }
        }
    }
    {
        let mut chest_query = world.query_filtered::<&Position, With<Chest>>();
        for chest_pos in chest_query.iter(world) {
            let cpt = chest_pos.to_point();
            if visible_tiles.contains(&cpt) {
                item_visible = true;
                if DistanceAlg::Chebyshev.distance2d(pos, cpt) <= 1.5 {
                    adjacent_to_chest = true;
                }
            }
        }
    }

    // Hostile nearby
    let hostile_nearby = if let Some(af) = &actor_faction {
        let mut entity_query = world.query::<(&Position, &Faction)>();
        entity_query.iter(world).any(|(epos, efaction)| {
            let ept = epos.to_point();
            ept != pos && visible_tiles.contains(&ept) && faction_matrix.is_hostile_to(&af.0.0, &efaction.0.0)
        })
    } else {
        false
    };

    // --- Squad-derived state ---
    use crate::game::squad::{Morale, SquadBlackboard, SquadId, SquadLeader};

    let self_morale_low = world.get::<Morale>(entity)
        .map(|m| m.0 < 0.3)
        .unwrap_or(false);

    // Find this entity's squad blackboard (on the leader)
    let squad_id = world.get::<SquadId>(entity).copied();
    let (squad_retreating, near_leader) = if let Some(sid) = squad_id {
        let mut bb_query = world.query_filtered::<(&SquadId, &SquadBlackboard, &Position), With<SquadLeader>>();
        let bb_data = bb_query.iter(world)
            .find(|(leader_sid, _, _)| **leader_sid == sid)
            .map(|(_, bb, leader_pos)| (bb.retreat_ordered, leader_pos.to_point()));

        match bb_data {
            Some((retreating, leader_pt)) => {
                let near = DistanceAlg::Chebyshev.distance2d(pos, leader_pt) <= 4.0;
                (retreating, near)
            }
            None => (false, false),
        }
    } else {
        (false, false)
    };

    // Check if an ally is between us and the player
    let ally_between_self_and_threat = if let Some(pp) = player_pos {
        let self_dist = DistanceAlg::Chebyshev.distance2d(pos, pp);
        let mut ally_query = world.query_filtered::<&Position, With<Monster>>();
        ally_query.iter(world).any(|apos| {
            let apt = apos.to_point();
            apt != pos && DistanceAlg::Chebyshev.distance2d(apt, pp) < self_dist
        })
    } else {
        false
    };

    WorldState {
        player_visible,
        hostile_nearby,
        hp_low,
        has_escape_route,
        adjacent_to_threat,
        carrying_items,
        at_hoard,
        item_visible,
        adjacent_to_item,
        adjacent_to_chest,
        squad_retreating,
        near_leader,
        self_morale_low,
        can_cast_useful_spell: {
            // Check if entity has any monster ability off cooldown
            world.get::<crate::game::staves::MonsterAbilities>(entity)
                .map(|ma| ma.0.iter().any(|a| a.current_cooldown == 0))
                .unwrap_or(false)
        },
        ally_between_self_and_threat,
    }
}

pub(super) fn dispatch_action(entity: Entity, action_name: &str, ai: &mut GoapAI, world: &mut World) {
    let pos = world.get::<Position>(entity).map(|p| p.to_point()).unwrap_or(Point::new(0, 0));

    // Any non-roam action clears the roam target.
    if action_name != "roam" {
        ai.roam_target = None;
    }

    match action_name {
        "flee" => {
            let mut player_query = world.query_filtered::<&Position, With<Player>>();
            let player_pos = player_query.iter(world).next().map(|p| p.to_point());
            if let Some(pp) = player_pos {
                if let Some(intent) = try_flee_movement(entity, pos, pp, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "attack" => {
            // Attack the nearest adjacent hostile (usually the player).
            let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
            let target = player_query.iter(world).next()
                .filter(|(_, pp)| DistanceAlg::Chebyshev.distance2d(pos, pp.to_point()) <= 1.5)
                .map(|(e, _)| e);
            if let Some(target) = target {
                world.write_message(MeleeIntent { attacker: entity, target });
            } else {
                world.write_message(WaitIntent { entity });
            }
        }

        "seek_item" => {
            // Pathfind toward nearest visible floor item or chest.
            let viewshed = world.get::<Viewshed>(entity).cloned();
            let target = find_nearest_loot(entity, pos, ai.hoard_position, viewshed.as_ref(), world);
            if let Some(target_pt) = target {
                if let Some(intent) = pathfind_toward(entity, pos, target_pt, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "pick_up_item" => {
            world.write_message(PickUpIntent { entity });
        }

        "open_chest" => {
            // Find the nearest adjacent chest and emit OpenChestIntent.
            let chest_entity = {
                let mut chest_query = world.query_filtered::<(Entity, &Position), With<Chest>>();
                chest_query.iter(world)
                    .filter(|(_, cp)| DistanceAlg::Chebyshev.distance2d(pos, cp.to_point()) <= 1.5)
                    .min_by_key(|(_, cp)| {
                        let d = DistanceAlg::Pythagoras.distance2d(pos, cp.to_point());
                        (d * 100.0) as i32
                    })
                    .map(|(e, _)| e)
            };
            if let Some(chest) = chest_entity {
                world.write_message(OpenChestIntent { entity, chest_entity: chest });
            } else {
                world.write_message(WaitIntent { entity });
            }
        }

        "return_to_hoard" => {
            if let Some(hoard_pos) = ai.hoard_position {
                if let Some(intent) = pathfind_toward(entity, pos, hoard_pos, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "drop_items" => {
            world.write_message(super::DropAtHoardMessage { entity });
        }

        // --- Goblin squad actions ---

        "attack_melee" | "attack" => {
            let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
            let target = player_query.iter(world).next()
                .filter(|(_, pp)| DistanceAlg::Chebyshev.distance2d(pos, pp.to_point()) <= 1.5)
                .map(|(e, _)| e);
            if let Some(target) = target {
                world.write_message(MeleeIntent { attacker: entity, target });
            } else {
                world.write_message(WaitIntent { entity });
            }
        }

        "engage_enemy" => {
            // Pathfind toward the player.
            let mut player_query = world.query_filtered::<&Position, With<Player>>();
            let target = player_query.iter(world).next().map(|p| p.to_point());
            if let Some(target_pt) = target {
                if let Some(intent) = pathfind_toward(entity, pos, target_pt, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "move_to_leader" | "command_position" => {
            // Pathfind toward the squad leader.
            use crate::game::squad::{SquadId, SquadLeader};
            let leader_pos = world.get::<SquadId>(entity).copied().and_then(|sid| {
                let mut q = world.query_filtered::<(&SquadId, &Position), With<SquadLeader>>();
                q.iter(world)
                    .find(|(lsid, _)| **lsid == sid)
                    .map(|(_, p)| p.to_point())
            });
            if let Some(lp) = leader_pos {
                if let Some(intent) = pathfind_toward(entity, pos, lp, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "retreat_to_fallback" => {
            // Pathfind toward the squad's fallback point.
            use crate::game::squad::{SquadBlackboard, SquadId, SquadLeader};
            let fallback = world.get::<SquadId>(entity).copied().and_then(|sid| {
                let mut q = world.query_filtered::<(&SquadId, &SquadBlackboard), With<SquadLeader>>();
                q.iter(world)
                    .find(|(lsid, _)| **lsid == sid)
                    .and_then(|(_, bb)| bb.fallback_point)
            });
            if let Some(fb) = fallback {
                if let Some(intent) = pathfind_toward(entity, pos, fb, world) {
                    world.write_message(intent);
                    return;
                }
            }
            // No fallback point — just flee from player instead.
            let mut player_query = world.query_filtered::<&Position, With<Player>>();
            if let Some(pp) = player_query.iter(world).next().map(|p| p.to_point()) {
                if let Some(intent) = try_flee_movement(entity, pos, pp, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "reposition_behind_ally" => {
            // Move to a tile where an ally is between us and the player.
            let mut player_query = world.query_filtered::<&Position, With<Player>>();
            let player_pos = player_query.iter(world).next().map(|p| p.to_point());
            if let Some(pp) = player_pos {
                // Flee from player — this naturally puts us behind allies.
                if let Some(intent) = try_flee_movement(entity, pos, pp, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "ranged_attack" => {
            // Fire at the player via RangedAttackIntent.
            use crate::game::actions::RangedAttackIntent;
            let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
            let target = player_query.iter(world).next()
                .map(|(e, _)| e);
            if let Some(target) = target {
                world.write_message(RangedAttackIntent { attacker: entity, target });
            } else {
                world.write_message(WaitIntent { entity });
            }
        }

        "cast_spell" => {
            // Delegate to the monster ability system.
            use crate::game::ai::try_use_ability_world;
            if !try_use_ability_world(entity, world) {
                world.write_message(WaitIntent { entity });
            }
        }

        "order_retreat" => {
            // Commander orders retreat — set retreat_ordered on the blackboard.
            use crate::game::squad::{SquadBlackboard, SquadId, SquadLeader};
            let sid = world.get::<SquadId>(entity).copied();
            let spawn_pos = world.get::<crate::game::MonsterAI>(entity)
                .and_then(|ai| ai.spawn_position);
            if let Some(sid) = sid {
                let mut q = world.query_filtered::<(&SquadId, &mut SquadBlackboard), With<SquadLeader>>();
                for (lsid, mut bb) in q.iter_mut(world) {
                    if *lsid == sid {
                        bb.retreat_ordered = true;
                        if bb.fallback_point.is_none() {
                            bb.fallback_point = spawn_pos;
                        }
                        break;
                    }
                }
            }
            world.write_message(WaitIntent { entity });
        }

        _ => {
            // "roam" — pathfind to a random reachable tile. Pick a new target
            // when we don't have one or we've reached the current one.
            if ai.roam_target.is_none() || ai.roam_target == Some(pos) {
                let old = ai.roam_target;
                ai.roam_target = pick_random_walkable_tile_near(pos, world);
                let name = world.get::<crate::components::Name>(entity)
                    .map(|n| n.0.clone()).unwrap_or_default();
                bevy::log::info!(
                    "ROAM {} {entity:?}: pos=({},{}) old_target={:?} new_target={:?}",
                    name, pos.x, pos.y, old, ai.roam_target
                );
            }

            if let Some(target) = ai.roam_target {
                if let Some(intent) = pathfind_toward(entity, pos, target, world) {
                    world.write_message(intent);
                    return;
                }
                // Pathfinding failed — pick a new target next turn.
                ai.roam_target = None;
            }
            world.write_message(WaitIntent { entity });
        }
    }
}

/// Pick a random walkable tile near a position for roaming.
/// Searches within ROAM_RADIUS tiles to keep monsters in their local area.
const ROAM_RADIUS: i32 = 12;

fn pick_random_walkable_tile_near(pos: Point, world: &mut World) -> Option<Point> {
    use crate::map::tile::is_walkable;
    let map = world.resource::<Map>();
    let w = map.width();
    let h = map.height();
    let tiles = &map.tiles;

    // Collect walkable positions within ROAM_RADIUS of the current position.
    let walkable: Vec<Point> = ((pos.x - ROAM_RADIUS).max(0)..=(pos.x + ROAM_RADIUS).min(w - 1))
        .flat_map(|x| ((pos.y - ROAM_RADIUS).max(0)..=(pos.y + ROAM_RADIUS).min(h - 1)).map(move |y| (x, y)))
        .filter(|&(x, y)| is_walkable(tiles[(y * w + x) as usize]))
        .map(|(x, y)| Point::new(x, y))
        .collect();

    if walkable.is_empty() {
        return None;
    }
    let mut rng = rand::rng();
    use rand::Rng;
    let idx = rng.random_range(0..walkable.len());
    Some(walkable[idx])
}

/// Find the nearest visible floor item or chest, excluding items at the hoard position.
pub(super) fn find_nearest_loot(
    _entity: Entity,
    pos: Point,
    hoard_position: Option<Point>,
    viewshed: Option<&Viewshed>,
    world: &mut World,
) -> Option<Point> {
    let vt = viewshed.map(|v| &v.visible_tiles)?;
    let mut best: Option<(Point, f32)> = None;

    let mut item_query = world.query_filtered::<&Position, (With<Item>, Without<InInventory>)>();
    for item_pos in item_query.iter(world) {
        let ipt = item_pos.to_point();
        if hoard_position.is_some_and(|hp| ipt == hp) { continue; }
        if vt.contains(&ipt) {
            let dist = DistanceAlg::Pythagoras.distance2d(pos, ipt);
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((ipt, dist));
            }
        }
    }

    let mut chest_query = world.query_filtered::<&Position, With<Chest>>();
    for chest_pos in chest_query.iter(world) {
        let cpt = chest_pos.to_point();
        if vt.contains(&cpt) {
            let dist = DistanceAlg::Pythagoras.distance2d(pos, cpt);
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((cpt, dist));
            }
        }
    }

    best.map(|(pt, _)| pt)
}

/// System that handles `DropAtHoardMessage`: drops all inventory items at the entity's position.
pub fn handle_drop_at_hoard(
    mut commands: Commands,
    mut messages: MessageReader<DropAtHoardMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut inv_query: Query<(&Position, &mut Inventory)>,
) {
    for msg in messages.read() {
        let Ok((pos, mut inv)) = inv_query.get_mut(msg.entity) else { continue; };
        let drop_pos = *pos;
        for item_entity in inv.items.drain(..) {
            commands.entity(item_entity)
                .remove::<InInventory>()
                .insert(Position { x: drop_pos.x, y: drop_pos.y })
                .insert(Visibility::Inherited)
                .insert(FloorEntityMarker);
        }
        crate::game::actions::finish_turn(&mut commands, &mut finish_writer, msg.entity, crate::constants::BASE_ACTION_COST, crate::game::actions::ActionKind::Movement);
    }
}

use super::DropAtHoardMessage;
