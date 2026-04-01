use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::component::Component;
use bevy::prelude::{
    Assets, Children, Commands, Entity, InheritedVisibility, Message, MessageReader, Query, Res,
    ResMut, Resource, Sprite, Text2d, TextColor, TextFont, TextureAtlas, Transform, Vec3,
    ViewVisibility, Visibility, With, default, warn,
};
use bracket_lib::prelude::Point;
use serde::{Deserialize, Serialize};

use crate::assets::{self, TileManifest, TileManifestHandle, TileSpriteAssets};
use crate::components::{Collider, MovementMode, Viewshed};
use crate::map::Map;
use crate::map::map::GRID_SIZE;

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
    /// Escape portal on the final floor. Walkable, non-opaque.
    Portal,
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
    /// Naturally placed dead vegetation. Does NOT regrow.
    DeadGrass,
    Rubble,
    Moss,
    Fungus,
    Cobweb,
    Bloodstain,
    /// TallGrass that was trampled. Regrows into TallGrass over time.
    TrampledGrass,
    /// Fungus that was trampled. Regrows into Fungus over time.
    TrampledFungus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: TerrainType,
    pub liquid: LiquidType,
    pub decoration: Decoration,
}

// ---------------------------------------------------------------------------
// Tile Promotion System (Brogue-aligned)
// ---------------------------------------------------------------------------

/// What a tile promotes into. Can target either the decoration or terrain layer.
#[derive(Debug, Clone, Copy)]
pub enum PromotionTarget {
    Decoration(Decoration),
    Terrain(TerrainType),
}

/// A timed promotion rule: what a tile becomes and at what rate.
#[derive(Debug, Clone, Copy)]
pub struct PromotionRule {
    pub target: PromotionTarget,
    /// Chance per turn out of 10000 (Brogue scale). 10000 = 100%, 100 = 1%.
    pub chance_per_turn: u16,
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
            TerrainType::Portal => "Portal",
        }
    }

    /// What this terrain becomes when stepped on. None = no step promotion.
    pub fn on_step_promotion(&self) -> Option<PromotionTarget> {
        None
    }

    /// Timed promotion rule. None = no passive change.
    pub fn timed_promotion(&self) -> Option<PromotionRule> {
        match self {
            // Open doors close automatically next turn (Brogue: 10000/10000 = 100%).
            // PromotionCooldown prevents closing on the same turn the door was opened.
            TerrainType::OpenDoor => Some(PromotionRule {
                target: PromotionTarget::Terrain(TerrainType::Door),
                chance_per_turn: 10000,
            }),
            _ => None,
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
            Decoration::TrampledGrass => "TrumpledGrass",
            Decoration::TrampledFungus => "TrumpledFungus",
        }
    }

    /// Movement cost multiplier. Applied to both player movement and AI pathing.
    /// Values > 1.0 slow movement through this decoration.
    pub fn movement_cost(&self) -> f32 {
        match self {
            _ => 1.0,
        }
    }

    /// Whether this decoration blocks line of sight.
    pub fn blocks_fov(&self) -> bool {
        matches!(self, Decoration::TallGrass | Decoration::Fungus)
    }

    /// What this decoration becomes when stepped on. None = no step promotion.
    /// Cobwebs are handled separately via the entangle mechanic (not a promotion).
    pub fn on_step_promotion(&self) -> Option<PromotionTarget> {
        match self {
            Decoration::TallGrass => Some(PromotionTarget::Decoration(Decoration::TrampledGrass)),
            Decoration::Fungus => Some(PromotionTarget::Decoration(Decoration::TrampledFungus)),
            _ => None,
        }
    }

    /// Timed promotion rule. None = no passive change.
    /// Uses Brogue's 0-10000 scale (100 = ~1% per turn).
    pub fn timed_promotion(&self) -> Option<PromotionRule> {
        match self {
            Decoration::TrampledGrass => Some(PromotionRule {
                target: PromotionTarget::Decoration(Decoration::TallGrass),
                chance_per_turn: 100,
            }),
            Decoration::TrampledFungus => Some(PromotionRule {
                target: PromotionTarget::Decoration(Decoration::Fungus),
                chance_per_turn: 100,
            }),
            _ => None,
        }
    }

    /// Whether stepping on this decoration entangles the creature.
    pub fn entangles(&self) -> bool {
        matches!(self, Decoration::Cobweb)
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
        TerrainType::HiddenDoor => false, // Not walkable until discovered → Door
        TerrainType::LockedDoor => false, // Not walkable until unlocked → Door
        TerrainType::Portal => true,
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
        LiquidType::Water => true, // deep water — Brogue's T_IS_DEEP_WATER
        LiquidType::Lava => true,  // instant death
        LiquidType::Chasm => true, // impassable void
        _ => false,
    }
}

pub fn is_opaque(tile: Tile) -> bool {
    let terrain_opaque = matches!(
        tile.terrain,
        TerrainType::Wall | TerrainType::Door | TerrainType::HiddenDoor | TerrainType::LockedDoor
    );
    terrain_opaque || tile.decoration.blocks_fov()
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
                if matches!(
                    tile.terrain,
                    TerrainType::DownStairs | TerrainType::UpStairs
                ) {
                    0.2
                } else {
                    0.0
                },
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
            liquid_asset
                .map(|a| a.ascii_bg)
                .unwrap_or(terrain_asset.ascii_bg)
        } else {
            terrain_asset.ascii_bg
        };

        // Determine character: important terrain (stairs, doors) > decoration (dry only) > liquid > terrain
        let terrain_has_priority = matches!(
            tile.terrain,
            TerrainType::DownStairs
                | TerrainType::UpStairs
                | TerrainType::Door
                | TerrainType::OpenDoor
                | TerrainType::LockedDoor
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

        let display_char = if ascii_char.is_empty() {
            "?".to_string()
        } else {
            ascii_char
        };

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
                crate::game::ascii_mode::AsciiBackground {
                    base_color: bg_color,
                },
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
    mut promotion_cooldown: ResMut<crate::game::tile_promotion::PromotionCooldown>,
) {
    let mut any = false;

    let tile_manifest = tile_manifests.get(&tile_manifest_handle.0);

    for msg in messages.read() {
        // 1. Update Map resource (source of truth for game logic).
        let idx = map.xy_idx(msg.position.x, msg.position.y);
        map.tiles[idx].terrain = msg.new_terrain;

        // Mark this tile on cooldown so the promotion tick doesn't revert it same-turn.
        promotion_cooldown
            .0
            .insert((msg.position.x, msg.position.y));

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
            && let Some(asset) = manifest.tiles.get(msg.new_terrain.name())
        {
            let (texture_path, index) = assets::parse_sprite_path(&asset.sprite);

            if let Some(texture_handle) = tile_sprite_assets.handles.get(texture_path) {
                sprite.image = texture_handle.clone();
            }
            if let Some(layout_handle) = tile_sprite_assets.layouts.get(texture_path)
                && let Some(ref mut texture_atlas) = sprite.texture_atlas
            {
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

/// Request to change a tile's decoration at runtime. Handled by
/// `apply_decoration_mutations`, which updates the Map resource and the ECS
/// tile entity's ASCII glyph and color.
#[derive(Message)]
pub struct DecorationMutationMessage {
    pub position: Point,
    pub new_decoration: Decoration,
}

/// Applies queued decoration mutations, keeping Map resource and ECS tile entities in sync.
pub fn apply_decoration_mutations(
    mut messages: MessageReader<DecorationMutationMessage>,
    mut map: ResMut<Map>,
    tile_index: Res<TileEntityIndex>,
    tile_query: Query<Option<&Children>, With<TileMarker>>,
    mut glyph_query: Query<
        (&mut Text2d, &mut TextColor),
        With<crate::game::ascii_mode::AsciiGlyph>,
    >,
    mut color_query: Query<&mut crate::game::ascii_mode::AsciiGlyphColor>,
    mut viewshed_query: Query<&mut Viewshed>,
    tile_manifests: Res<Assets<TileManifest>>,
    tile_manifest_handle: Res<TileManifestHandle>,
) {
    let mut fov_changed = false;

    let tile_manifest = tile_manifests.get(&tile_manifest_handle.0);

    for msg in messages.read() {
        let idx = map.xy_idx(msg.position.x, msg.position.y);
        let old_decoration = map.tiles[idx].decoration;
        map.tiles[idx].decoration = msg.new_decoration;

        // Check if FOV changed (decoration blocking/unblocking vision).
        if old_decoration.blocks_fov() != msg.new_decoration.blocks_fov() {
            fov_changed = true;
        }

        // Update ASCII glyph on the tile entity.
        let Some(&tile_entity) = tile_index.0.get(&(msg.position.x, msg.position.y)) else {
            continue;
        };

        let Ok(children) = tile_query.get(tile_entity) else {
            continue;
        };

        // Determine what char/color the tile should display now.
        // Priority: important terrain > decoration (dry only) > liquid > terrain
        // (mirrors the logic in spawn_tile_entity)
        let tile = map.tiles[idx];
        let terrain_has_priority = matches!(
            tile.terrain,
            TerrainType::DownStairs
                | TerrainType::UpStairs
                | TerrainType::Door
                | TerrainType::OpenDoor
                | TerrainType::LockedDoor
        );

        if let Some(manifest) = tile_manifest
            && !terrain_has_priority
        {
            // Pick the display source: decoration if present and dry, else terrain
            let (display_name, is_decoration) = if tile.decoration != Decoration::None
                && tile.liquid == crate::map::tile::LiquidType::None
            {
                (tile.decoration.name(), true)
            } else {
                (tile.terrain.name(), false)
            };

            if let Some(asset) = manifest.tiles.get(display_name) {
                let new_char = if asset.ascii_char.is_empty() {
                    display_name.to_string()
                } else {
                    asset.ascii_char.clone()
                };
                let new_color = if is_decoration {
                    asset.ascii_fg
                } else {
                    asset.ascii_fg
                };

                if let Some(children) = children {
                    for &child in children.iter() {
                        if let Ok((mut text, mut text_color)) = glyph_query.get_mut(child) {
                            **text = new_char.clone();
                            text_color.0 = new_color;
                        }
                        if let Ok(mut glyph_color) = color_query.get_mut(child) {
                            glyph_color.0 = new_color;
                        }
                    }
                }
            }
        }
    }

    if fov_changed {
        for mut viewshed in viewshed_query.iter_mut() {
            viewshed.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::MovementMode;

    fn tile(terrain: TerrainType, liquid: LiquidType) -> Tile {
        Tile {
            terrain,
            liquid,
            decoration: Decoration::None,
        }
    }

    // ---- is_walkable ----

    #[test]
    fn floor_no_liquid_is_walkable() {
        assert!(is_walkable(tile(TerrainType::Floor, LiquidType::None)));
    }

    #[test]
    fn wall_is_not_walkable() {
        assert!(!is_walkable(tile(TerrainType::Wall, LiquidType::None)));
    }

    #[test]
    fn floor_with_deep_water_is_walkable() {
        assert!(is_walkable(tile(TerrainType::Floor, LiquidType::Water)));
    }

    #[test]
    fn floor_with_shallow_water_is_walkable() {
        assert!(is_walkable(tile(
            TerrainType::Floor,
            LiquidType::ShallowWater
        )));
    }

    #[test]
    fn floor_with_lava_is_not_walkable() {
        assert!(!is_walkable(tile(TerrainType::Floor, LiquidType::Lava)));
    }

    #[test]
    fn floor_with_chasm_is_not_walkable() {
        assert!(!is_walkable(tile(TerrainType::Floor, LiquidType::Chasm)));
    }

    #[test]
    fn door_is_not_walkable() {
        assert!(!is_walkable(tile(TerrainType::Door, LiquidType::None)));
    }

    #[test]
    fn open_door_is_walkable() {
        assert!(is_walkable(tile(TerrainType::OpenDoor, LiquidType::None)));
    }

    #[test]
    fn stairs_are_walkable() {
        assert!(is_walkable(tile(TerrainType::DownStairs, LiquidType::None)));
        assert!(is_walkable(tile(TerrainType::UpStairs, LiquidType::None)));
    }

    // ---- can_entity_enter_tile ----

    #[test]
    fn land_mode_enters_floor() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::None),
            MovementMode::Land,
        ));
    }

    #[test]
    fn land_mode_enters_deep_water() {
        // Land entities CAN enter deep water (they just get penalized)
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Water),
            MovementMode::Land,
        ));
    }

    #[test]
    fn land_mode_blocked_by_wall() {
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Wall, LiquidType::None),
            MovementMode::Land,
        ));
    }

    #[test]
    fn land_mode_blocked_by_lava() {
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Lava),
            MovementMode::Land,
        ));
    }

    #[test]
    fn immune_to_water_enters_deep_water() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Water),
            MovementMode::ImmuneToWater,
        ));
    }

    #[test]
    fn immune_to_water_enters_floor() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::None),
            MovementMode::ImmuneToWater,
        ));
    }

    #[test]
    fn restricted_to_liquid_enters_deep_water() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Water),
            MovementMode::RestrictedToLiquid,
        ));
    }

    #[test]
    fn restricted_to_liquid_enters_shallow_water() {
        assert!(can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::ShallowWater),
            MovementMode::RestrictedToLiquid,
        ));
    }

    #[test]
    fn restricted_to_liquid_blocked_by_dry_floor() {
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::None),
            MovementMode::RestrictedToLiquid,
        ));
    }

    #[test]
    fn restricted_to_liquid_blocked_by_wall_with_water() {
        // Wall + Water: terrain not walkable even though liquid is present
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Wall, LiquidType::Water),
            MovementMode::RestrictedToLiquid,
        ));
    }

    #[test]
    fn restricted_to_liquid_blocked_by_lava() {
        // Lava has liquid but is_walkable returns false for lava
        assert!(!can_entity_enter_tile(
            tile(TerrainType::Floor, LiquidType::Lava),
            MovementMode::RestrictedToLiquid,
        ));
    }

    // ---- is_pathing_blocker ----

    #[test]
    fn deep_water_is_pathing_blocker() {
        assert!(is_pathing_blocker(tile(
            TerrainType::Floor,
            LiquidType::Water
        )));
    }

    #[test]
    fn lava_is_pathing_blocker() {
        assert!(is_pathing_blocker(tile(
            TerrainType::Floor,
            LiquidType::Lava
        )));
    }

    #[test]
    fn chasm_is_pathing_blocker() {
        assert!(is_pathing_blocker(tile(
            TerrainType::Floor,
            LiquidType::Chasm
        )));
    }

    #[test]
    fn shallow_water_is_not_pathing_blocker() {
        assert!(!is_pathing_blocker(tile(
            TerrainType::Floor,
            LiquidType::ShallowWater
        )));
    }

    #[test]
    fn dry_floor_is_not_pathing_blocker() {
        assert!(!is_pathing_blocker(tile(
            TerrainType::Floor,
            LiquidType::None
        )));
    }

    // ---- is_opaque ----

    #[test]
    fn wall_is_opaque() {
        assert!(is_opaque(tile(TerrainType::Wall, LiquidType::None)));
    }

    #[test]
    fn floor_is_not_opaque() {
        assert!(!is_opaque(tile(TerrainType::Floor, LiquidType::None)));
    }

    #[test]
    fn deep_water_is_not_opaque() {
        assert!(!is_opaque(tile(TerrainType::Floor, LiquidType::Water)));
    }

    #[test]
    fn closed_door_is_opaque() {
        assert!(is_opaque(tile(TerrainType::Door, LiquidType::None)));
    }
}
