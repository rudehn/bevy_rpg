//! MachineBuilder — places interactive machine rooms during map generation.
//!
//! Unlike prefabs (tile grids), machines use feature-based blueprints:
//! a list of features with placement hints that are resolved procedurally
//! within a room.

use bevy::log::info;
use bracket_lib::prelude::{Point, Rect};
use bracket_lib::random::RandomNumberGenerator;

use crate::map::builders::{BuilderMap, MetaMapBuilder};
use crate::map::tile::{Decoration, TerrainType, is_walkable};

// =====================================================================
// Blueprint Data Model
// =====================================================================

/// Describes where to place a feature within a room.
#[derive(Debug, Clone)]
pub enum FeaturePlacement {
    /// Center of the room.
    Center,
    /// Within 2 tiles of center.
    NearCenter,
    /// Any walkable floor tile in the room.
    RandomFloor,
    /// Adjacent to a wall (for levers, etc.)
    AgainstWall,
}

/// What to place at the resolved position.
#[derive(Debug, Clone)]
pub enum FeatureKind {
    /// Place a prop + machine trigger/effect.
    MachineEntity {
        prop_name: String,
        trigger: crate::game::machines::MachineTrigger,
        effect: crate::game::machines::MachineEffect,
        /// If true, the machine entity is despawned after activation.
        consume_on_use: bool,
    },
    /// Place a regular chest prop (opens normally via chest logic).
    Chest,
    /// Place a candle (light source) at this position.
    Light,
    /// Spread a decoration in a radius around this position.
    Decorate { decoration: Decoration, radius: i32 },
}

/// A single feature within a machine blueprint.
#[derive(Debug, Clone)]
pub struct MachineFeature {
    pub placement: FeaturePlacement,
    pub kind: FeatureKind,
}

/// A machine blueprint — describes what to build, not where.
pub struct MachineBlueprint {
    pub name: &'static str,
    pub min_floor: i32,
    pub max_floor: i32,
    pub min_room_width: i32,
    pub min_room_height: i32,
    pub frequency: u32,
    pub features: Vec<MachineFeature>,
}

/// Data recorded by the builder for later entity spawning.
#[derive(Debug, Clone)]
pub struct MachineSpawn {
    pub pos: Point,
    pub prop_name: String,
    pub trigger: crate::game::machines::MachineTrigger,
    pub effect: crate::game::machines::MachineEffect,
    pub consume_on_use: bool,
}

// =====================================================================
// Blueprint Catalog
// =====================================================================

use crate::game::machines::{MachineEffect, MachineTrigger};

fn shrine_blueprint() -> MachineBlueprint {
    MachineBlueprint {
        name: "Shrine",
        min_floor: 1,
        max_floor: 20,
        min_room_width: 5,
        min_room_height: 5,
        frequency: 3,
        features: vec![
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::MachineEntity {
                    prop_name: "altar".to_string(),
                    trigger: MachineTrigger::BumpActivate,
                    effect: MachineEffect::Multi(vec![
                        MachineEffect::HealFull,
                        MachineEffect::SpawnItem {
                            item_name: "Scroll of Enchanting".to_string(),
                        },
                    ]),
                    consume_on_use: false,
                },
            },
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::Decorate {
                    decoration: Decoration::Moss,
                    radius: 2,
                },
            },
        ],
    }
}

fn trapped_vault_blueprint() -> MachineBlueprint {
    MachineBlueprint {
        name: "Trapped Vault",
        min_floor: 3,
        max_floor: 20,
        min_room_width: 5,
        min_room_height: 5,
        frequency: 2,
        features: vec![
            // Normal chest — opens via regular chest logic, spawns loot
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::Chest,
            },
            // Hidden step-trigger at same position — when player walks onto the
            // tile (after chest despawns and they pick up loot), monsters ambush
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::MachineEntity {
                    prop_name: "".to_string(), // invisible trigger, no prop
                    trigger: MachineTrigger::StepActivate,
                    effect: MachineEffect::SpawnMonsters {
                        monster_name: String::new(), // empty = pick level-appropriate monsters
                        count: 2,
                    },
                    consume_on_use: true,
                },
            },
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::Decorate {
                    decoration: Decoration::Cobweb,
                    radius: 2,
                },
            },
        ],
    }
}

fn all_blueprints() -> Vec<MachineBlueprint> {
    vec![shrine_blueprint(), trapped_vault_blueprint()]
}

// =====================================================================
// Machine Budget
// =====================================================================

/// Determine the machine budget (min, max) for the given floor depth.
fn machine_budget(depth: i32, rng: &mut RandomNumberGenerator) -> usize {
    let (min, max) = match depth {
        1..=3 => (3, 5),
        4..=6 => (4, 6),
        _ => (5, 8),
    };
    rng.range(min, max + 1) as usize
}

// =====================================================================
// Machine Builder (MetaMapBuilder)
// =====================================================================

pub struct MachineBuilder;

impl MachineBuilder {
    pub fn new() -> Box<Self> {
        Box::new(Self)
    }
}

impl MetaMapBuilder for MachineBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let Some(rooms) = build_data.rooms.as_ref() else {
            return;
        };
        let depth = build_data.map.depth;
        let mut rng = RandomNumberGenerator::new();

        let max_machines = machine_budget(depth, &mut rng);

        // Clone rooms so we can borrow build_data mutably later
        let rooms_snapshot: Vec<Rect> = rooms.clone();

        // Collect available rooms (not used by prefabs, skip room 0)
        let available_rooms: Vec<Rect> = rooms_snapshot
            .iter()
            .skip(1) // Room 0 is reserved for the player start
            .filter(|room| {
                let rw = room.x2 - room.x1;
                let rh = room.y2 - room.y1;
                if rw < 4 || rh < 4 {
                    return false;
                }
                // Skip rooms that overlap with any exclusion zone
                !build_data
                    .decoration_exclusion_zones
                    .iter()
                    .any(|ez| {
                        room.x1 < ez.x2
                            && room.x2 > ez.x1
                            && room.y1 < ez.y2
                            && room.y2 > ez.y1
                    })
            })
            .copied()
            .collect();

        info!(
            "MachineBuilder: {} eligible rooms out of {} total, budget {}",
            available_rooms.len(),
            rooms_snapshot.len(),
            max_machines
        );

        if available_rooms.is_empty() {
            return;
        }

        let blueprints = all_blueprints();
        let eligible: Vec<&MachineBlueprint> = blueprints
            .iter()
            .filter(|bp| depth >= bp.min_floor && depth <= bp.max_floor)
            .collect();

        if eligible.is_empty() {
            info!(
                "MachineBuilder: no eligible blueprints for depth {}",
                depth
            );
            return;
        }

        let mut placed = 0usize;
        let mut claimed_rooms: Vec<bool> = vec![false; available_rooms.len()];

        for (room_idx, room) in available_rooms.iter().enumerate() {
            if placed >= max_machines {
                break;
            }
            if claimed_rooms[room_idx] {
                continue;
            }

            // 20% chance to skip placing a machine in any given room
            if rng.range(0, 100) >= 80 {
                continue;
            }

            let rw = room.x2 - room.x1;
            let rh = room.y2 - room.y1;

            // Find blueprints that fit this room
            let fitting: Vec<&MachineBlueprint> = eligible
                .iter()
                .filter(|bp| rw >= bp.min_room_width && rh >= bp.min_room_height)
                .copied()
                .collect();

            if fitting.is_empty() {
                continue;
            }

            // Weighted random selection
            let total_freq: u32 = fitting.iter().map(|bp| bp.frequency).sum();
            let mut roll = rng.range(0, total_freq as i32) as u32;
            let mut chosen = &fitting[0];
            for bp in &fitting {
                if roll < bp.frequency {
                    chosen = bp;
                    break;
                }
                roll -= bp.frequency;
            }

            // Place the machine features
            let cx = (room.x1 + room.x2) / 2;
            let cy = (room.y1 + room.y2) / 2;

            for feature in &chosen.features {
                let pos =
                    resolve_placement(&feature.placement, *room, cx, cy, &build_data.map, &mut rng);
                let Some(pos) = pos else { continue };

                match &feature.kind {
                    FeatureKind::MachineEntity {
                        prop_name,
                        trigger,
                        effect,
                        consume_on_use,
                    } => {
                        if prop_name.is_empty() {
                            // Invisible trigger — no prop, just machine components
                            // Still add to machine_spawn_list; materialization handles empty prop_name
                        }
                        build_data.machine_spawn_list.push(MachineSpawn {
                            pos,
                            prop_name: prop_name.clone(),
                            trigger: trigger.clone(),
                            effect: effect.clone(),
                            consume_on_use: *consume_on_use,
                        });
                    }
                    FeatureKind::Chest => {
                        build_data
                            .prop_spawn_list
                            .push((pos, "chest".to_string()));
                    }
                    FeatureKind::Light => {
                        build_data
                            .prop_spawn_list
                            .push((pos, "candle".to_string()));
                    }
                    FeatureKind::Decorate { decoration, radius } => {
                        // Spread decoration around the position
                        for dy in -radius..=*radius {
                            for dx in -radius..=*radius {
                                let nx = pos.x + dx;
                                let ny = pos.y + dy;
                                let idx = build_data.map.xy_idx(nx, ny);
                                if idx < build_data.map.tiles.len()
                                    && is_walkable(build_data.map.tiles[idx])
                                    && build_data.map.tiles[idx].decoration == Decoration::None
                                {
                                    build_data.map.tiles[idx].decoration = *decoration;
                                }
                            }
                        }
                    }
                }
            }

            // Mark room as claimed (add exclusion zone)
            claimed_rooms[room_idx] = true;
            build_data.decoration_exclusion_zones.push(*room);
            placed += 1;

            let cx = (room.x1 + room.x2) / 2;
            let cy = (room.y1 + room.y2) / 2;
            info!(
                "Placed machine '{}' in room at ({}, {})",
                chosen.name, cx, cy
            );
        }
    }
}

// =====================================================================
// Placement Resolution
// =====================================================================

/// Resolve a FeaturePlacement to concrete coordinates within a room.
fn resolve_placement(
    placement: &FeaturePlacement,
    room: Rect,
    cx: i32,
    cy: i32,
    map: &crate::map::map::Map,
    rng: &mut RandomNumberGenerator,
) -> Option<Point> {
    match placement {
        FeaturePlacement::Center => {
            // Try center first; if not walkable, search nearby
            let idx = map.xy_idx(cx, cy);
            if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                return Some(Point::new(cx, cy));
            }
            // Spiral search from center
            for r in 1..=3 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        let x = cx + dx;
                        let y = cy + dy;
                        let idx = map.xy_idx(x, y);
                        if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                            return Some(Point::new(x, y));
                        }
                    }
                }
            }
            None
        }
        FeaturePlacement::NearCenter => {
            for _ in 0..10 {
                let x = cx + rng.range(-2, 3);
                let y = cy + rng.range(-2, 3);
                let idx = map.xy_idx(x, y);
                if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                    return Some(Point::new(x, y));
                }
            }
            Some(Point::new(cx, cy)) // Fallback to center
        }
        FeaturePlacement::RandomFloor => {
            for _ in 0..20 {
                let x = rng.range(room.x1 + 1, room.x2);
                let y = rng.range(room.y1 + 1, room.y2);
                let idx = map.xy_idx(x, y);
                if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                    return Some(Point::new(x, y));
                }
            }
            None
        }
        FeaturePlacement::AgainstWall => {
            // Find a floor tile adjacent to a wall
            for _ in 0..20 {
                let x = rng.range(room.x1 + 1, room.x2);
                let y = rng.range(room.y1 + 1, room.y2);
                let idx = map.xy_idx(x, y);
                if idx >= map.tiles.len() || !is_walkable(map.tiles[idx]) {
                    continue;
                }
                // Check if adjacent to a wall
                let dirs = [(0, -1), (0, 1), (-1, 0), (1, 0)];
                let near_wall = dirs.iter().any(|(dx, dy)| {
                    let ni = map.xy_idx(x + dx, y + dy);
                    ni < map.tiles.len() && map.tiles[ni].terrain == TerrainType::Wall
                });
                if near_wall {
                    return Some(Point::new(x, y));
                }
            }
            None
        }
    }
}
