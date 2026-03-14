use bevy::camera::visibility::RenderLayers;
use bevy::ecs::component::Component;
use serde::{Deserialize, Serialize};
use bevy::prelude::{
    Commands, Entity, InheritedVisibility, Sprite, TextureAtlas, Transform, Vec3, ViewVisibility,
    Visibility,
};
use bracket_lib::prelude::Point;

use crate::assets::{TileManifest, TileSpriteAssets};
use crate::components::Collider;
use crate::map::map::GRID_SIZE;

#[derive(Component)]
pub struct TileMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Component, Serialize, Deserialize)]
pub enum TerrainType {
    #[default]
    Wall,
    Floor,
    DownStairs,
    UpStairs,
    Empty,
    Door,
    OpenDoor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Component, Serialize, Deserialize)]
pub enum LiquidType {
    #[default]
    None,
    Water,
    ShallowWater,
    Lava,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: TerrainType,
    pub liquid: LiquidType,
}

impl TerrainType {
    pub fn name(&self) -> &'static str {
        match self {
            TerrainType::Wall => "Wall",
            TerrainType::Floor => "Floor",
            TerrainType::DownStairs => "DownStairs",
            TerrainType::UpStairs => "UpStairs",
            TerrainType::Empty => "Empty",
            TerrainType::Door => "Door",
            TerrainType::OpenDoor => "OpenDoor",
        }
    }
}

impl LiquidType {
    pub fn name(&self) -> &'static str {
        match self {
            LiquidType::None => "None",
            LiquidType::Water => "Water",
            LiquidType::ShallowWater => "ShallowWater",
            LiquidType::Lava => "Lava",
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

pub fn is_walkable(tile: Tile) -> bool {
    // Both terrain and liquid must be walkable
    let terrain_walkable = match tile.terrain {
        TerrainType::Wall => false,
        TerrainType::Floor => true,
        TerrainType::DownStairs => true,
        TerrainType::UpStairs => true,
        TerrainType::Empty => false,
        TerrainType::Door => false,
        TerrainType::OpenDoor => true,
    };

    let liquid_walkable = match tile.liquid {
        LiquidType::None => true,
        LiquidType::Water => true,
        LiquidType::ShallowWater => true,
        LiquidType::Lava => false,
    };

    terrain_walkable && liquid_walkable
}

pub fn is_passable(tile: Tile) -> bool {
    // Topologically passable: anywhere an entity *could* go, or doors.
    // This is used for connectivity checks like the ChokeMap.
    match tile.terrain {
        TerrainType::Wall => false,
        TerrainType::Empty => false,
        _ => true, // Doors, floors, stairs are all passable connections
    }
}

pub fn is_opaque(tile: Tile) -> bool {
    // If either layer is opaque, the tile is opaque
    match tile.terrain {
        TerrainType::Wall => true,
        TerrainType::Door => true,
        _ => false,
    }
}

pub fn spawn_tile_entity(
    commands: &mut Commands,
    _map_entity: Entity,
    tile: Tile,
    pt: Point,
    tile_manifest: &TileManifest,
    tile_sprite_assets: &TileSpriteAssets,
) -> Entity {
    let terrain_asset = tile_manifest
        .tiles
        .get(tile.terrain.name())
        .expect("Terrain type not in manifest");

    let (texture_path, index) = crate::assets::parse_sprite_path(&terrain_asset.sprite);

    let texture_handle = tile_sprite_assets
        .handles
        .get(texture_path)
        .expect("Texture handle not found")
        .clone();
    let layout_handle = tile_sprite_assets
        .layouts
        .get(texture_path)
        .expect("Layout handle not found")
        .clone();

    // Determine scale to fit one game map tile (GRID_SIZE)
    let tile_size = terrain_asset
        .tile_size
        .unwrap_or(bevy::prelude::UVec2::new(16, 16));
    let scale_x = GRID_SIZE.x / tile_size.x as f32;
    let scale_y = GRID_SIZE.y / tile_size.y as f32;

    let mut command = commands.spawn((
        TileMarker,
        Sprite::from_atlas_image(
            texture_handle,
            TextureAtlas {
                index,
                layout: layout_handle,
            },
        ),
        tile.terrain,
        tile.liquid,
        TileVisibility::Hidden,
        TileExplored::Unexplored,
        Transform {
            translation: Vec3::new(pt.x as f32 * GRID_SIZE.x, pt.y as f32 * GRID_SIZE.y, 0.0),
            scale: Vec3::new(scale_x, scale_y, 1.0),
            ..Default::default()
        },
        Visibility::Hidden,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        RenderLayers::layer(1),
    ));

    if !is_walkable(tile) {
        command.insert(Collider);
    }

    let tile_entity = command.id();

    // If there's a liquid, spawn it as a child overlay
    if tile.liquid != LiquidType::None {
        let liquid_asset = tile_manifest
            .tiles
            .get(tile.liquid.name())
            .expect("Liquid type not in manifest");
        let (l_texture_path, l_index) = crate::assets::parse_sprite_path(&liquid_asset.sprite);

        let l_texture_handle = tile_sprite_assets
            .handles
            .get(l_texture_path)
            .expect("Liquid texture not found")
            .clone();
        let l_layout_handle = tile_sprite_assets
            .layouts
            .get(l_texture_path)
            .expect("Liquid layout not found")
            .clone();

        let l_child = commands
            .spawn((
                Sprite::from_atlas_image(
                    l_texture_handle,
                    TextureAtlas {
                        index: l_index,
                        layout: l_layout_handle,
                    },
                ),
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.1)), // Slightly above terrain
                RenderLayers::layer(1),
            ))
            .id();

        commands.entity(tile_entity).add_child(l_child);
    }

    tile_entity
}
