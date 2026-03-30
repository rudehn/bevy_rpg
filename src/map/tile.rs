use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::component::Component;
use bevy::prelude::{
    Assets, Children, Commands, Entity, InheritedVisibility, Message, MessageReader, Res, ResMut,
    Resource, Sprite, TextureAtlas, Transform, Vec3, ViewVisibility, Visibility, Query, With,
    Text2d, TextFont, TextColor, default, warn,
};
use bracket_lib::prelude::Point;
use serde::{Deserialize, Serialize};

use crate::assets::{self, TileManifest, TileManifestHandle, TileSpriteAssets};
use crate::components::{Collider, MovementMode, Viewshed};
use crate::map::map::GRID_SIZE;
use crate::map::Map;

/// Spatial index mapping grid `(x, y)` → tile `Entity`.
/// Built once per floor load by `spawn_tiles_into_ecs`; never modified at runtime.
#[derive(Resource, Default)]
pub struct TileEntityIndex(pub HashMap<(i32, i32), Entity>);

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

/// Mode-aware walkability check. Accounts for the entity's MovementMode when
/// deciding whether it can enter a tile.
pub fn can_entity_enter_tile(tile: Tile, mode: MovementMode) -> bool {
    match mode {
        MovementMode::Land | MovementMode::ImmuneToWater => is_walkable(tile),
        MovementMode::RestrictedToLiquid => {
            // Must have liquid AND terrain must be walkable
            tile.liquid != LiquidType::None && is_walkable(tile)
        }
    }
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
    let terrain_opaque = matches!(tile.terrain,
        TerrainType::Wall | TerrainType::Door | TerrainType::HiddenDoor | TerrainType::LockedDoor
    );
    let decoration_opaque = matches!(tile.decoration,
        Decoration::TallGrass | Decoration::Fungus
    );
    terrain_opaque || decoration_opaque
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

        // Determine character: important terrain (stairs, doors) > decoration (dry only) > liquid > terrain
        let terrain_has_priority = matches!(tile.terrain,
            TerrainType::DownStairs | TerrainType::UpStairs | TerrainType::Door | TerrainType::OpenDoor | TerrainType::LockedDoor
        );
        let (ascii_char, fg_color) = if terrain_has_priority {
            (terrain_asset.ascii_char.clone(), terrain_asset.ascii_fg)
        } else if tile.decoration != Decoration::None && tile.liquid == LiquidType::None {
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

// ---------------------------------------------------------------------------
// Runtime tile mutation
// ---------------------------------------------------------------------------

/// Request to change a tile's terrain at runtime. Handled by `apply_tile_mutations`,
/// which updates both the `Map` resource and the ECS tile entity (sprite, collider,
/// ASCII glyph, viewshed dirty flags).
#[derive(Message)]
pub struct TileMutationMessage {
    pub position: Point,
    pub new_terrain: TerrainType,
}

/// Applies queued tile mutations, keeping Map resource and ECS tile entities in sync.
pub fn apply_tile_mutations(
    mut commands: Commands,
    mut messages: MessageReader<TileMutationMessage>,
    mut map: ResMut<Map>,
    tile_index: Res<TileEntityIndex>,
    mut tile_query: Query<(&mut TerrainType, &mut Sprite, Option<&Children>)>,
    mut glyph_query: Query<&mut Text2d, With<crate::game::ascii_mode::AsciiGlyph>>,
    mut viewshed_query: Query<&mut Viewshed>,
    tile_manifests: Res<Assets<TileManifest>>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_sprite_assets: Res<TileSpriteAssets>,
) {
    let mut any = false;

    let tile_manifest = tile_manifests.get(&tile_manifest_handle.0);

    for msg in messages.read() {
        // 1. Update Map resource (source of truth for game logic).
        let idx = map.xy_idx(msg.position.x, msg.position.y);
        map.tiles[idx].terrain = msg.new_terrain;

        // 2. Look up the ECS tile entity via spatial index.
        let Some(&tile_entity) = tile_index.0.get(&(msg.position.x, msg.position.y)) else {
            warn!(
                "TileMutationMessage at ({}, {}) — no tile entity in index",
                msg.position.x, msg.position.y
            );
            continue;
        };

        let Ok((mut terrain_type, mut sprite, children)) = tile_query.get_mut(tile_entity) else {
            warn!(
                "TileMutationMessage at ({}, {}) — tile entity {:?} missing components",
                msg.position.x, msg.position.y, tile_entity
            );
            continue;
        };

        // 3. Update ECS terrain component.
        *terrain_type = msg.new_terrain;

        // 4. Update sprite from tile manifest.
        if let Some(manifest) = tile_manifest
            && let Some(asset) = manifest.tiles.get(msg.new_terrain.name()) {
                let (texture_path, index) = assets::parse_sprite_path(&asset.sprite);

                if let Some(texture_handle) = tile_sprite_assets.handles.get(texture_path) {
                    sprite.image = texture_handle.clone();
                }
                if let Some(layout_handle) = tile_sprite_assets.layouts.get(texture_path)
                    && let Some(ref mut texture_atlas) = sprite.texture_atlas {
                        texture_atlas.index = index;
                        texture_atlas.layout = layout_handle.clone();
                    }

                // 5. Update ASCII glyph child.
                if let Some(children) = children {
                    let new_char = if asset.ascii_char.is_empty() {
                        msg.new_terrain.name()
                    } else {
                        &asset.ascii_char
                    };
                    for &child in children.iter() {
                        if let Ok(mut text) = glyph_query.get_mut(child) {
                            **text = new_char.to_string();
                        }
                    }
                }
            }

        // 6. Add or remove Collider based on walkability of the full tile.
        let full_tile = map.tiles[idx];
        if is_walkable(full_tile) {
            commands.entity(tile_entity).remove::<Collider>();
        } else {
            commands.entity(tile_entity).insert(Collider);
        }

        any = true;
    }

    // 7. Mark all viewsheds dirty so FOV is recalculated.
    if any {
        for mut viewshed in viewshed_query.iter_mut() {
            viewshed.dirty = true;
        }
    }
}
