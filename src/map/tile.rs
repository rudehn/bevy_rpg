use bevy::ecs::component::Component;
use bevy::prelude::{Color, Commands, Entity, Transform};
use bevy_ecs_tilemap::prelude::{TileBundle, TileColor, TilePos, TileTextureIndex, TilemapId};
use bracket_lib::prelude::Point;

use crate::components::Collider;
use crate::map::map::{GRID_SIZE, MapId};

pub const FLOOR: usize = 49;
pub const WALL: usize = 40;
pub const DOWN_STAIRS: usize = 45;
pub const UP_STAIRS: usize = 21;
pub const SOLDIER: usize = 97;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Wall,
    Floor,
    DownStairs,
    UpStairs,
    Empty,
}

#[derive(Component, Default, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TileVisibility {
    #[default]
    Hidden,
    Visible,
}

#[derive(Component, Default, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TileExplored {
    #[default]
    Unexplored,
    Explored,
}

pub fn is_walkable(tile: TileType) -> bool {
    match tile {
        TileType::Wall => false,
        TileType::Floor => true,
        TileType::DownStairs => true,
        TileType::UpStairs => true,
        TileType::Empty => false,
    }
}

pub fn is_opaque(tile: TileType) -> bool {
    matches!(tile, TileType::Wall)
}

pub fn tile_texture(tile: TileType) -> usize {
    match tile {
        TileType::Floor => FLOOR,
        TileType::Wall => WALL,
        TileType::DownStairs => DOWN_STAIRS,
        TileType::UpStairs => UP_STAIRS,
        TileType::Empty => FLOOR,
    }
}

pub fn spawn_tile_entity(
    commands: &mut Commands,
    map_entity: Entity,
    tile_pos: TilePos,
    tile_type: TileType,
    pt: Point,
    map_id: MapId,
) -> Entity {
    let texture_index = tile_texture(tile_type);

    let mut command = commands.spawn((
        TileBundle {
            position: tile_pos,
            tilemap_id: TilemapId(map_entity),
            texture_index: TileTextureIndex(texture_index as u32),
            color: TileColor(Color::BLACK), // Initially black for fog of war
            ..Default::default()
        },
        tile_type,
        TileVisibility::Hidden,
        TileExplored::Unexplored,
        Transform::from_xyz(pt.x as f32 * GRID_SIZE.x, pt.y as f32 * GRID_SIZE.y, 0.0),
        map_id,
    ));

    if !is_walkable(tile_type) {
        command.insert(Collider);
    }
    if is_opaque(tile_type) {
        // command.insert(LightOccluder2d::default());
        // command.insert(LightOccluder2d {
        //     shape: LightOccluder2dShape::Rectangle {
        //         half_size: Vec2::new(GRID_SIZE.x / 2.0, GRID_SIZE.y / 2.0),
        //     },
        // });
    }

    command.id()
}
