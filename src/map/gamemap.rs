use bracket_lib::prelude::{Algorithm2D, BaseMap, Point};

use crate::map::basemap::Map;
use crate::map::tile::{is_walkable, TileType};

#[derive(Default, Clone)]
pub struct GameMap {
    pub name: String,
    pub tiles: Vec<TileType>,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub downstairs_position: Option<Point>,
}

impl GameMap {
    /// Creates a new map of the given size, with all tiles set to `Wall`.
    pub fn new<S: ToString>(depth: i32, width: i32, height: i32, name: S) -> Self {
        let map_tile_count = (width * height) as usize;
        Self {
            name: name.to_string(),
            tiles: vec![TileType::Wall; map_tile_count],
            width,
            height,
            depth,
            downstairs_position: None,
        }
    }

    pub fn xy_idx(&self, x: i32, y: i32) -> usize {
        (y as usize * self.width as usize) + x as usize
    }

    pub fn idx_xy(&self, idx: usize) -> (i32, i32) {
        (idx as i32 % self.width, idx as i32 / self.width)
    }
}

impl Map for GameMap {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn get_tile(&self, pt: Point) -> Option<TileType> {
        if self.in_bounds(pt) {
            let idx = self.xy_idx(pt.x, pt.y);
            Some(self.tiles[idx])
        } else {
            None
        }
    }

    fn set_tile(&mut self, pt: Point, tile: TileType) {
        if self.in_bounds(pt) {
            let idx = self.xy_idx(pt.x, pt.y);
            self.tiles[idx] = tile;
        }
    }
}

impl BaseMap for GameMap {
    fn is_opaque(&self, idx: usize) -> bool {
        self.tiles[idx] == TileType::Wall
    }

    fn get_available_exits(&self, idx: usize) -> bracket_lib::prelude::SmallVec<[(usize, f32); 10]> {
        let mut exits = bracket_lib::prelude::SmallVec::new();
        let (x, y) = self.idx_xy(idx);
        let w = self.width as usize;

        // Cardinal directions
        if self.in_bounds(Point::new(x - 1, y)) && is_walkable(self.tiles[idx - 1]) {
            exits.push((idx - 1, 1.0))
        };
        if self.in_bounds(Point::new(x + 1, y)) && is_walkable(self.tiles[idx + 1]) {
            exits.push((idx + 1, 1.0))
        };
        if self.in_bounds(Point::new(x, y - 1)) && is_walkable(self.tiles[idx - w]) {
            exits.push((idx - w, 1.0))
        };
        if self.in_bounds(Point::new(x, y + 1)) && is_walkable(self.tiles[idx + w]) {
            exits.push((idx + w, 1.0))
        };

        // Diagonals
        if self.in_bounds(Point::new(x - 1, y - 1)) && is_walkable(self.tiles[idx - w - 1]) {
            exits.push((idx - w - 1, 1.45));
        }
        if self.in_bounds(Point::new(x + 1, y - 1)) && is_walkable(self.tiles[idx - w + 1]) {
            exits.push((idx - w + 1, 1.45));
        }
        if self.in_bounds(Point::new(x - 1, y + 1)) && is_walkable(self.tiles[idx + w - 1]) {
            exits.push((idx + w - 1, 1.45));
        }
        if self.in_bounds(Point::new(x + 1, y + 1)) && is_walkable(self.tiles[idx + w + 1]) {
            exits.push((idx + w + 1, 1.45));
        }

        exits
    }

    fn get_pathing_distance(&self, idx1: usize, idx2: usize) -> f32 {
        let p1 = Point::new(idx1 % self.width as usize, idx1 / self.width as usize);
        let p2 = Point::new(idx2 % self.width as usize, idx2 / self.width as usize);
        bracket_lib::prelude::DistanceAlg::Pythagoras.distance2d(p1, p2)
    }
}

impl Algorithm2D for GameMap {
    fn dimensions(&self) -> Point {
        Point::new(self.width, self.height)
    }

    fn point2d_to_index(&self, pt: Point) -> usize {
        self.xy_idx(pt.x, pt.y)
    }

    fn index_to_point2d(&self, idx: usize) -> Point {
        Point::new(idx as i32 % self.width, idx as i32 / self.width)
    }
}
