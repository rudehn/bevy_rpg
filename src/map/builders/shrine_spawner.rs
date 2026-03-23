use bevy::prelude::*;
use bracket_lib::prelude::{Point, RandomNumberGenerator, Rect};

use crate::{
    assets::{ShrineCategoryDef, ShrineEffectDef},
    game::{
        items::Rarity,
        shrines::{ShrineData, ShrineEffectInstance, ShrinesPurchased},
    },
    map::{
        builders::{BuilderMap, MetaMapBuilder, ShrineSpawnEntry},
        map::Map,
        tile::{is_walkable, LiquidType, TerrainType},
    },
};

pub struct ShrineSpawner {
    categories: Vec<ShrineCategoryDef>,
    purchased: Vec<String>,
}

impl MetaMapBuilder for ShrineSpawner {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.spawn_shrines(build_data);
    }
}

impl ShrineSpawner {
    pub fn new(categories: Vec<ShrineCategoryDef>, purchased: &ShrinesPurchased) -> Box<ShrineSpawner> {
        Box::new(ShrineSpawner {
            categories,
            purchased: purchased.0.clone(),
        })
    }

    fn spawn_shrines(&mut self, build_data: &mut BuilderMap) {
        if self.categories.is_empty() {
            return;
        }

        let mut rng = RandomNumberGenerator::new();
        let depth = build_data.map.depth;

        let Some(rooms) = build_data.rooms.clone() else {
            warn!("ShrineSpawner: rooms not set, skipping");
            return;
        };

        let mut pending: Vec<ShrineSpawnEntry> = Vec::new();
        let map = &build_data.map;

        for room in rooms.iter() {
            // ~33% chance per room
            if rng.range(0, 100) >= 33 {
                continue;
            }

            if let Some(pt) = walkable_room_point(room, map, &mut rng) {
                // Pick a random category
                let cat_idx = rng.range(0, self.categories.len());
                let category = &self.categories[cat_idx];

                // Roll 3 effects based on floor depth rarity tiers
                let rarity_slots = rarity_slots_for_depth(depth, &mut rng);
                let mut effects = Vec::new();

                for target_rarity in &rarity_slots {
                    if let Some(effect) = pick_effect(category, target_rarity, &self.purchased, &effects, &mut rng) {
                        effects.push(effect);
                    }
                }

                if effects.is_empty() {
                    continue;
                }

                let shrine_data = ShrineData {
                    category_id: category.id.clone(),
                    category_name: category.name.clone(),
                    effects,
                };

                pending.push(ShrineSpawnEntry {
                    pos: pt,
                    shrine_data,
                    category_id: category.id.clone(),
                });
            }
        }

        for entry in pending {
            build_data.shrine_spawn_list.push(entry);
        }
    }
}

/// Determine the three rarity slots based on floor depth.
fn rarity_slots_for_depth(depth: i32, rng: &mut RandomNumberGenerator) -> [Rarity; 3] {
    match depth {
        1..=5 => [Rarity::Common, Rarity::Common, Rarity::Uncommon],
        6..=10 => [Rarity::Common, Rarity::Uncommon, Rarity::Uncommon],
        11..=15 => [Rarity::Common, Rarity::Uncommon, Rarity::Rare],
        _ => {
            let third = if rng.range(0, 2) == 0 {
                Rarity::Rare
            } else {
                Rarity::Legendary
            };
            [Rarity::Uncommon, Rarity::Rare, third]
        }
    }
}

/// Pick a random effect of the target rarity from the category, avoiding
/// already-purchased unique effects and effects already selected for this shrine.
fn pick_effect(
    category: &ShrineCategoryDef,
    target_rarity: &Rarity,
    purchased: &[String],
    already_selected: &[ShrineEffectInstance],
    rng: &mut RandomNumberGenerator,
) -> Option<ShrineEffectInstance> {
    let candidates: Vec<&ShrineEffectDef> = category
        .effects
        .iter()
        .filter(|e| {
            e.rarity == *target_rarity
                && !(e.unique && purchased.contains(&e.id))
                && !already_selected.iter().any(|s| s.id == e.id)
        })
        .collect();

    if candidates.is_empty() {
        // Fallback: try any rarity from this category that isn't taken
        let fallback: Vec<&ShrineEffectDef> = category
            .effects
            .iter()
            .filter(|e| {
                !(e.unique && purchased.contains(&e.id))
                    && !already_selected.iter().any(|s| s.id == e.id)
            })
            .collect();
        if fallback.is_empty() {
            return None;
        }
        let idx = rng.range(0, fallback.len());
        let e = fallback[idx];
        return Some(to_instance(e));
    }

    let idx = rng.range(0, candidates.len());
    let e = candidates[idx];
    Some(to_instance(e))
}

fn to_instance(e: &ShrineEffectDef) -> ShrineEffectInstance {
    ShrineEffectInstance {
        id: e.id.clone(),
        name: e.name.clone(),
        description: e.description.clone(),
        rarity: e.rarity.clone(),
        cost: e.cost,
        kind: e.kind.clone(),
        unique: e.unique,
    }
}

fn walkable_room_point(room: &Rect, map: &Map, rng: &mut RandomNumberGenerator) -> Option<Point> {
    for _ in 0..20 {
        let x = if room.width() > 2 {
            rng.roll_dice(1, room.width() - 2) + room.x1 + 1
        } else {
            room.x1 + 1
        };
        let y = if room.height() > 2 {
            rng.roll_dice(1, room.height() - 2) + room.y1 + 1
        } else {
            room.y1 + 1
        };
        let idx = map.xy_idx(x, y);
        if is_walkable(map.tiles[idx])
            && map.tiles[idx].liquid == LiquidType::None
            && !matches!(map.tiles[idx].terrain, TerrainType::UpStairs | TerrainType::DownStairs)
        {
            return Some(Point::new(x, y));
        }
    }
    None
}
