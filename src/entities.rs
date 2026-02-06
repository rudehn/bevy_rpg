use serde::Deserialize;
use shipyard::{
    AddComponent, Component, Delete, EntityId, Get, IntoIter, UniqueView, UniqueViewMut, View,
    ViewMut, World,
};

use crate::components::{Faction, GodMode, Name, OtherLevelPosition, Player, Position};
use crate::events::{Event, EventType, SingleTarget, add_event};
use crate::map::{DISCOVERED, Map, T_LAVA_INSTA_DEATH, VISIBLE};
use crate::scenes::RunState;
use bracket_lib::prelude::{DistanceAlg, Point, field_of_view};

pub fn get_entity_name(entity_id: EntityId, ecs: &World) -> String {
    let names = ecs.borrow::<View<Name>>().unwrap();
    let default_name = Name {
        name: "Nameless Entity (Bug)".to_string(),
    };
    let name = names.get(entity_id).unwrap_or(&default_name);
    name.name.clone()
}

#[derive(Component)]
pub struct Traits {
    pub traits: Vec<EntityTrait>, // Store these on the entity for easy rendering in the UI
}

// These are traits that monsters can spawn with
#[derive(Deserialize, Debug, Clone)]
pub enum EntityTrait {
    // TODO - put resistances in here??
    #[serde(rename = "sticky")]
    StickyBody,
    #[serde(rename = "poisonous")]
    Poisonous,
    #[serde(rename = "splits")]
    Splits,
    #[serde(rename = "reanimate_skeleton")]
    ReanimateSkeleton,
    #[serde(rename = "explodes")]
    Explodes,
    // ThickHide, // flat damage reduction
    // IronWill, //Immunity or high resistance to status effects (e.g., 50% chance to resist Stun).
    // Regeneration,
    // Nimble, // +chance to evade
    // Retaliation, // when hit, deal % damage back to attacker
    // ViciousBlow, // increased crit chance / damage
    // BloodRage, // Gain increased damage for every point of missing health.
    // Vampirism, // heal a % of damage dealt
    // Poisonous, // attacks inflict poison DoT
    // PenetratingStrike, // attacks ignore a % of armor
    // Swift, // increased movement speed / turn
    // Opportunist, // Deals bonus damage to targets affected by a status effect
    // Overload, // Spells/Skills cost more but are significantly more powerful.
    // ArcaneFocus, // mana regen bonus
    // Resourceful, // reduced cooldown
    // Defiance, // Gain a massive defensive boost (e.g., +50% armor) when surrounded by 3 or more enemies.
    // ChillingPresence, // All attacks apply a minor Slow effect.
    // ShockingGrasp, // When damaged by a melee attack, the attacker takes minor electrical damage and has a chance to be Stunned.
    // ArcaneResonance, // Arcane/Magic damage automatically transfers a portion of the damage dealt to the target's Mana/Resource pool.
    // SharedPain, // When the unit is damaged, the nearest ally takes a small fraction of the damage.
    // Ritualist,  // Skills/Spells cast in the last 3 turns have their Mana/Resource cost permanently reduced by 1 (down to a minimum).
    // Enduring, // Non-damaging debuffs applied to the unit expire 1 turn earlier.
    // Martyrdom, // If the unit dies while adjacent to a friendly unit, the friendly unit gains a powerful temporary buff.
    // PainRefusal, // When health is low (below 30%), incoming healing is halved, but the unit gains +3 movement speed.
    // Overwhelm, // Attacks apply an extra Stack of a temporary debuff (e.g., Fatigue). Upon 3 stacks, the target is Stunned.
    // ResourceSiphon, // Skills/Spells have a chance to steal a small amount of the target's Mana or Stamina.
    // Recharge, // If the unit does not use a Skill or Spell on its turn, all ability cooldowns are reduced by an extra turn.
    // ManaShield, // A percentage of incoming physical damage is first deducted from the unit's Mana pool instead of Health.
    // GrimHarvest, // Killing an enemy permanently gives a chance to increases the unit's maximum Mana by 1 (max +20).
    // Incorporeal, // Cannot be hit by physical attacks, but takes double damage from magical/elemental attacks.
    // Overcharge, // Any healing applied to this unit that exceeds its max health is converted into a temporary damage buff.
    // Toughness, // Permanently increases Maximum Health by a fixed percentage (e.g., +15% Max HP).
    // Brawler, // Permanently increases all base Physical Damage dealt by a fixed amount.
    // FocusedStrike, // Grants bonus damage if the unit has attacked the same target on the previous turn.
    // Electrifying, // Attacks inflict electric damage
    // Electric attacks jump to +1 target
    // poison +potency
    // poison +duration
    // Crits cause bleed
    // 19 roll is a crit
    // super crit - 18 roll is a crit
    // TrueStrike, // Ignores the target's Evasion or Dodge chance.
    // + 1 trait selection
}

impl EntityTrait {
    pub fn get_description(&self) -> String {
        match self {
            EntityTrait::StickyBody => "Contact will cause slow",
            EntityTrait::Poisonous => "Attacks inflict poison",
            EntityTrait::Splits => "Splits a copy with half HP when hit",
            EntityTrait::ReanimateSkeleton => "Reanimates as a skeleton when killed",
            EntityTrait::Explodes => "Explodes on death",
        }
        .to_string()
    }
}

#[derive(Component, Debug, Clone)]
pub struct BlocksVisibility {}

#[derive(Component, Debug, Clone)]
pub struct Hidden {}

#[derive(Component)]
pub struct Viewshed {
    pub visible_tiles: Vec<Point>,
    pub range: i32,
}

pub fn recalculate_visibility_event_handler(event: &mut Event, ecs: &mut World) {
    if let EventType::RecalculateVisibility { entity } = event.event_type {
        ecs.run(
            |mut viewsheds: ViewMut<Viewshed>,
             positions: View<Position>,
             mut map: UniqueViewMut<Map>,
             player: UniqueView<Player>,
             mut hidden: ViewMut<Hidden>,
             names: View<Name>| {
                let mut viewshed = (&mut viewsheds).get(entity).unwrap();
                let position = positions.get(entity).unwrap();
                update_entity_visibility(
                    entity,
                    &mut viewshed,
                    position,
                    &mut map,
                    &player,
                    &mut hidden,
                    &names,
                );
            },
        );
    }
}

pub fn update_entity_visibility(
    entity_id: EntityId,
    viewshed: &mut Viewshed,
    position: &Position,
    map: &mut Map,
    player: &Player,
    hidden: &mut ViewMut<Hidden>,
    names: &View<Name>,
) {
    viewshed.visible_tiles.clear();
    viewshed.visible_tiles =
        field_of_view(Point::new(position.x, position.y), viewshed.range, &*map);
    viewshed
        .visible_tiles
        .retain(|p| p.x >= 0 && p.x < map.width() && p.y >= 0 && p.y < map.height());

    if entity_id == player.id {
        for t in map.tiles.iter_mut() {
            t.clear_flags(VISIBLE);
        }
        for vis in viewshed.visible_tiles.iter() {
            let idx = map.xy_idx(vis.x, vis.y);
            map.tiles[idx].set_flags(VISIBLE | DISCOVERED);

            // Chance to reveal hidden things
            crate::spatial::for_each_tile_content(idx, |e| {
                if let Ok(_) = hidden.get(e) {
                    if crate::rng::roll_dice(1, 50) == 1 {
                        if let Ok(name) = names.get(e) {
                            crate::gamelog::Logger::new()
                                .append("You spotted:")
                                .npc_name(&name.name)
                                .log();
                        }
                        hidden.delete(e);
                    }
                }
            });
        }
    }
}

pub fn visibility_system(
    mut viewsheds: ViewMut<Viewshed>,
    positions: View<Position>,
    mut map: UniqueViewMut<Map>,
    player: UniqueView<Player>,
    blocks_visibility: View<BlocksVisibility>,
    mut hidden: ViewMut<Hidden>,
    names: View<Name>,
) {
    map.view_blocked.clear();
    // Update all locations on the map that block visiblity
    for (block_pos, _block) in (&positions, &blocks_visibility).iter() {
        let idx = map.xy_idx(block_pos.x, block_pos.y);
        map.view_blocked.insert(idx);
    }

    for (id, (mut viewshed, position)) in (&mut viewsheds, &positions).iter().with_id() {
        update_entity_visibility(
            id,
            &mut viewshed,
            position,
            &mut map,
            &player,
            &mut hidden,
            &names,
        );
    }
}

pub fn get_entities_in_vision(
    viewsheds: &View<Viewshed>,
    factions: &View<Faction>,
    positions: &View<Position>,
    map: &UniqueView<Map>,
    entity: EntityId,
    include_self: bool,
) -> Vec<(EntityId, f32)> {
    let mut entities_in_view: Vec<(EntityId, f32)> = Vec::new();
    let my_pos = positions.get(entity).unwrap();

    // Check the entity has a vision
    if let Ok(vs) = viewsheds.get(entity) {
        for tile_point in vs.visible_tiles.iter() {
            let tile_idx = map.xy_idx(tile_point.x, tile_point.y);
            let distance_to_target =
                DistanceAlg::Pythagoras.distance2d(*tile_point, Point::new(my_pos.x, my_pos.y));
            crate::spatial::for_each_tile_content(tile_idx, |possible_target| {
                if !include_self && possible_target == entity {
                    return;
                }
                if factions.get(possible_target).is_ok() {
                    entities_in_view.push((possible_target, distance_to_target));
                }
            });
        }
    }
    entities_in_view.sort_by(|a, b| a.1.total_cmp(&b.1));
    entities_in_view
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Movement {
    Static,
    Random,
    RandomWaypoint {
        path: Option<Vec<usize>>,
    },
    Guard {
        map_cell: usize,
        path: Option<Vec<usize>>,
    },
}

fn apply_teleport(
    ecs: &mut World,
    x: i32,
    y: i32,
    depth: i32,
    player_only: bool,
    target_id: EntityId,
) {
    let player = ecs.borrow::<UniqueView<Player>>().unwrap();
    let map = ecs.borrow::<UniqueView<Map>>().unwrap();
    let mut runstate = ecs.borrow::<UniqueViewMut<RunState>>().unwrap();
    let mut positions = ecs.borrow::<ViewMut<Position>>().unwrap();
    let mut other_level = ecs.borrow::<ViewMut<OtherLevelPosition>>().unwrap();

    if !player_only || target_id == player.id {
        // Apply teleports
        // Moving across the current map; local teleport
        if depth == map.depth {
            add_event(EventType::MoveTo {
                tile_idx: map.xy_idx(x, y),
                entity_id: target_id,
            });
        } else if target_id == player.id {
            *runstate = RunState::TeleportingToOtherLevel { x, y, depth }
        } else if let Ok(pos) = positions.get(target_id) {
            let idx = map.xy_idx(pos.x, pos.y);
            crate::spatial::remove_entity(target_id, idx);
            other_level.add_component_unchecked(target_id, OtherLevelPosition { x, y, depth });
            positions.delete(target_id);
        }
    }
}

pub fn teleport_event_handler(event: &mut Event, ecs: &mut World) {
    if let EventType::TeleportTo {
        x,
        y,
        depth,
        player_only,
        target,
    } = event.event_type
    {
        match target {
            SingleTarget::Entity { target } => {
                apply_teleport(ecs, x, y, depth, player_only, target)
            }
            SingleTarget::Tile { tile_idx } => {
                let tile_entities = crate::spatial::get_tile_content_clone(tile_idx);
                for entity_id in tile_entities {
                    apply_teleport(ecs, x, y, depth, player_only, entity_id);
                }
            }
        }
    }
}

pub fn movement_event_handler(event: &mut Event, ecs: &mut World) {
    if let EventType::MoveTo {
        tile_idx,
        entity_id,
    } = event.event_type
    {
        let map = ecs.borrow::<UniqueView<Map>>().unwrap();
        let mut positions = ecs.borrow::<ViewMut<Position>>().unwrap();
        let mut player = ecs.borrow::<UniqueViewMut<Player>>().unwrap();
        if let Ok(mut pos) = (&mut positions).get(entity_id) {
            let start_idx = map.xy_idx(pos.x, pos.y);
            crate::spatial::move_entity(entity_id, start_idx, tile_idx);
            let (dest_x, dest_y) = map.idx_to_xy(tile_idx);
            pos.x = dest_x;
            pos.y = dest_y;
            if entity_id == player.id {
                player.pos.x = pos.x;
                player.pos.y = pos.y;
            }
            add_event(EventType::EntityMoved {
                entity_id: entity_id,
            });
        }
    }
}

pub fn entity_moved_event_handler(event: &mut Event, ecs: &mut World) {
    if let EventType::EntityMoved { entity_id } = event.event_type {
        add_event(EventType::RecalculateVisibility { entity: entity_id });
        add_event(EventType::RecalculateDynamicLighting);
        // let player = ecs.borrow::<UniqueView<Player>>().unwrap();
        // if entity_id == player.id {
        // add_event(EventType::RecalculateDynamicLighting);
        // }
    }
}

pub fn tile_hazard_event_handler(event: &mut Event, ecs: &mut World) {
    // We only care if an entity actually moved
    if let EventType::EntityMoved { entity_id } = event.event_type {
        let map = ecs.borrow::<UniqueView<Map>>().unwrap();
        let positions = ecs.borrow::<View<Position>>().unwrap();

        // Check the entity's current position
        if let Ok(pos) = positions.get(entity_id) {
            let idx = map.xy_idx(pos.x, pos.y);
            if map.tiles[idx].has_highest_priority_terrain_flags(T_LAVA_INSTA_DEATH) {
                let name = get_entity_name(entity_id, ecs);
                crate::gamelog::Logger::new()
                    .npc_name(name)
                    .append("died in lava.")
                    .log();

                if let Ok(godmode) = ecs.borrow::<View<GodMode>>().unwrap().get(entity_id) {
                    if godmode.god_mode {
                        return;
                    }
                }
                add_event(EventType::Death { entity_id });
            }
        }
    }
}
