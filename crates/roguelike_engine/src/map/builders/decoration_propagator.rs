//! Brogue-style decoration propagation.
//!
//! For each [`DecorationRule`] eligible at the current depth, places a
//! configurable number of seeds on tiles matching the rule's terrain
//! filter, then BFS-flood-fills neighbours with a per-step decaying
//! chance. Seeds avoid exclusion zones and may require wall-adjacent,
//! corner, or nearby-liquid placements.
//!
//! Runs as a [`MapBuilder`] in the [`BuilderPhase::Finalization`] phase so
//! it sees the final terrain layout (rooms, lakes, doors, prefabs).

use std::collections::VecDeque;

use bracket_lib::prelude::{Algorithm2D as _, Point};

use super::{BuildContext, BuilderPhase, MapBuilder};
use crate::map::decoration_rule::DecorationRule;
use crate::map::map::Map;
use crate::map::tile::{Decoration, LiquidType, TerrainType};

pub struct DecorationPropagator {
    rules: Vec<DecorationRule>,
    depth: i32,
    density: f32,
}

impl DecorationPropagator {
    /// `density` scales the per-rule seed count (0.0 = none, 1.0 = full).
    pub fn new(rules: Vec<DecorationRule>, depth: i32, density: f32) -> Self {
        Self { rules, depth, density }
    }
}

impl<C: BuildContext> MapBuilder<C> for DecorationPropagator {
    fn name(&self) -> &'static str { "DecorationPropagator" }
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::Finalization) }

    fn build(&mut self, ctx: &mut C) {
        let width = ctx.width();
        let height = ctx.height();
        let exclusion_zones: Vec<bracket_lib::prelude::Rect> =
            ctx.exclusion_zones().to_vec();

        for rule in &self.rules {
            if self.depth < rule.min_floor || self.depth > rule.max_floor {
                continue;
            }

            let raw_seeds = ctx.rng().range(rule.min_seeds, rule.max_seeds + 1);
            let seed_count = ((raw_seeds as f32) * self.density).ceil() as i32;

            for _ in 0..seed_count {
                // Try to find a valid seed position (max 50 attempts).
                let mut seed_pos: Option<(i32, i32)> = None;
                for _ in 0..50 {
                    let x = ctx.rng().range(1, width - 1);
                    let y = ctx.rng().range(1, height - 1);

                    let valid = {
                        let map = ctx.map();
                        let tile = map.tiles[map.xy_idx(x, y)];
                        rule.requires_terrain.contains(&tile.terrain)
                            && tile.decoration == Decoration::None
                            && !in_exclusion_zone(Point::new(x, y), &exclusion_zones)
                            && (!rule.wall_adjacent_only || has_adjacent_wall(map, x, y))
                            && (!rule.corner_only || is_corner(map, x, y))
                            && (!rule.requires_nearby_liquid
                                || has_nearby_liquid(map, x, y, 3))
                    };

                    if valid {
                        seed_pos = Some((x, y));
                        break;
                    }
                }

                let Some((sx, sy)) = seed_pos else { continue };

                // Place seed.
                let idx = ctx.map().xy_idx(sx, sy);
                ctx.map_mut().tiles[idx].decoration = rule.decoration;

                // BFS propagation.
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

                        let (nidx, ntile) = {
                            let map = ctx.map();
                            if !map.in_bounds(npt) {
                                continue;
                            }
                            let nidx = map.xy_idx(nx, ny);
                            (nidx, map.tiles[nidx])
                        };

                        if !rule.requires_terrain.contains(&ntile.terrain) {
                            continue;
                        }
                        if ntile.decoration != Decoration::None {
                            continue;
                        }
                        if in_exclusion_zone(npt, &exclusion_zones) {
                            continue;
                        }

                        if ctx.rng().range(0, 100) >= (chance * 100.0) as i32 {
                            continue;
                        }

                        let dec = if let Some(ref chain) = rule.chain {
                            if ctx.rng().range(0, 100) < (chain.chance * 100.0) as i32 {
                                chain.decoration
                            } else {
                                rule.decoration
                            }
                        } else {
                            rule.decoration
                        };

                        ctx.map_mut().tiles[nidx].decoration = dec;
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

// ─── Helpers ────────────────────────────────────────────────────────────

fn in_exclusion_zone(pt: Point, zones: &[bracket_lib::prelude::Rect]) -> bool {
    zones
        .iter()
        .any(|r| pt.x >= r.x1 && pt.x <= r.x2 && pt.y >= r.y1 && pt.y <= r.y2)
}

fn has_adjacent_wall(map: &Map, x: i32, y: i32) -> bool {
    [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
        .iter()
        .any(|&(nx, ny)| {
            let pt = Point::new(nx, ny);
            map.in_bounds(pt) && map.tiles[map.xy_idx(nx, ny)].terrain == TerrainType::Wall
        })
}

fn is_corner(map: &Map, x: i32, y: i32) -> bool {
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

fn has_nearby_liquid(map: &Map, x: i32, y: i32, radius: i32) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builders::EngineBuilderMap;
    use crate::map::tile::{Decoration, TerrainType};

    fn rule_for(name: &str, decoration: Decoration, seeds: i32) -> DecorationRule {
        DecorationRule {
            name: name.to_string(),
            min_floor: 1,
            max_floor: 99,
            min_seeds: seeds,
            max_seeds: seeds,
            decoration,
            requires_terrain: vec![TerrainType::Floor],
            propagation_chance: 0.0,
            propagation_decay: 0.0,
            max_propagation_depth: 0,
            wall_adjacent_only: false,
            corner_only: false,
            requires_nearby_liquid: false,
            chain: None,
        }
    }

    #[test]
    fn skips_rules_outside_depth_window() {
        let mut ctx = EngineBuilderMap::with_open_room(20, 20, 42);
        let mut rule = rule_for("grass", Decoration::Grass, 5);
        rule.min_floor = 5;
        rule.max_floor = 10;
        let mut prop = DecorationPropagator::new(vec![rule], /* depth = */ 1, 1.0);
        prop.build(&mut ctx);
        let painted = ctx
            .map
            .tiles
            .iter()
            .filter(|t| t.decoration == Decoration::Grass)
            .count();
        assert_eq!(painted, 0, "out-of-range depth should produce no decoration");
    }

    #[test]
    fn places_at_least_one_seed_when_eligible() {
        let mut ctx = EngineBuilderMap::with_open_room(20, 20, 42);
        let rule = rule_for("grass", Decoration::Grass, 5);
        let mut prop = DecorationPropagator::new(vec![rule], 1, 1.0);
        prop.build(&mut ctx);
        let painted = ctx
            .map
            .tiles
            .iter()
            .filter(|t| t.decoration == Decoration::Grass)
            .count();
        assert!(painted > 0, "expected at least one seed; got {painted}");
    }

    #[test]
    fn density_zero_paints_nothing() {
        let mut ctx = EngineBuilderMap::with_open_room(20, 20, 42);
        let rule = rule_for("grass", Decoration::Grass, 5);
        let mut prop = DecorationPropagator::new(vec![rule], 1, 0.0);
        prop.build(&mut ctx);
        let painted = ctx
            .map
            .tiles
            .iter()
            .filter(|t| t.decoration == Decoration::Grass)
            .count();
        assert_eq!(painted, 0, "density 0 should suppress all seeds");
    }

    #[test]
    fn respects_terrain_filter() {
        let mut ctx = EngineBuilderMap::with_open_room(20, 20, 42);
        // Wall tile at the border won't satisfy requires_terrain=Floor.
        let mut rule = rule_for("wallgrass", Decoration::Grass, 1);
        rule.requires_terrain = vec![TerrainType::Wall];
        let mut prop = DecorationPropagator::new(vec![rule], 1, 1.0);
        prop.build(&mut ctx);
        // Borders are walls but we limit seeds to 1..(w-1), 1..(h-1) — interior.
        // So no wall position is reachable as a seed → 0 painted.
        let painted = ctx
            .map
            .tiles
            .iter()
            .filter(|t| t.decoration == Decoration::Grass)
            .count();
        assert_eq!(painted, 0);
    }

    #[test]
    fn deterministic_under_fixed_seed() {
        fn run(seed: u64) -> usize {
            let mut ctx = EngineBuilderMap::with_open_room(30, 30, seed);
            let mut rule = rule_for("grass", Decoration::Grass, 4);
            rule.propagation_chance = 0.5;
            rule.propagation_decay = 0.7;
            rule.max_propagation_depth = 5;
            let mut prop = DecorationPropagator::new(vec![rule], 1, 1.0);
            prop.build(&mut ctx);
            ctx.map
                .tiles
                .iter()
                .filter(|t| t.decoration == Decoration::Grass)
                .count()
        }
        let a = run(123);
        let b = run(123);
        assert_eq!(a, b, "same seed should produce identical results");
    }
}
