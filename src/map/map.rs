use bevy::{ecs::system::command::insert_resource, prelude::*};
use bevy_ecs_tilemap::prelude::*;
use bracket_lib::prelude::{Algorithm2D, BaseMap, Point};

use crate::{
    components::Viewshed,
    game::AppState,
    map::tile::{TileExplored, TileType, TileVisibility, is_opaque, is_walkable},
    player::{Player, move_player},
};

/*
There are two map types.

1. The Map struct defined here. This grid based map handles all game logic, from map generation
   to collision and fog of war.

2. The Bevy_ecs_tilemap. This third party map handles all the rendering of all entities on the level.
   This handles sprites, visibility, pixel location, etc.

*/
pub const TILE_SIZE: TilemapTileSize = TilemapTileSize { x: 16.0, y: 16.0 };
pub const GRID_SIZE: TilemapGridSize = TilemapGridSize { x: 16.0, y: 16.0 };
pub const MAP_SIZE: TilemapSize = TilemapSize { x: 80, y: 60 };

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TilemapPlugin)
            .insert_resource(Map::default()) // This will always be the active level
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
pub struct DungeonECSMap; // Tag for entity holding the active ECS tilemap

// --------------------------------------------------------------------------------
// SYSTEMS
// --------------------------------------------------------------------------------

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

#[derive(Default, Clone, Resource)]
pub struct Map {
    pub name: String,
    pub tiles: Vec<TileType>,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
}

impl Map {
    /// Creates a new map of the given size, with all tiles set to `Wall`.
    pub fn new<S: ToString>(depth: i32, width: i32, height: i32, name: S) -> Self {
        let map_tile_count = (width * height) as usize;
        Self {
            name: name.to_string(),
            tiles: vec![TileType::Wall; map_tile_count],
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

    pub fn get_tile(&self, pt: Point) -> Option<TileType> {
        if self.in_bounds(pt) {
            let idx = self.xy_idx(pt.x, pt.y);
            Some(self.tiles[idx])
        } else {
            None
        }
    }

    pub fn set_tile(&mut self, pt: Point, tile: TileType) {
        if self.in_bounds(pt) {
            let idx = self.xy_idx(pt.x, pt.y);
            self.tiles[idx] = tile;
        }
    }
}

impl BaseMap for Map {
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
