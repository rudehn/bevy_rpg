use bevy::camera::visibility::RenderLayers;
use bevy::ecs::component::Component;
use serde::{Deserialize, Serialize};
use bevy::prelude::{
    Commands, Entity, InheritedVisibility, Sprite, TextureAtlas, Transform, Vec3, ViewVisibility,
    Visibility, Text2d, TextFont, TextColor, default,
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
    /// Renders as Wall until discovered, then converts to Door.
    HiddenDoor,
    /// Requires a matching key item to open. Renders as a locked door.
    LockedDoor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Component, Serialize, Deserialize)]
pub enum LiquidType {
    #[default]
    None,
    Water,
    ShallowWater,
    Lava,
    /// Impassable void — no wreath, blocks everything.
    Chasm,
}

/// Purely visual decoration overlay on a tile. Does not affect walkability or opacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Decoration {
    #[default]
    None,
    Grass,
    TallGrass,
    DeadGrass,
    Rubble,
    Moss,
    Fungus,
    Cobweb,
    Bloodstain,
    ScorchedEarth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: TerrainType,
    pub liquid: LiquidType,
    pub decoration: Decoration,
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
            TerrainType::HiddenDoor => "HiddenDoor",
            TerrainType::LockedDoor => "LockedDoor",
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
            LiquidType::Chasm => "Chasm",
        }
    }
}

impl Decoration {
    pub fn name(&self) -> &'static str {
        match self {
            Decoration::None => "None",
            Decoration::Grass => "Grass",
            Decoration::TallGrass => "TallGrass",
            Decoration::DeadGrass => "DeadGrass",
            Decoration::Rubble => "Rubble",
            Decoration::Moss => "Moss",
            Decoration::Fungus => "Fungus",
            Decoration::Cobweb => "Cobweb",
            Decoration::Bloodstain => "Bloodstain",
            Decoration::ScorchedEarth => "ScorchedEarth",
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

/// Marker for child sprite entities that render decoration overlays on tiles.
#[derive(Component)]
pub struct DecorationOverlay;

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
        TerrainType::HiddenDoor => false,  // Not walkable until discovered → Door
        TerrainType::LockedDoor => false,  // Not walkable until unlocked → Door
    };

    let liquid_walkable = match tile.liquid {
        LiquidType::None => true,
        LiquidType::Water => true,
        LiquidType::ShallowWater => true,
        LiquidType::Lava => false,
        LiquidType::Chasm => false,
    };

    terrain_walkable && liquid_walkable
}

pub fn is_passable(tile: Tile) -> bool {
    // Topologically passable: anywhere an entity *could* go, or doors.
    // Used for connectivity checks (ChokeMap, flood-fill). HiddenDoor and
    // LockedDoor are passable so connectivity checkers don't reject maps
    // where these are the only connection between regions.
    match tile.terrain {
        TerrainType::Wall => false,
        TerrainType::Empty => false,
        _ => true, // Doors, floors, stairs, HiddenDoor, LockedDoor all passable
    }
}

/// Brogue's T_LAKE_PATHING_BLOCKER / T_PATHING_BLOCKER concept.
/// These tiles are physically walkable but AI and level design should avoid them.
/// Deep water, lava, and chasm are pathing blockers.
pub fn is_pathing_blocker(tile: Tile) -> bool {
    match tile.liquid {
        LiquidType::Water => true,   // deep water — Brogue's T_IS_DEEP_WATER
        LiquidType::Lava => true,    // instant death
        LiquidType::Chasm => true,   // impassable void
        _ => false,
    }
}

pub fn is_opaque(tile: Tile) -> bool {
    match tile.terrain {
        TerrainType::Wall => true,
        TerrainType::Door => true,
        TerrainType::HiddenDoor => true,  // Blocks FOV like a wall
        TerrainType::LockedDoor => true,  // Blocks FOV like a closed door
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
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
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
            translation: Vec3::new(
                pt.x as f32 * GRID_SIZE.x,
                pt.y as f32 * GRID_SIZE.y,
                // Stairs render above liquid overlays (z=0.1) so they stay visible
                if matches!(tile.terrain, TerrainType::DownStairs | TerrainType::UpStairs) { 0.2 } else { 0.0 },
            ),
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
                crate::game::ascii_mode::LiquidOverlay,
            ))
            .id();

        commands.entity(tile_entity).add_child(l_child);
    }

    // NOTE: Decoration sprite overlays are not spawned yet — all decoration entries
    // in tiles.ron use floor_stone.png as placeholder. Once proper decoration sprites
    // exist, spawn a child entity here at z=0.05 with DecorationOverlay marker
    // (same pattern as the liquid overlay above).

    // --- ASCII mode children ---
    if let Some(font) = ascii_font {
        // Children inherit parent's scale transform. Compensate so they render
        // at the correct pixel size regardless of the atlas tile's native size.
        let inv_scale = Vec3::new(1.0 / scale_x, 1.0 / scale_y, 1.0);

        // Determine background color: liquid overrides terrain
        let bg_color = if tile.liquid != LiquidType::None {
            let liquid_asset = tile_manifest.tiles.get(tile.liquid.name());
            liquid_asset.map(|a| a.ascii_bg).unwrap_or(terrain_asset.ascii_bg)
        } else {
            terrain_asset.ascii_bg
        };

        // Determine character: decoration (dry only) > liquid > terrain
        let (ascii_char, fg_color) = if tile.decoration != Decoration::None && tile.liquid == LiquidType::None {
            let dec_asset = tile_manifest.tiles.get(tile.decoration.name());
            match dec_asset {
                Some(da) if !da.ascii_char.is_empty() => (da.ascii_char.clone(), da.ascii_fg),
                _ => (terrain_asset.ascii_char.clone(), terrain_asset.ascii_fg),
            }
        } else if tile.liquid != LiquidType::None {
            let liquid_asset = tile_manifest.tiles.get(tile.liquid.name());
            match liquid_asset {
                Some(la) if !la.ascii_char.is_empty() => (la.ascii_char.clone(), la.ascii_fg),
                _ => (terrain_asset.ascii_char.clone(), terrain_asset.ascii_fg),
            }
        } else {
            (terrain_asset.ascii_char.clone(), terrain_asset.ascii_fg)
        };

        let display_char = if ascii_char.is_empty() { "?".to_string() } else { ascii_char };

        // Background quad — sized to fill one grid cell after parent scale is applied
        let bg_child = commands
            .spawn((
                Sprite {
                    color: bg_color,
                    custom_size: Some(GRID_SIZE),
                    ..default()
                },
                Transform {
                    scale: inv_scale,
                    ..default()
                },
                Visibility::Hidden,
                crate::game::ascii_mode::AsciiBackground { base_color: bg_color },
                RenderLayers::layer(1),
            ))
            .id();
        commands.entity(tile_entity).add_child(bg_child);

        // Character glyph — also counter-scaled so font renders at correct size
        let glyph_child = commands
            .spawn((
                Text2d::new(display_char),
                TextFont {
                    font: font.0.clone(),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(fg_color),
                Transform {
                    translation: Vec3::new(0.0, 0.0, 0.05),
                    scale: inv_scale,
                    ..default()
                },
                Visibility::Hidden,
                crate::game::ascii_mode::AsciiGlyph,
                crate::game::ascii_mode::AsciiGlyphColor(fg_color),
                RenderLayers::layer(1),
            ))
            .id();
        commands.entity(tile_entity).add_child(glyph_child);
    }

    tile_entity
}
