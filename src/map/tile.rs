use bevy::ecs::component::Component;
use bevy::prelude::{Commands, Entity, Transform, Color};
use bevy_ecs_tilemap::prelude::{TileBundle, TilePos, TilemapId, TileTextureIndex, TileColor};
use bracket_lib::prelude::Point;

use crate::components::Collider;
use crate::map::map::GRID_SIZE;

pub const FLOOR: usize = 49;
pub const WALL: usize = 40;
pub const DOWN_STAIRS: usize = 61;
pub const SOLDIER: usize = 97;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Wall,
    Floor,
    DownStairs,
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
        TileType::Empty => FLOOR,
    }
}

pub fn spawn_tile_entity(
    commands: &mut Commands,
    map_entity: Entity,
    tile_pos: TilePos,
    tile_type: TileType,
    pt: Point,
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

