use bracket_lib::noise::{FastNoise, NoiseType};
use rand::Rng;
// use rltk::{ BaseMap, Algorithm2D, Point };
// use specs::prelude::*;
use shipyard::Unique;
use std::collections::{HashMap, HashSet};
use std::slice::{Iter, IterMut};
mod tiletype;
use bracket_lib::algorithm_traits::{Algorithm2D, BaseMap};
use bracket_lib::geometry::Point;
use bracket_lib::prelude::{DistanceAlg, FontCharType, RGB, SmallVec, to_cp437};
pub use tiletype::*;
mod color;
mod features;
mod light;

use std::ops::{Index, IndexMut};

mod builders;
pub use builders::level_builder;
pub use color::*;

mod dungeon;

pub use dungeon::{MasterDungeonMap, freeze_level_entities, level_transition, thaw_level_entities};
pub use features::*;
pub use light::*;

use crate::components::Position;
use crate::map::tiletype::TileFlags;
use crate::settings::{MAP_HEIGHT, MAP_WIDTH};

// Dungeon flags
// enum tileFlags {
// 	DISCOVERED					= Fl(0),
// 	VISIBLE						= Fl(1),	// cell has sufficient light and is in field of view, ready to draw.
// 	HAS_PLAYER					= Fl(2),
// 	HAS_MONSTER					= Fl(3),
// 	HAS_DORMANT_MONSTER			= Fl(4),	// hidden monster on the square
// 	HAS_ITEM					= Fl(5),
// 	IN_FIELD_OF_VIEW			= Fl(6),	// player has unobstructed line of sight whether or not there is enough light
// 	WAS_VISIBLE					= Fl(7),
// 	HAS_STAIRS                  = Fl(8),
//     SEARCHED_FROM_HERE          = Fl(9),    // player already auto-searched here; can't auto-search here again
// 	IS_IN_SHADOW				= Fl(10),	// so that a player gains an automatic stealth bonus
// 	MAGIC_MAPPED				= Fl(11),
// 	ITEM_DETECTED				= Fl(12),
// 	CLAIRVOYANT_VISIBLE			= Fl(13),
// 	WAS_CLAIRVOYANT_VISIBLE		= Fl(14),
// 	CLAIRVOYANT_DARKENED		= Fl(15),	// magical blindness from a cursed ring of clairvoyance
// 	CAUGHT_FIRE_THIS_TURN		= Fl(16),	// so that fire does not spread asymmetrically
// 	PRESSURE_PLATE_DEPRESSED	= Fl(17),	// so that traps do not trigger repeatedly while you stand on them
// 	STABLE_MEMORY				= Fl(18),	// redraws will be pulled from the memory array, not recalculated
// 	KNOWN_TO_BE_TRAP_FREE		= Fl(19),	// keep track of where the player has stepped or watched monsters step as he knows no traps are there
// 	IS_IN_PATH					= Fl(20),	// the yellow trail leading to the cursor
// 	IN_LOOP						= Fl(21),	// this cell is part of a terrain loop
// 	IS_CHOKEPOINT				= Fl(22),	// if this cell is blocked, part of the map will be rendered inaccessible
// 	IS_GATE_SITE				= Fl(23),	// consider placing a locked door here
// 	IS_IN_ROOM_MACHINE			= Fl(24),
// 	IS_IN_AREA_MACHINE			= Fl(25),
// 	IS_POWERED					= Fl(26),	// has been activated by machine power this turn (flag can probably be eliminated if needed)
// 	IMPREGNABLE					= Fl(27),	// no tunneling allowed!
// 	TERRAIN_COLORS_DANCING		= Fl(28),	// colors here will sparkle when the game is idle
// 	TELEPATHIC_VISIBLE			= Fl(29),	// potions of telepathy let you see through other creatures' eyes
// 	WAS_TELEPATHIC_VISIBLE		= Fl(30),	// potions of telepathy let you see through other creatures' eyes

// 	IS_IN_MACHINE				= (IS_IN_ROOM_MACHINE | IS_IN_AREA_MACHINE), 	// sacred ground; don't generate items here, or teleport randomly to it

// 	PERMANENT_TILE_FLAGS = (DISCOVERED | MAGIC_MAPPED | ITEM_DETECTED | HAS_ITEM | HAS_DORMANT_MONSTER
// 							| HAS_STAIRS | SEARCHED_FROM_HERE | PRESSURE_PLATE_DEPRESSED
// 							| STABLE_MEMORY | KNOWN_TO_BE_TRAP_FREE | IN_LOOP
// 							| IS_CHOKEPOINT | IS_GATE_SITE | IS_IN_MACHINE | IMPREGNABLE),

// 	ANY_KIND_OF_VISIBLE			= (VISIBLE | CLAIRVOYANT_VISIBLE | TELEPATHIC_VISIBLE),
// };

#[derive(Clone, Copy, PartialEq)]
pub struct Cell {
    pub dungeon: TileType,
    pub liquid: TileType,
    pub features: TileType,
    pub flags: TileFlags,
    pub remembered_fg: Color,
    pub remembered_bg: Color,
    pub remembered_char: FontCharType,
}
impl Cell {
    /// Figure out which tile we should be returning to render
    pub fn get_highest_priority_tile(&self) -> TileType {
        let mut highest_priority = self.dungeon.data().priority;
        let mut highest_priority_tile = self.dungeon;

        for tile in [self.liquid, self.features] {
            let tile_priority = tile.data().priority;
            if tile_priority < highest_priority {
                highest_priority = tile_priority;
                highest_priority_tile = tile;
            }
        }
        highest_priority_tile
    }
    pub fn layers(&self) -> Vec<TileType> {
        let layers = vec![self.dungeon, self.liquid, self.features];
        layers
    }
    pub fn terrain_flags(&self) -> TileFlags {
        self.dungeon.data().flags | self.liquid.data().flags | self.features.data().flags
    }
    pub fn has_terrain_flags(&self, flags: TileFlags) -> bool {
        (flags & self.terrain_flags()) != 0
    }
    pub fn has_highest_priority_terrain_flags(&self, flags: TileFlags) -> bool {
        self.get_highest_priority_tile().has_flags(flags)
    }
    pub fn has_flags(&self, flags: TileFlags) -> bool {
        (flags & self.flags) != 0
    }
    pub fn set_flags(&mut self, flags: TileFlags) {
        self.flags |= flags;
    }
    pub fn clear_flags(&mut self, flags: TileFlags) {
        self.flags &= !flags;
    }
    pub fn default() -> Self {
        let data = TileType::Wall.data();
        Self {
            dungeon: TileType::Wall,
            liquid: TileType::Empty,
            features: TileType::Empty,
            flags: 0,
            remembered_fg: data.fg.unwrap_or(Color::default()),
            remembered_bg: data.bg.unwrap_or(Color::default()),
            remembered_char: to_cp437(data.glyph),
        }
    }
}

/// A generic, rectangular grid structure for holding any type T.
#[derive(Clone)]
pub struct Grid<T>
where
    T: Copy + Clone + PartialEq, // Requires T to be copyable for easy initialization
{
    pub data: Vec<T>,
    pub width: i32,
    pub height: i32,
}

impl<T> Grid<T>
where
    T: Copy + Clone + PartialEq,
{
    /// Creates a new Grid initialized with a default value.
    pub fn new(width: i32, height: i32, default_value: T) -> Self {
        let size = (width * height) as usize;
        Grid {
            data: vec![default_value; size],
            width,
            height,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Converts (x, y) coordinates to a flat array index.
    pub fn xy_idx(&self, x: i32, y: i32) -> usize {
        (y as usize * self.width as usize) + x as usize
    }

    /// Converts a flat index to (x, y) coordinates.
    pub fn idx_to_xy(&self, idx: usize) -> (i32, i32) {
        let x = idx as i32 % self.width;
        let y = idx as i32 / self.width;
        (x, y)
    }

    /// Checks if coordinates are within the map bounds.
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.width && y >= 0 && y < self.height
    }

    /// Safely gets an immutable reference to the element at (x, y).
    pub fn at(&self, x: i32, y: i32) -> Option<&T> {
        if self.in_bounds(x, y) {
            Some(&self.data[self.xy_idx(x, y)])
        } else {
            None
        }
    }

    /// Safely gets a mutable reference to the element at (x, y).
    pub fn at_mut(&mut self, x: i32, y: i32) -> Option<&mut T> {
        if self.in_bounds(x, y) {
            let idx = self.xy_idx(x, y);
            // SAFETY: Bounds check is done, so we can access directly
            Some(&mut self.data[idx])
        } else {
            None
        }
    }

    pub fn fill(&mut self, fill_value: T) {
        for idx in self.data.iter_mut() {
            *idx = fill_value;
        }
    }
    pub fn set(&mut self, x: i32, y: i32, value: T) {
        *self.at_mut(x, y).unwrap() = value;
    }

    /// Returns an immutable iterator over the elements of the grid.
    /// This allows iterating over the grid using `for item in &grid { ... }`.
    pub fn iter(&self) -> Iter<'_, T> {
        self.data.iter()
    }

    /// Returns a mutable iterator over the elements of the grid.
    /// This allows iterating over the grid using `for item in &mut grid { ... }`.
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        self.data.iter_mut()
    }

    pub fn replace(&mut self, initial: T, replaced: T) {
        for idx in self.data.iter_mut() {
            if *idx == initial {
                *idx = replaced;
            }
        }
    }
}

impl<T> Index<usize> for Grid<T>
where
    T: Copy + Clone + PartialEq,
{
    // The type of the output (the element we return)
    type Output = T;

    /// Defines the behavior for `grid[idx]`.
    fn index(&self, idx: usize) -> &Self::Output {
        // We defer to the underlying vector's index implementation.
        // This will panic if `idx` is out of bounds, matching standard vector behavior.
        &self.data[idx]
    }
}

impl<T> IndexMut<usize> for Grid<T>
where
    T: Copy + Clone + PartialEq,
{
    /// Defines the behavior for `&mut grid[idx]`.
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        // We defer to the underlying vector's index_mut implementation.
        // This will panic if `idx` is out of bounds.
        &mut self.data[idx]
    }
}

impl Grid<TileType> {
    /// Creates a new Grid<TileType> by extracting the dungeon layer from every Cell
    /// in the source Grid<Cell>.
    pub fn from_cell_grid(source_grid: &Grid<Cell>) -> Self {
        let width = source_grid.width;
        let height = source_grid.height;
        let size = (width * height) as usize;

        // Use map to efficiently convert the Vec<Cell> data into Vec<TileType> data
        let tiles: Vec<TileType> = source_grid.data.iter().map(|cell| cell.dungeon).collect();

        // Return the new Grid<TileType>
        Grid {
            data: tiles,
            width,
            height,
        }
    }
}

impl Grid<Cell> {
    /// Creates a new Grid<Cell> by setting the dungeon layer from the source Grid<TileType>
    /// and initializing other Cell fields to defaults (e.g., TileType::Nothing for liquid).
    pub fn to_cell_grid(source_grid: &Grid<TileType>) -> Self {
        let width = source_grid.width;
        let height = source_grid.height;

        // Use map to iterate over the TileType data and create new Cell structs
        let cells: Vec<Cell> = source_grid
            .data
            .iter()
            .map(|&tile_type| {
                let tile_data = tile_type.data();
                Cell {
                    dungeon: tile_type,
                    // Initialize the liquid layer to nothing/none
                    liquid: TileType::Empty,
                    features: TileType::Empty,
                    // Initialize flags to 0 (no terrain modification flags set)
                    flags: 0,
                    remembered_fg: tile_data.fg.unwrap_or(Color::default()),
                    remembered_bg: tile_data.bg.unwrap_or(Color::default()),
                    remembered_char: to_cp437(tile_data.glyph),
                }
            })
            .collect();

        // Return the new Grid<Cell>
        Grid {
            data: cells,
            width,
            height,
        }
    }
}

#[derive(Clone, Unique)]
pub struct Map {
    pub tiles: Grid<Cell>,
    pub depth: i32,
    pub name: String,
    pub bloodstains: HashSet<usize>,
    pub view_blocked: HashSet<usize>,
    pub outdoors: bool,
    pub light: Vec<RGB>,
    pub tile_light_cache: Vec<RGB>,
}

impl Default for Map {
    fn default() -> Self {
        Map::new(0, MAP_WIDTH, MAP_HEIGHT, "Default Map")
    }
}

impl Map {
    /// Generates an empty map, consisting entirely of solid walls
    pub fn new<S: ToString>(new_depth: i32, width: i32, height: i32, name: S) -> Map {
        let map_tile_count = (width * height) as usize;
        crate::spatial::set_size(map_tile_count);
        Map {
            tiles: Grid::new(width, height, Cell::default()),
            depth: new_depth,
            bloodstains: HashSet::new(),
            view_blocked: HashSet::new(),
            name: name.to_string(),
            outdoors: false,
            light: vec![RGB::from_f32(0.0, 0.0, 0.0); map_tile_count],
            tile_light_cache: vec![RGB::from_f32(0.0, 0.0, 0.0); map_tile_count],
        }
    }

    pub fn width(&self) -> i32 {
        self.tiles.width
    }
    pub fn height(&self) -> i32 {
        self.tiles.height
    }

    pub fn cell_has_terrain_flags(&self, x: i32, y: i32, flags: TileFlags) -> bool {
        if let Some(cell) = self.tiles.at(x, y) {
            cell.has_terrain_flags(flags)
        } else {
            false
        }
    }
    pub fn cell_has_highest_priority_terrain_flags(
        &self,
        x: i32,
        y: i32,
        flags: TileFlags,
    ) -> bool {
        if let Some(cell) = self.tiles.at(x, y) {
            cell.has_highest_priority_terrain_flags(flags)
        } else {
            false
        }
    }

    pub fn free_adjacent_tiles(&self, center_idx: usize, radius: i32) -> Vec<usize> {
        let mut free_tiles = Vec::new();

        let (center_x, center_y) = self.idx_to_xy(center_idx);

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let x = center_x + dx;
                let y = center_y + dy;

                if x < 0 || x >= self.width() || y < 0 || y >= self.height() {
                    continue;
                }

                let idx = self.xy_idx(x, y);

                // Tile is not blocked and no entity occupies it
                if !crate::spatial::is_blocked(idx) {
                    free_tiles.push(idx);
                }
            }
        }

        free_tiles
    }

    pub fn xy_idx(&self, x: i32, y: i32) -> usize {
        self.tiles.xy_idx(x, y)
    }

    pub fn idx_to_xy(&self, idx: usize) -> (i32, i32) {
        self.tiles.idx_to_xy(idx)
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        self.tiles.in_bounds(x, y)
    }

    /// Determine the cost to move to this cell
    /// None: Can't move to this cell
    /// Tile cost: No entities but the tile itself has a cost
    /// Entity cost: a large number that penalizes path search, but doesn't prevent it
    fn get_cell_cost(&self, x: i32, y: i32) -> Option<f32> {
        if !self.in_bounds(x, y) {
            return None;
        }

        let idx = self.xy_idx(x, y);
        let cell = &self.tiles.data[idx];
        // Is the map tile not passable?
        if cell.has_terrain_flags(T_OBSTRUCTS_PASSABILITY) {
            return None;
        }

        // Any entities in this cell?
        if crate::spatial::is_blocked(idx) {
            return Some(100.0);
        }
        // Return tile cost
        return Some(1.0);
        // let tt = self.tiles[idx as usize];
        // return Some(tile_cost(tt));
    }

    pub fn populate_blocked(&mut self) {
        crate::spatial::populate_blocked_from_map(self);
    }
}

impl Algorithm2D for Map {
    fn dimensions(&self) -> Point {
        Point::new(self.width(), self.height())
    }
}

impl BaseMap for Map {
    fn is_opaque(&self, idx: usize) -> bool {
        // Check bounds against the underlying data vector length
        if idx < self.tiles.data.len() {
            // 1. Check if the Cell's flags indicate vision obstruction (T_OBSTRUCTS_VISION)
            let cell = &self.tiles.data[idx];
            let blocks_sight_by_tile = cell.has_terrain_flags(T_OBSTRUCTS_VISION);

            // 2. Combine the tile obstruction check with the dynamic view_blocked set
            blocks_sight_by_tile || self.view_blocked.contains(&idx)
        } else {
            // Tiles outside the map are considered opaque (e.g., Void/Wall)
            true
        }
    }

    fn get_available_exits(&self, idx: usize) -> SmallVec<[(usize, f32); 10]> {
        let mut exits = SmallVec::new();
        let x = idx as i32 % self.width();
        let y = idx as i32 / self.width();
        let w = self.width() as usize;

        // Cardinal directions
        if let Some(cost) = self.get_cell_cost(x - 1, y) {
            exits.push((idx - 1, cost))
        };
        if let Some(cost) = self.get_cell_cost(x + 1, y) {
            exits.push((idx + 1, cost))
        };
        if let Some(cost) = self.get_cell_cost(x, y - 1) {
            exits.push((idx - w, cost))
        };
        if let Some(cost) = self.get_cell_cost(x, y + 1) {
            exits.push((idx + w, cost))
        };

        // Diagonals
        if let Some(cost) = self.get_cell_cost(x - 1, y - 1) {
            exits.push(((idx - w) - 1, cost))
        };
        if let Some(cost) = self.get_cell_cost(x + 1, y - 1) {
            exits.push(((idx - w) + 1, cost))
        };
        if let Some(cost) = self.get_cell_cost(x - 1, y + 1) {
            exits.push(((idx + w) - 1, cost))
        };
        if let Some(cost) = self.get_cell_cost(x + 1, y + 1) {
            exits.push(((idx + w) + 1, cost))
        };
        exits
    }

    fn get_pathing_distance(&self, idx1: usize, idx2: usize) -> f32 {
        let w = self.width() as usize;
        let p1 = Point::new(idx1 % w, idx1 / w);
        let p2 = Point::new(idx2 % w, idx2 / w);
        DistanceAlg::Pythagoras.distance2d(p1, p2)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    NW,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NoDirection, // No movement
}

impl Direction {
    pub fn cardinals() -> Vec<Direction> {
        vec![Direction::N, Direction::E, Direction::S, Direction::W]
    }
    pub fn iter() -> Vec<Direction> {
        vec![
            Direction::N,
            Direction::NE,
            Direction::E,
            Direction::SE,
            Direction::S,
            Direction::SW,
            Direction::W,
            Direction::NW,
        ]
    }
    fn opposite(&self) -> Self {
        match self {
            Direction::N => Direction::S,
            Direction::S => Direction::N,
            Direction::E => Direction::W,
            Direction::W => Direction::E,
            Direction::NW => Direction::SE,
            Direction::NE => Direction::SW,
            Direction::SW => Direction::NE,
            Direction::SE => Direction::NW,
            Direction::NoDirection => Direction::NoDirection,
        }
    }
    pub fn from_pos(current: &Position, target: &Position) -> Self {
        match target.x.cmp(&current.x) {
            std::cmp::Ordering::Less => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::NW,
                std::cmp::Ordering::Equal => Direction::W,
                std::cmp::Ordering::Greater => Direction::SW,
            },
            std::cmp::Ordering::Equal => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::N,
                std::cmp::Ordering::Equal => Direction::NoDirection,
                std::cmp::Ordering::Greater => Direction::S,
            },
            std::cmp::Ordering::Greater => match target.y.cmp(&current.y) {
                std::cmp::Ordering::Less => Direction::NE,
                std::cmp::Ordering::Equal => Direction::E,
                std::cmp::Ordering::Greater => Direction::SE,
            },
        }
    }

    pub fn offset(&self) -> Point {
        match self {
            Direction::NW => Point { x: -1, y: -1 },
            Direction::N => Point { x: 0, y: -1 },
            Direction::NE => Point { x: 1, y: -1 },
            Direction::E => Point { x: 1, y: 0 },
            Direction::SE => Point { x: 1, y: 1 },
            Direction::S => Point { x: 0, y: 1 },
            Direction::SW => Point { x: -1, y: 1 },
            Direction::W => Point { x: -1, y: 0 },
            Direction::NoDirection => Point { x: 0, y: 0 },
        }
    }
}

// This is used for pathfinding for agents, so we can add custom logic like monsters avoiding deep water
pub enum AgentType {
    Player,
    Monster,
}
pub struct AgentPathingMap {
    map: Map,
    agent_type: AgentType,
}

impl AgentPathingMap {
    pub fn new(map: &Map, agent_type: AgentType) -> Self {
        Self {
            map: map.clone(),
            agent_type,
        }
    }

    /// Calculates the additional movement cost for a specific agent type.
    fn get_agent_cost(&self, idx: usize, agent_type: &AgentType) -> Option<f32> {
        let cell = &self.map.tiles.data[idx];
        let mut cost = 0.0; // Default cost adjustment

        match agent_type {
            AgentType::Player => {}
            AgentType::Monster => {
                // Monsters don't path through deep water or lava
                if cell.has_highest_priority_terrain_flags(T_PATHING_BLOCKER) {
                    return None;
                }
            }
        }

        // Add a small penalty for moving into occupied tiles (less important for Dijkstra)
        // Note: Entity occupation checks are usually handled outside the BaseMap implementation.

        Some(cost)
    }
}

impl BaseMap for AgentPathingMap {
    // --- OPAQUENESS CHECK (WHO CAN SEE THROUGH WHAT) ---
    fn is_opaque(&self, idx: usize) -> bool {
        // Delegate the complex logic to the underlying Map, passing the context
        self.map.is_opaque(idx)
    }

    // --- AVAILABLE EXITS (MOVEMENT COST) ---
    fn get_available_exits(&self, idx: usize) -> SmallVec<[(usize, f32); 10]> {
        let mut exits = SmallVec::new();
        let (x, y) = self.map.idx_to_xy(idx);
        let x = idx as i32 % self.map.width();
        let w = self.map.width() as usize;

        // Array of movement options (dx, dy, diagonal_cost_multiplier)
        let moves = [
            (-1, 0, 1.0),
            (1, 0, 1.0),
            (0, -1, 1.0),
            (0, 1, 1.0), // Cardinal
            (-1, -1, 1.45),
            (1, -1, 1.45),
            (-1, 1, 1.45),
            (1, 1, 1.45), // Diagonals
        ];

        for (dx, dy, cost_mult) in moves.iter() {
            let next_x = x + dx;
            let next_y = y + dy;

            if let Some(base_cost) = self.map.get_cell_cost(next_x, next_y) {
                let next_idx = self.map.xy_idx(next_x, next_y);

                // CRITICAL: Call the agent-aware cost function
                if let Some(agent_cost) = self.get_agent_cost(next_idx, &self.agent_type) {
                    let total_cost = (base_cost + agent_cost) * cost_mult;
                    exits.push((next_idx, total_cost));
                }
            }
        }
        exits
    }

    fn get_pathing_distance(&self, idx1: usize, idx2: usize) -> f32 {
        // Distance is generally independent of the agent type
        let w = self.map.width() as usize;
        let p1 = Point::new(idx1 % w, idx1 / w);
        let p2 = Point::new(idx2 % w, idx2 / w);
        DistanceAlg::Pythagoras.distance2d(p1, p2)
    }
}
