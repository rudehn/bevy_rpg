use std::collections::VecDeque;

use bracket_lib::prelude::{Algorithm2D, Point, RandomNumberGenerator, Rect};

use crate::assets::DecorationRule;
use crate::map::tile::{Decoration, LiquidType, TerrainType};

use super::{BuilderMap, BuilderPhase, MetaMapBuilder};

pub struct DecorationPropagator {
    rules: Vec<DecorationRule>,
    depth: i32,
    density: f32,
}

impl MetaMapBuilder for DecorationPropagator {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.propagate(build_data);
    }

    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::Finalization) }
}

impl DecorationPropagator {
    pub fn new(rules: Vec<DecorationRule>, depth: i32, density: f32) -> Box<Self> {
        Box::new(Self { rules, depth, density })
    }
}

impl DecorationPropagator {
    /// Check if a point is inside any exclusion zone.
    fn in_exclusion_zone(pt: Point, zones: &[Rect]) -> bool {
        zones.iter().any(|r| {
            pt.x >= r.x1 && pt.x <= r.x2 && pt.y >= r.y1 && pt.y <= r.y2
        })
    }

    /// Check if a tile has a wall in any cardinal neighbor.
    fn has_adjacent_wall(map: &crate::map::map::Map, x: i32, y: i32) -> bool {
        [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
            .iter()
            .any(|&(nx, ny)| {
                let pt = Point::new(nx, ny);
                map.in_bounds(pt) && map.tiles[map.xy_idx(nx, ny)].terrain == TerrainType::Wall
            })
    }

    /// Check if a tile is in a corner (wall on two adjacent cardinal sides forming L).
    fn is_corner(map: &crate::map::map::Map, x: i32, y: i32) -> bool {
        let pairs = [
            ((x - 1, y), (x, y - 1)),
            ((x + 1, y), (x, y - 1)),
            ((x - 1, y), (x, y + 1)),
            ((x + 1, y), (x, y + 1)),
        ];
        pairs.iter().any(|&((ax, ay), (bx, by))| {
            let pa = Point::new(ax, ay);
            let pb = Point::new(bx, by);
            map.in_bounds(pa)
                && map.in_bounds(pb)
                && map.tiles[map.xy_idx(ax, ay)].terrain == TerrainType::Wall
                && map.tiles[map.xy_idx(bx, by)].terrain == TerrainType::Wall
        })
    }

    /// Check if any tile within radius has a non-None liquid.
    fn has_nearby_liquid(map: &crate::map::map::Map, x: i32, y: i32, radius: i32) -> bool {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let pt = Point::new(x + dx, y + dy);
                if map.in_bounds(pt)
                    && map.tiles[map.xy_idx(x + dx, y + dy)].liquid != LiquidType::None
                {
                    return true;
                }
            }
        }
        false
    }
}

impl DecorationPropagator {
    fn propagate(&self, build_data: &mut BuilderMap) {
        let mut rng = RandomNumberGenerator::new();
        let width = build_data.width;
        let height = build_data.height;
        let exclusion_zones = build_data.exclusion_zones().to_vec();

        for rule in &self.rules {
            if self.depth < rule.min_floor || self.depth > rule.max_floor {
                continue;
            }

            let raw_seeds = rng.range(rule.min_seeds, rule.max_seeds + 1);
            let seed_count = ((raw_seeds as f32) * self.density).ceil() as i32;

            for _ in 0..seed_count {
                // Try to find a valid seed position (max 50 attempts)
                let mut seed_pos = None;
                for _ in 0..50 {
                    let x = rng.range(1, width - 1);
                    let y = rng.range(1, height - 1);
                    let idx = build_data.map.xy_idx(x, y);
                    let tile = build_data.map.tiles[idx];

                    if !rule.requires_terrain.contains(&tile.terrain) {
                        continue;
                    }
                    if tile.decoration != Decoration::None {
                        continue;
                    }
                    if Self::in_exclusion_zone(
                        Point::new(x, y),
                        &exclusion_zones,
                    ) {
                        continue;
                    }
                    if rule.wall_adjacent_only
                        && !Self::has_adjacent_wall(&build_data.map, x, y)
                    {
                        continue;
                    }
                    if rule.corner_only && !Self::is_corner(&build_data.map, x, y) {
                        continue;
                    }
                    if rule.requires_nearby_liquid
                        && !Self::has_nearby_liquid(&build_data.map, x, y, 3)
                    {
                        continue;
                    }

                    seed_pos = Some((x, y));
                    break;
                }

                let Some((sx, sy)) = seed_pos else { continue };

                // Place seed
                let idx = build_data.map.xy_idx(sx, sy);
                build_data.map.tiles[idx].decoration = rule.decoration;

                // BFS propagation
                let mut queue: VecDeque<(i32, i32, f32, i32)> = VecDeque::new();
                queue.push_back((sx, sy, rule.propagation_chance, 0));

                while let Some((cx, cy, chance, depth)) = queue.pop_front() {
                    if depth >= rule.max_propagation_depth {
                        continue;
                    }

                    for &(dx, dy) in &[(0, 1), (0, -1), (1, 0), (-1, 0)] {
                        let nx = cx + dx;
                        let ny = cy + dy;
                        let npt = Point::new(nx, ny);

                        if !build_data.map.in_bounds(npt) {
                            continue;
                        }

                        let nidx = build_data.map.xy_idx(nx, ny);
                        let ntile = build_data.map.tiles[nidx];

                        if !rule.requires_terrain.contains(&ntile.terrain) {
                            continue;
                        }
                        if ntile.decoration != Decoration::None {
                            continue;
                        }
                        if Self::in_exclusion_zone(npt, &build_data.decoration_exclusion_zones) {
                            continue;
                        }

                        if rng.range(0, 100) >= (chance * 100.0) as i32 {
                            continue;
                        }

                        let dec = if let Some(ref chain) = rule.chain {
                            if rng.range(0, 100) < (chain.chance * 100.0) as i32 {
                                chain.decoration
                            } else {
                                rule.decoration
                            }
                        } else {
                            rule.decoration
                        };

                        build_data.map.tiles[nidx].decoration = dec;
                        queue.push_back((
                            nx,
                            ny,
                            chance * rule.propagation_decay,
                            depth + 1,
                        ));
                    }
                }
            }
        }
    }
}
