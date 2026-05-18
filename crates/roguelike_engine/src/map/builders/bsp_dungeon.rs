//! BSP (Binary Space Partition) dungeon generator.

use bracket_lib::prelude::Rect;
use petgraph::Graph;
use std::cmp::{max, min, Ordering};

use super::{BuildContext, BuilderPhase, MapBuilder};

#[derive(Clone)]
pub struct BspConfig {
    pub subdivision_variance: f64,
    pub depth: i32,
    pub min_room_width: i32,
    pub max_room_width: i32,
    pub min_room_height: i32,
    pub max_room_height: i32,
    pub min_padding: i32,
    pub max_padding: i32,
}

impl BspConfig {
    pub fn dungeon() -> Self {
        Self {
            subdivision_variance: 0.2,
            depth: 6,
            min_room_width: 6,
            max_room_width: 20,
            min_room_height: 5,
            max_room_height: 12,
            max_padding: 9000,
            min_padding: 2,
        }
    }

    pub fn interior() -> Self {
        Self {
            subdivision_variance: 0.2,
            depth: 5,
            min_room_width: 6,
            max_room_width: 9000,
            min_room_height: 6,
            max_room_height: 9000,
            max_padding: 0,
            min_padding: 0,
        }
    }

    fn min_region_width(&self) -> i32 {
        self.min_room_width + self.min_padding * 2
    }
    fn min_region_height(&self) -> i32 {
        self.min_room_height + self.min_padding * 2
    }
    fn subdivision_min(&self) -> f64 {
        0.5 - self.subdivision_variance / 2.0
    }
    fn subdivision_max(&self) -> f64 {
        0.5 + self.subdivision_variance / 2.0
    }
}

#[derive(Clone)]
pub struct BspDungeonBuilder {
    config: BspConfig,
}

impl BspDungeonBuilder {
    pub fn new(config: BspConfig) -> Self {
        Self { config }
    }

    pub fn dungeon() -> Self {
        Self { config: BspConfig::dungeon() }
    }

    pub fn interior() -> Self {
        Self { config: BspConfig::interior() }
    }
}

impl<C: BuildContext> MapBuilder<C> for BspDungeonBuilder {
    fn name(&self) -> &'static str { "BspDungeon" }
    fn phase(&self) -> Option<BuilderPhase> { Some(BuilderPhase::Geometry) }
    fn build(&mut self, ctx: &mut C) {
        let map_w = ctx.width();
        let map_h = ctx.height();

        let mut graph = Graph::<Rect, ()>::new();
        let root = graph.add_node(Rect::with_size(0, 0, map_w - 1, map_h - 1));
        let mut leaves = vec![root];
        let mut rooms = Vec::new();

        for _depth in 1..self.config.depth + 1 {
            leaves = leaves
                .iter()
                .flat_map(|&leaf| {
                    let leaf_rect = *graph.node_weight(leaf).unwrap();
                    let mut a_rect = leaf_rect;
                    let mut b_rect = leaf_rect;
                    let position = ctx.rng().range(
                        self.config.subdivision_min(),
                        self.config.subdivision_max(),
                    );
                    if ctx.rng().roll_dice(1, 2) == 1 {
                        a_rect.x2 -= (a_rect.width() as f64 * position).round() as i32;
                        b_rect.x1 = a_rect.x2;
                    } else {
                        a_rect.y2 -= (a_rect.height() as f64 * position).round() as i32;
                        b_rect.y1 = a_rect.y2;
                    }

                    if a_rect.width() < self.config.min_region_width()
                        || b_rect.width() < self.config.min_region_width()
                        || a_rect.height() < self.config.min_region_height()
                        || b_rect.height() < self.config.min_region_height()
                    {
                        vec![leaf]
                    } else {
                        let a = graph.add_node(a_rect);
                        let b = graph.add_node(b_rect);
                        graph.add_edge(leaf, a, ());
                        graph.add_edge(leaf, b, ());
                        vec![a, b]
                    }
                })
                .collect();

            ctx.take_snapshot();
        }

        for leaf in leaves {
            let partition = graph.node_weight(leaf).unwrap();
            let min_width = max(
                self.config.min_room_width,
                partition.width() - self.config.max_padding * 2,
            );
            let max_width = min(
                partition.width() - self.config.min_padding * 2,
                self.config.max_room_width,
            );
            let width = match min_width.cmp(&max_width) {
                Ordering::Equal => min_width,
                Ordering::Less => ctx.rng().range(min_width, max_width),
                _ => unreachable!(),
            };

            let min_x1 = partition.x1 + self.config.min_padding;
            let max_x1 = partition.x2 - width - self.config.min_padding;
            let x1 = match min_x1.cmp(&max_x1) {
                Ordering::Equal => min_x1,
                Ordering::Less => ctx.rng().range(min_x1, max_x1),
                _ => unreachable!(),
            };

            let min_height = max(
                self.config.min_room_height,
                partition.height() - self.config.max_padding * 2,
            );
            let max_height = min(
                partition.height() - self.config.min_padding * 2,
                self.config.max_room_height,
            );
            let height = match min_height.cmp(&max_height) {
                Ordering::Equal => min_height,
                Ordering::Less => ctx.rng().range(min_height, max_height),
                _ => unreachable!(),
            };

            let min_y1 = partition.y1 + self.config.min_padding;
            let max_y1 = partition.y2 - height - self.config.min_padding;
            let y1 = match min_y1.cmp(&max_y1) {
                Ordering::Equal => min_y1,
                Ordering::Less => ctx.rng().range(min_y1, max_y1),
                _ => unreachable!(),
            };

            rooms.push(Rect::with_size(x1, y1, width, height));
        }
        ctx.take_snapshot();
        ctx.set_rooms(rooms);
    }
}
