use crate::map::builders::{BuilderMap, InitialMapBuilder};
use bracket_lib::geometry::Rect;
use bracket_lib::random::RandomNumberGenerator;
use petgraph::Graph;
use std::cmp::{Ordering, max, min};

#[derive(Clone)]
pub struct BspConfig {
    /// 0..=1; higher values lead to more varied room aspect ratios
    pub subdivision_variance: f64,
    pub depth: i32,

    pub min_room_width: i32,
    pub max_room_width: i32,

    pub min_room_height: i32,
    pub max_room_height: i32,

    /// Minimum number of wall tiles between the edge of the region and the start of the room.
    /// Value is *per side*.
    pub min_padding: i32,
    /// Maximum number of wall tiles between the edge of the region and the start of the room.
    /// Value is *per side*.
    pub max_padding: i32,
}

impl BspConfig {
    pub fn dungeon() -> Self {
        BspConfig {
            subdivision_variance: 0.2,
            depth: 6,
            min_room_width: 6,
            max_room_width: 20,
            min_room_height: 5,
            max_room_height: 12,
            max_padding: 9000, // Arbitrarily large number. Not maxint because that leads to overflow.
            min_padding: 2,
        }
    }

    pub fn interior() -> Self {
        BspConfig {
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

impl InitialMapBuilder for BspDungeonBuilder {
    #[allow(dead_code)]
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        self.build(build_data);
    }
}

impl BspDungeonBuilder {
    #[allow(dead_code)]
    pub fn new(config: BspConfig) -> Box<BspDungeonBuilder> {
        Box::new(BspDungeonBuilder { config })
    }

    pub fn dungeon() -> Box<BspDungeonBuilder> {
        Box::new(BspDungeonBuilder {
            config: BspConfig::dungeon(),
        })
    }

    pub fn interior() -> Box<BspDungeonBuilder> {
        Box::new(BspDungeonBuilder {
            config: BspConfig::interior(),
        })
    }

    fn build(&mut self, build_data: &mut BuilderMap) {
        let mut graph = Graph::<Rect, ()>::new();
        let root = graph.add_node(Rect::with_size(
            0,
            0,
            build_data.map.width() - 1,
            build_data.map.height() - 1,
        ));
        let mut leaves = vec![root];
        let mut rooms = Vec::new();

        // Generate space partition
        let mut rng = RandomNumberGenerator::new();
        let mut max_depth = 0;
        for depth in 1..self.config.depth + 1 {
            leaves = leaves
                .iter()
                .flat_map(|&leaf| {
                    let leaf_rect = graph.node_weight(leaf).unwrap();
                    let mut a_rect;
                    let mut b_rect;

                    a_rect = *leaf_rect;
                    b_rect = *leaf_rect;
                    let position =
                        rng.range(self.config.subdivision_min(), self.config.subdivision_max());
                    if rng.roll_dice(1, 2) == 1 {
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
                        max_depth = max(max_depth, depth);
                        vec![a, b]
                    }
                })
                .collect();

            build_data.take_snapshot();
        }

        // Create room in each partition
        for leaf in leaves {
            let partition = graph.node_weight(leaf).unwrap();

            // Generate random room width based on config
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
                Ordering::Less => rng.range(min_width, max_width),
                _ => unreachable!(),
            };

            // Generate random room left-edge, based on config and width
            let min_x1 = partition.x1 + self.config.min_padding;
            let max_x1 = partition.x2 - width - self.config.min_padding;
            let x1 = match min_x1.cmp(&max_x1) {
                Ordering::Equal => min_x1,
                Ordering::Less => rng.range(min_x1, max_x1),
                _ => unreachable!(),
            };

            // Generate random room height based on config
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
                Ordering::Less => rng.range(min_height, max_height),
                _ => unreachable!(),
            };

            // Generate random room top-edge, based on config and height
            let min_y1 = partition.y1 + self.config.min_padding;
            let may_y1 = partition.y2 - height - self.config.min_padding;
            let y1 = match min_y1.cmp(&may_y1) {
                Ordering::Equal => min_y1,
                Ordering::Less => rng.range(min_y1, may_y1),
                _ => unreachable!(),
            };

            let room = Rect::with_size(x1, y1, width, height);
            rooms.push(room);
        }
        build_data.take_snapshot();

        build_data.rooms = Some(rooms);
    }
}
