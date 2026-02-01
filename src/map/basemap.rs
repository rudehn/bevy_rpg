use bracket_lib::prelude::{Algorithm2D, BaseMap, Point};

use super::tile::TileType;

/// A trait that defines the basic functions of a map.
pub trait Map: BaseMap + Algorithm2D {
    fn width(&self) -> i32;
    fn height(&self) -> i32;

    fn get_tile(&self, pt: Point) -> Option<TileType>;
    fn set_tile(&mut self, pt: Point, tile: TileType);
}
