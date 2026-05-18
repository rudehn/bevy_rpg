//! Grid-based map resource and pathfinding integration.

use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, BaseMap, DistanceAlg, Point, SmallVec};

use crate::components::{Collider, MovementMode, Position};
use crate::map::tile::{
    can_entity_enter_tile, is_opaque, is_passable, is_pathing_blocker, Decoration, LiquidType,
    TerrainType, Tile,
};

// =====================================================================
// Map resource
// =====================================================================

#[derive(Default, Clone, Resource)]
pub struct Map {
    pub name: String,
    pub tiles: Vec<Tile>,
    pub explored_tiles: Vec<bool>,
    pub blocked: Vec<bool>,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
}

impl Map {
    pub fn new<S: ToString>(depth: i32, width: i32, height: i32, name: S) -> Self {
        let count = (width * height) as usize;
        Self {
            name: name.to_string(),
            tiles: vec![
                Tile {
                    terrain: TerrainType::Wall,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
                count
            ],
            explored_tiles: vec![false; count],
            blocked: vec![false; count],
            width,
            height,
            depth,
        }
    }

    pub fn xy_idx(&self, x: i32, y: i32) -> usize {
        (y as usize * self.width as usize) + x as usize
    }

    pub fn idx_xy(&self, idx: usize) -> (i32, i32) {
        (idx as i32 % self.width, idx as i32 / self.width)
    }

    pub fn width(&self) -> i32 {
        self.width
    }
    pub fn height(&self) -> i32 {
        self.height
    }
    pub fn depth(&self) -> i32 {
        self.depth
    }

    pub fn in_bounds(&self, pt: Point) -> bool {
        pt.x >= 0 && pt.x < self.width && pt.y >= 0 && pt.y < self.height
    }

    pub fn get_tile(&self, pt: Point) -> Option<Tile> {
        if self.in_bounds(pt) {
            Some(self.tiles[self.xy_idx(pt.x, pt.y)])
        } else {
            None
        }
    }

    pub fn set_tile(&mut self, pt: Point, terrain: TerrainType) {
        if self.in_bounds(pt) {
            let idx = self.xy_idx(pt.x, pt.y);
            self.tiles[idx].terrain = terrain;
        }
    }

    pub fn set_liquid(&mut self, pt: Point, liquid: LiquidType) {
        if self.in_bounds(pt) {
            let idx = self.xy_idx(pt.x, pt.y);
            self.tiles[idx].liquid = liquid;
        }
    }

    pub fn get_pathing_cost(&self, x: i32, y: i32) -> Option<f32> {
        if !self.in_bounds(Point::new(x, y)) {
            return None;
        }
        let idx = self.xy_idx(x, y);
        let tile = self.tiles[idx];

        if !is_passable(tile) {
            return None;
        }
        if tile.liquid == LiquidType::Chasm {
            return None;
        }
        if is_pathing_blocker(tile) {
            return Some(5.0);
        }
        if self.blocked.get(idx).copied().unwrap_or(false) {
            return Some(10.0);
        }
        let dec_cost = tile.decoration.movement_cost();
        if dec_cost > 1.0 {
            return Some(dec_cost);
        }
        Some(1.0)
    }
}

// =====================================================================
// bracket-lib integration
// =====================================================================

impl BaseMap for Map {
    fn is_opaque(&self, idx: usize) -> bool {
        is_opaque(self.tiles[idx])
    }

    fn get_available_exits(&self, idx: usize) -> SmallVec<[(usize, f32); 10]> {
        let mut exits = SmallVec::new();
        let (x, y) = self.idx_xy(idx);

        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x + dx;
                let ny = y + dy;
                if let Some(base_cost) = self.get_pathing_cost(nx, ny) {
                    let next_idx = self.xy_idx(nx, ny);
                    let cost = if dx != 0 && dy != 0 {
                        base_cost * 1.45
                    } else {
                        base_cost
                    };
                    exits.push((next_idx, cost));
                }
            }
        }
        exits
    }

    fn get_pathing_distance(&self, idx1: usize, idx2: usize) -> f32 {
        let (x1, y1) = self.idx_xy(idx1);
        let (x2, y2) = self.idx_xy(idx2);
        DistanceAlg::Pythagoras.distance2d(Point::new(x1, y1), Point::new(x2, y2))
    }
}

impl Algorithm2D for Map {
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

// =====================================================================
// MapWithMode — mode-aware pathfinding wrapper
// =====================================================================

pub struct MapWithMode<'a> {
    pub map: &'a Map,
    pub mode: MovementMode,
}

impl<'a> MapWithMode<'a> {
    fn get_pathing_cost(&self, x: i32, y: i32) -> Option<f32> {
        if !self.map.in_bounds(Point::new(x, y)) {
            return None;
        }
        let idx = self.map.xy_idx(x, y);
        let tile = self.map.tiles[idx];

        if !is_passable(tile) {
            return None;
        }

        match self.mode {
            MovementMode::Land => {
                if tile.liquid == LiquidType::Chasm {
                    return None;
                }
                if tile.liquid == LiquidType::Water {
                    return None;
                }
                if is_pathing_blocker(tile) {
                    return Some(5.0);
                }
                if self.map.blocked.get(idx).copied().unwrap_or(false) {
                    return Some(10.0);
                }
                let dec = tile.decoration.movement_cost();
                if dec > 1.0 {
                    return Some(dec);
                }
                Some(1.0)
            }
            MovementMode::ImmuneToWater => {
                if tile.liquid == LiquidType::Chasm {
                    return None;
                }
                if tile.liquid == LiquidType::Water {
                    return Some(tile.decoration.movement_cost().max(1.0));
                }
                if is_pathing_blocker(tile) {
                    return Some(5.0);
                }
                if self.map.blocked.get(idx).copied().unwrap_or(false) {
                    return Some(10.0);
                }
                let dec = tile.decoration.movement_cost();
                if dec > 1.0 {
                    return Some(dec);
                }
                Some(1.0)
            }
            MovementMode::RestrictedToLiquid => {
                if !can_entity_enter_tile(tile, self.mode) {
                    return None;
                }
                if self.map.blocked.get(idx).copied().unwrap_or(false) {
                    return Some(10.0);
                }
                let dec = tile.decoration.movement_cost();
                if dec > 1.0 {
                    return Some(dec);
                }
                Some(1.0)
            }
        }
    }
}

impl<'a> BaseMap for MapWithMode<'a> {
    fn is_opaque(&self, idx: usize) -> bool {
        is_opaque(self.map.tiles[idx])
    }

    fn get_available_exits(&self, idx: usize) -> SmallVec<[(usize, f32); 10]> {
        let mut exits = SmallVec::new();
        let (x, y) = self.map.idx_xy(idx);

        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x + dx;
                let ny = y + dy;
                if let Some(base_cost) = self.get_pathing_cost(nx, ny) {
                    let next_idx = self.map.xy_idx(nx, ny);
                    let cost = if dx != 0 && dy != 0 {
                        base_cost * 1.45
                    } else {
                        base_cost
                    };
                    exits.push((next_idx, cost));
                }
            }
        }
        exits
    }

    fn get_pathing_distance(&self, idx1: usize, idx2: usize) -> f32 {
        let (x1, y1) = self.map.idx_xy(idx1);
        let (x2, y2) = self.map.idx_xy(idx2);
        DistanceAlg::Pythagoras.distance2d(Point::new(x1, y1), Point::new(x2, y2))
    }
}

impl<'a> Algorithm2D for MapWithMode<'a> {
    fn dimensions(&self) -> Point {
        Point::new(self.map.width, self.map.height)
    }

    fn point2d_to_index(&self, pt: Point) -> usize {
        self.map.xy_idx(pt.x, pt.y)
    }

    fn index_to_point2d(&self, idx: usize) -> Point {
        Point::new(idx as i32 % self.map.width, idx as i32 / self.map.width)
    }
}

// =====================================================================
// Systems
// =====================================================================

/// Marks tiles occupied by Collider entities as blocked in the Map.
pub fn populate_blocked_tiles(
    mut map: ResMut<Map>,
    collider_query: Query<&Position, With<Collider>>,
) {
    for b in map.blocked.iter_mut() {
        *b = false;
    }
    for pos in collider_query.iter() {
        let pt = Point::new(pos.x, pos.y);
        if map.in_bounds(pt) {
            let idx = map.xy_idx(pos.x, pos.y);
            map.blocked[idx] = true;
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_map(width: i32, height: i32, tiles: Vec<Tile>) -> Map {
        let count = (width * height) as usize;
        assert_eq!(tiles.len(), count);
        Map {
            name: "test".to_string(),
            tiles,
            explored_tiles: vec![false; count],
            blocked: vec![false; count],
            width,
            height,
            depth: 1,
        }
    }

    fn floor() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::None, decoration: Decoration::None }
    }
    fn wall() -> Tile {
        Tile { terrain: TerrainType::Wall, liquid: LiquidType::None, decoration: Decoration::None }
    }
    fn deep_water() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::Water, decoration: Decoration::None }
    }
    fn shallow_water() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::ShallowWater, decoration: Decoration::None }
    }
    fn lava() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::Lava, decoration: Decoration::None }
    }
    fn chasm() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::Chasm, decoration: Decoration::None }
    }

    // ---- Map::get_pathing_cost ----

    #[test]
    fn pathing_cost_floor_is_one() {
        let map = make_map(3, 3, vec![floor(); 9]);
        assert_eq!(map.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn pathing_cost_wall_is_none() {
        let map = make_map(3, 3, vec![wall(); 9]);
        assert_eq!(map.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn pathing_cost_deep_water_is_five() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        assert_eq!(map.get_pathing_cost(1, 1), Some(5.0));
    }

    #[test]
    fn pathing_cost_out_of_bounds_is_none() {
        let map = make_map(3, 3, vec![floor(); 9]);
        assert_eq!(map.get_pathing_cost(-1, 0), None);
        assert_eq!(map.get_pathing_cost(3, 0), None);
    }

    #[test]
    fn pathing_cost_blocked_tile_is_ten() {
        let mut map = make_map(3, 3, vec![floor(); 9]);
        map.blocked[4] = true;
        assert_eq!(map.get_pathing_cost(1, 1), Some(10.0));
    }

    // ---- MapWithMode pathing costs ----

    #[test]
    fn land_mode_deep_water_impassable() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };
        assert_eq!(mwm.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn land_mode_floor_costs_one() {
        let map = make_map(3, 3, vec![floor(); 9]);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn land_mode_lava_costs_five() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = lava();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(5.0));
    }

    #[test]
    fn immune_to_water_deep_water_costs_one() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::ImmuneToWater };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn immune_to_water_floor_costs_one() {
        let map = make_map(3, 3, vec![floor(); 9]);
        let mwm = MapWithMode { map: &map, mode: MovementMode::ImmuneToWater };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn restricted_to_liquid_deep_water_costs_one() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn restricted_to_liquid_dry_floor_is_impassable() {
        let map = make_map(3, 3, vec![floor(); 9]);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        assert_eq!(mwm.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn restricted_to_liquid_shallow_water_costs_one() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = shallow_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn restricted_to_liquid_wall_is_impassable() {
        let map = make_map(3, 3, vec![wall(); 9]);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        assert_eq!(mwm.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn all_modes_wall_is_impassable() {
        let map = make_map(3, 3, vec![wall(); 9]);
        for mode in [MovementMode::Land, MovementMode::ImmuneToWater, MovementMode::RestrictedToLiquid] {
            let mwm = MapWithMode { map: &map, mode };
            assert_eq!(mwm.get_pathing_cost(1, 1), None, "mode {:?}", mode);
        }
    }

    #[test]
    fn all_modes_out_of_bounds_is_none() {
        let map = make_map(3, 3, vec![floor(); 9]);
        for mode in [MovementMode::Land, MovementMode::ImmuneToWater, MovementMode::RestrictedToLiquid] {
            let mwm = MapWithMode { map: &map, mode };
            assert_eq!(mwm.get_pathing_cost(-1, 0), None);
        }
    }

    // ---- MapWithMode::get_available_exits ----

    #[test]
    fn land_mode_excludes_water_from_exits() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };
        let exits = mwm.get_available_exits(0);
        let center_exit = exits.iter().find(|(idx, _)| *idx == 4);
        assert!(center_exit.is_none(), "land mode should not pathfind into deep water");
    }

    #[test]
    fn restricted_to_liquid_no_exits_to_dry_floor() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        let exits = mwm.get_available_exits(4);
        assert!(exits.is_empty(), "no exits to dry floor");
    }

    #[test]
    fn restricted_to_liquid_exits_to_adjacent_water() {
        let tiles = vec![deep_water(); 9];
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        let exits = mwm.get_available_exits(4);
        assert_eq!(exits.len(), 8);
    }

    // ---- A* integration ----

    #[test]
    fn land_mode_paths_around_water() {
        let mut tiles = vec![floor(); 15];
        tiles[5 + 2] = deep_water();
        let map = make_map(5, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };
        let path = bracket_lib::prelude::a_star_search(
            mwm.point2d_to_index(Point::new(0, 1)),
            mwm.point2d_to_index(Point::new(4, 1)),
            &mwm,
        );
        assert!(path.success);
        let water_idx = map.xy_idx(2, 1);
        assert!(!path.steps.contains(&water_idx));
    }

    #[test]
    fn restricted_to_liquid_paths_through_water_only() {
        let mut tiles = vec![wall(); 15];
        for x in 0..5 {
            tiles[5 + x] = deep_water();
        }
        let map = make_map(5, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        let path = bracket_lib::prelude::a_star_search(
            mwm.point2d_to_index(Point::new(0, 1)),
            mwm.point2d_to_index(Point::new(4, 1)),
            &mwm,
        );
        assert!(path.success);
        assert_eq!(path.steps.len(), 5);
    }

    #[test]
    fn restricted_to_liquid_no_path_across_dry_gap() {
        let tiles = vec![deep_water(), deep_water(), floor(), deep_water(), deep_water()];
        let map = make_map(5, 1, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        let path = bracket_lib::prelude::a_star_search(
            mwm.point2d_to_index(Point::new(0, 0)),
            mwm.point2d_to_index(Point::new(4, 0)),
            &mwm,
        );
        assert!(!path.success);
    }

    // ---- Chasm ----

    #[test]
    fn pathing_cost_chasm_is_none() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = chasm();
        let map = make_map(3, 3, tiles);
        assert_eq!(map.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn land_mode_chasm_is_impassable() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = chasm();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };
        assert_eq!(mwm.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn immune_to_water_chasm_is_impassable() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = chasm();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::ImmuneToWater };
        assert_eq!(mwm.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn restricted_to_liquid_chasm_is_impassable() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = chasm();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        assert_eq!(mwm.get_pathing_cost(1, 1), None);
    }
}
