use bevy::ecs::component::Component;
use bevy::prelude::{Color, Commands, Entity, Transform, TextureAtlas, Sprite, Vec3};
use bevy::camera::visibility::RenderLayers;
use bevy_ecs_tilemap::prelude::{TileColor, TilePos, TilemapId, TileTextureIndex};
use bracket_lib::prelude::Point;

use crate::components::Collider;
use crate::map::map::GRID_SIZE;
use crate::assets::{TileManifest, TileSpriteAssets};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Wall,
    Floor,
    DownStairs,
    UpStairs,
    Empty,
    Door,
    OpenDoor,
}

impl TileType {
    pub fn name(&self) -> &'static str {
        match self {
            TileType::Wall => "Wall",
            TileType::Floor => "Floor",
            TileType::DownStairs => "DownStairs",
            TileType::UpStairs => "UpStairs",
            TileType::Empty => "Empty",
            TileType::Door => "Door",
            TileType::OpenDoor => "OpenDoor",
        }
    }
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
        TileType::Door => false, // Closed doors are obstacles
        TileType::OpenDoor => true,
    }
}

pub fn is_opaque(tile: TileType) -> bool {
    match tile {
        TileType::Wall => true,
        TileType::Door => true,
        TileType::OpenDoor => false,
        _ => false,
    }
}

pub fn spawn_tile_entity(
    commands: &mut Commands,
    map_entity: Entity,
    tile_pos: TilePos,
    tile_type: TileType,
    pt: Point,
    tile_manifest: &TileManifest,
    tile_sprite_assets: &TileSpriteAssets,
) -> Entity {
    let asset = tile_manifest.tiles.get(tile_type.name()).expect("Tile type not in manifest");
    
    let sprite_path_parts: Vec<&str> = asset.sprite.split('#').collect();
    let texture_path = sprite_path_parts[0];
    let index = sprite_path_parts[1].parse::<usize>().unwrap_or_default();

    let texture_handle = tile_sprite_assets.handles.get(texture_path).expect("Texture handle not found").clone();
    let layout_handle = tile_sprite_assets.layouts.get(texture_path).expect("Layout handle not found").clone();

    // Determine scale to fit one game map tile (GRID_SIZE)
    let tile_size = asset.tile_size.unwrap_or(bevy::prelude::UVec2::new(16, 16));
    let scale_x = GRID_SIZE.x / tile_size.x as f32;
    let scale_y = GRID_SIZE.y / tile_size.y as f32;

    let mut command = commands.spawn((
        tile_pos,
        TilemapId(map_entity),
        TileTextureIndex(index as u32), // Add for tilemap rendering support
        TileColor(Color::BLACK), // Keep for visibility system compatibility
        Sprite::from_atlas_image(
            texture_handle,
            TextureAtlas {
                index,
                layout: layout_handle,
            },
        ),
        tile_type,
        TileVisibility::Hidden,
        TileExplored::Unexplored,
        Transform {
            translation: Vec3::new(pt.x as f32 * GRID_SIZE.x, pt.y as f32 * GRID_SIZE.y, 0.0),
            scale: Vec3::new(scale_x, scale_y, 1.0),
            ..Default::default()
        },
        RenderLayers::layer(1),
    ));

    if !is_walkable(tile_type) {
        command.insert(Collider);
    }

    command.id()
}
