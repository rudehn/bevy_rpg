use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;
use std::collections::HashMap;
// Removed: use bevy_light_2d::prelude::{LightOccluder2d, LightOccluder2dShape};
use bracket_lib::prelude::{Algorithm2D, BaseMap, Point};

use crate::{
    components::{Position, Viewshed},
    game::AppState,
    map::tile::{TileExplored, TileType, TileVisibility, is_opaque, is_walkable},
    player::{Player, move_player},
};

pub const TILE_SIZE: TilemapTileSize = TilemapTileSize { x: 16.0, y: 16.0 };
pub const GRID_SIZE: TilemapGridSize = TilemapGridSize { x: 16.0, y: 16.0 };
pub const MAP_SIZE: TilemapSize = TilemapSize { x: 80, y: 60 };

#[derive(Resource, Default)]
pub struct PlayerPosition(pub Position);

#[derive(Resource, Default)]
pub struct MapHistory {
    pub maps: HashMap<i32, GameMap>,
}

#[derive(Resource)]
pub struct ActiveMap(pub MapId);

impl Default for ActiveMap {
    fn default() -> Self {
        ActiveMap(MapId(0))
    }
}

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TilemapPlugin) // Required by bevy_ecs_tilemap
            .init_resource::<MapHistory>()
            .init_resource::<ActiveMap>()
            .init_resource::<PlayerPosition>()
            .add_systems(
                Update,
                update_tile_visibility
                    .run_if(in_state(AppState::InGame))
                    .after(move_player),
            );
    }
}

// Tag for the entity that holds the map storage
#[derive(Component)]
pub struct DungeonMap;

// Tag for all entities that belong to a specific map instance
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapId(pub i32);

pub fn update_tile_visibility(
    player_query: Query<&Viewshed, (With<Player>, Changed<Viewshed>)>,
    mut tile_render_query: Query<(
        &TilePos,
        &mut TileColor,
        &mut TileVisibility,
        &mut TileExplored,
    )>,
) {
    let Ok(player_viewshed) = player_query.single() else {
        return;
    };

    let fov_tiles = &player_viewshed.visible_tiles;

    // Update tile visibility and color
    for (tile_pos, mut tile_color, mut tile_visibility, mut tile_explored) in
        tile_render_query.iter_mut()
    {
        let current_point = Point::new(tile_pos.x as i32, tile_pos.y as i32);

        if fov_tiles.contains(&current_point) {
            *tile_visibility = TileVisibility::Visible;
            *tile_explored = TileExplored::Explored;
            tile_color.0 = Color::WHITE; // Visible tiles are full bright
        } else {
            *tile_visibility = TileVisibility::Hidden;
            if *tile_explored == TileExplored::Explored {
                tile_color.0 = Color::srgb(0.5, 0.5, 0.5); // Explored but not visible are dim
            } else {
                tile_color.0 = Color::BLACK; // Unexplored and not visible are black
            }
        }
    }
}

/// A trait that defines the basic functions of a map.
pub trait Map: BaseMap + Algorithm2D {
    fn width(&self) -> i32;
    fn height(&self) -> i32;
    fn depth(&self) -> i32;

    fn get_tile(&self, pt: Point) -> Option<TileType>;
    fn set_tile(&mut self, pt: Point, tile: TileType);
}

#[derive(Default, Clone)]
pub struct GameMap {
    pub name: String,
    pub tiles: Vec<TileType>,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub candle_positions: Vec<Point>,
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
            candle_positions: Vec::new(),
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

    fn depth(&self) -> i32 {
        self.depth
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
        is_opaque(self.tiles[idx])
    }

    fn get_available_exits(
        &self,
        idx: usize,
    ) -> bracket_lib::prelude::SmallVec<[(usize, f32); 10]> {
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
/// A read-only adapter to view a `bevy_ecs_tilemap` as a `Map` trait object.
/// This allows pathfinding and other algorithms to run on the live ECS data.
/// It is constructed within a Bevy system.
pub struct EcsMap<'w, 's, 'a> {
    pub tile_storage: &'w TileStorage,
    pub tile_query: &'w Query<'w, 's, &'a TileType>,
    pub map_size: TilemapSize,
    pub depth: i32, // Added depth field
}

impl<'w, 's, 'a> Map for EcsMap<'w, 's, 'a> {
    fn width(&self) -> i32 {
        self.map_size.x as i32
    }

    fn height(&self) -> i32 {
        self.map_size.y as i32
    }

    fn get_tile(&self, pt: Point) -> Option<TileType> {
        if !self.in_bounds(pt) {
            return None;
        }
        let tile_pos = TilePos {
            x: pt.x as u32,
            y: pt.y as u32,
        };
        self.tile_storage
            .get(&tile_pos)
            .and_then(|tile_entity| self.tile_query.get(tile_entity).ok().copied())
    }

    /// This is a read-only adapter. Setting tiles must be done via Commands.
    fn set_tile(&mut self, _pt: Point, _tile: TileType) {
        panic!("EcsMap is a read-only adapter. Use Commands to modify the map.");
    }

    fn depth(&self) -> i32 {
        self.depth
    }
}

impl<'w, 's, 'a> BaseMap for EcsMap<'w, 's, 'a> {
    fn is_opaque(&self, idx: usize) -> bool {
        let pt = self.index_to_point2d(idx);
        match self.get_tile(pt) {
            Some(tile) => is_opaque(tile),
            _ => false,
        }
    }

    fn get_available_exits(
        &self,
        idx: usize,
    ) -> bracket_lib::prelude::SmallVec<[(usize, f32); 10]> {
        let mut exits = bracket_lib::prelude::SmallVec::new();
        let pt = self.index_to_point2d(idx);

        for dx in -1..=1 {
            for dy in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }

                let next_pt = Point::new(pt.x + dx, pt.y + dy);
                if self.in_bounds(next_pt) {
                    if let Some(tile) = self.get_tile(next_pt) {
                        if !matches!(tile, TileType::Wall) {
                            let next_idx = self.point2d_to_index(next_pt);
                            let distance = if dx == 0 || dy == 0 { 1.0 } else { 1.45 };
                            exits.push((next_idx, distance));
                        }
                    }
                }
            }
        }

        exits
    }

    fn get_pathing_distance(&self, idx1: usize, idx2: usize) -> f32 {
        let p1 = self.index_to_point2d(idx1);
        let p2 = self.index_to_point2d(idx2);
        bracket_lib::prelude::DistanceAlg::Pythagoras.distance2d(p1, p2)
    }
}

impl<'w, 's, 'a> Algorithm2D for EcsMap<'w, 's, 'a> {
    fn dimensions(&self) -> Point {
        Point::new(self.map_size.x as i32, self.map_size.y as i32)
    }

    fn point2d_to_index(&self, pt: Point) -> usize {
        (pt.y as usize * self.map_size.x as usize) + pt.x as usize
    }

    fn index_to_point2d(&self, idx: usize) -> Point {
        Point::new(idx as i32 % self.width(), idx as i32 / self.width())
    }
}
