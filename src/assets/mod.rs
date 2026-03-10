use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::Deserialize;
use std::collections::HashMap;

use crate::game::items::{ArmorSlot, ItemKind, Rarity};

use crate::{
    constants::{TILE_SIZE_X, TILE_SIZE_Y},
    game::{AppState, camera},
};

mod serde_helpers {
    use bevy::prelude::UVec2;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize_f32_as_option<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Some(f32::deserialize(deserializer)?))
    }

    pub fn deserialize_i32_as_option<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Some(i32::deserialize(deserializer)?))
    }

    pub fn deserialize_uvec2_as_option<'de, D>(deserializer: D) -> Result<Option<UVec2>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Some(UVec2::deserialize(deserializer)?))
    }
}

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CandleSpritesheet>()
            .init_resource::<MonsterManifestHandle>()
            .init_resource::<MonsterSpawnTableHandle>()
            .init_resource::<TileManifestHandle>()
            .init_resource::<ItemManifestHandle>()
            .init_resource::<ItemSpawnTableHandle>()
            .init_resource::<PlayerAssetHandle>()
            .add_systems(
                OnEnter(AppState::Loading),
                (
                    setup_candle_spritesheet,
                    load_monster_manifest,
                    load_monster_spawn_table,
                    load_tile_manifest,
                    load_item_manifest,
                    load_item_spawn_table,
                    load_player_asset,
                ),
            )
            .add_systems(
                Update,
                (
                    load_monster_sprites,
                    load_tile_sprites,
                    load_item_sprites,
                    check_assets_loaded,
                )
                    .run_if(in_state(AppState::Loading)),
            );
    }
}

pub struct LoadingPlugin;
impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AssetsPlugin,
            RonAssetPlugin::<MonsterManifest>::new(&["monsters.ron"]),
            RonAssetPlugin::<MonsterSpawnTable>::new(&["monster_spawns.ron"]),
            RonAssetPlugin::<TileManifest>::new(&["tiles.ron"]),
            RonAssetPlugin::<ItemManifest>::new(&["items.ron"]),
            RonAssetPlugin::<ItemSpawnTable>::new(&["item_spawns.ron"]),
            RonAssetPlugin::<PlayerAsset>::new(&["player.ron"]),
        ))
        .add_systems(Startup, (camera::setup_camera, set_clear_color))
        .init_resource::<MonsterSpriteAssets>()
        .init_resource::<TileSpriteAssets>()
        .init_resource::<ItemSpriteAssets>();
    }
}

#[derive(Resource, Default)]
pub struct MonsterSpriteAssets {
    pub handles: HashMap<String, Handle<Image>>,
    pub layouts: HashMap<String, Handle<TextureAtlasLayout>>,
}

#[derive(Resource, Default)]
pub struct TileSpriteAssets {
    pub handles: HashMap<String, Handle<Image>>,
    pub layouts: HashMap<String, Handle<TextureAtlasLayout>>,
}

#[derive(Resource, Default)]
pub struct ItemSpriteAssets {
    pub handles: HashMap<String, Handle<Image>>,
    pub layouts: HashMap<String, Handle<TextureAtlasLayout>>,
}

#[derive(Resource, Default)]
pub struct CandleSpritesheet {
    pub layout: Handle<TextureAtlasLayout>,
    pub texture: Handle<Image>,
}

#[derive(Asset, TypePath, Deserialize, Resource, Debug, Clone)]
pub struct PlayerAsset {
    pub name: String,
    pub perception: i32,
    pub sprite: String,
    pub level: i32,
    pub base_hp: i32,
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub agility: i32,
    pub intelligence: i32,
    pub damage: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MonsterLootEntry {
    pub item: String,
    pub spawn_chance: f32,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterAsset {
    pub name: String,
    pub perception: i32,
    pub sprite: String,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub grid_size: Option<UVec2>,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub tile_size: Option<UVec2>,

    pub level: i32,
    pub base_hp: i32,
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub agility: i32,
    pub damage: String,

    #[serde(default, deserialize_with = "serde_helpers::deserialize_i32_as_option")]
    pub regen: Option<i32>,

    #[serde(default)]
    pub loot_table: Vec<MonsterLootEntry>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterManifest {
    pub monsters: HashMap<String, MonsterAsset>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterSpawnInfo {
    pub monster: String,
    pub min_floor: i32,
    pub max_floor: i32,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterSpawnTable {
    pub spawns: Vec<MonsterSpawnInfo>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct ItemSpawnInfo {
    pub item: String,
    pub min_floor: i32,
    pub max_floor: i32,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_spawn_chance")]
    pub spawn_chance: f32,
}

fn default_weight() -> i32 { 1 }
fn default_spawn_chance() -> f32 { 0.75 }

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct ItemSpawnTable {
    pub spawns: Vec<ItemSpawnInfo>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct TileAsset {
    pub sprite: String,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub grid_size: Option<UVec2>,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub tile_size: Option<UVec2>,
}

#[derive(Asset, TypePath, Deserialize, Resource, Debug, Clone)]
pub struct TileManifest {
    pub tiles: HashMap<String, TileAsset>,
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct ItemAsset {
    pub name: String,
    pub sprite: String,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub grid_size: Option<UVec2>,
    #[serde(
        default,
        deserialize_with = "serde_helpers::deserialize_uvec2_as_option"
    )]
    pub tile_size: Option<UVec2>,
    pub is_victory: bool,

    #[serde(default)]
    pub item_kind: ItemKind,
    #[serde(default)]
    pub armor_slot: Option<ArmorSlot>,
    #[serde(default)]
    pub damage: Option<String>,
    #[serde(default)]
    pub defense: i32,
    #[serde(default)]
    pub rarity: Rarity,
    #[serde(default)]
    pub str_bonus: i32,
    #[serde(default)]
    pub dex_bonus: i32,
    #[serde(default)]
    pub con_bonus: i32,
    #[serde(default)]
    pub agi_bonus: i32,
    #[serde(default)]
    pub int_bonus: i32,
    #[serde(default)]
    pub per_bonus: i32,
}

#[derive(Asset, TypePath, Deserialize, Resource, Debug, Clone)]
pub struct ItemManifest {
    pub items: HashMap<String, ItemAsset>,
}

fn setup_candle_spritesheet(
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut candle_spritesheet: ResMut<CandleSpritesheet>,
) {
    candle_spritesheet.texture = asset_server.load("candle.png");
    candle_spritesheet.layout = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(TILE_SIZE_X, TILE_SIZE_Y),
        4,
        1,
        None,
        None,
    ));
}

#[derive(Resource, Default)]
pub struct MonsterManifestHandle(pub Handle<MonsterManifest>);

#[derive(Resource, Default)]
pub struct MonsterSpawnTableHandle(pub Handle<MonsterSpawnTable>);

#[derive(Resource, Default)]
pub struct TileManifestHandle(pub Handle<TileManifest>);

#[derive(Resource, Default)]
pub struct ItemManifestHandle(pub Handle<ItemManifest>);

#[derive(Resource, Default)]
pub struct ItemSpawnTableHandle(pub Handle<ItemSpawnTable>);

#[derive(Resource, Default)]
pub struct PlayerAssetHandle(pub Handle<PlayerAsset>);

fn load_monster_manifest(
    asset_server: Res<AssetServer>,
    mut monster_manifest_handle: ResMut<MonsterManifestHandle>,
) {
    monster_manifest_handle.0 = asset_server.load("monsters.ron");
}

fn load_monster_spawn_table(
    asset_server: Res<AssetServer>,
    mut handle: ResMut<MonsterSpawnTableHandle>,
) {
    handle.0 = asset_server.load("monster_spawns.ron");
}

fn load_tile_manifest(asset_server: Res<AssetServer>, mut handle: ResMut<TileManifestHandle>) {
    handle.0 = asset_server.load("tiles.ron");
}

fn load_item_manifest(asset_server: Res<AssetServer>, mut handle: ResMut<ItemManifestHandle>) {
    handle.0 = asset_server.load("items.ron");
}

fn load_item_spawn_table(asset_server: Res<AssetServer>, mut handle: ResMut<ItemSpawnTableHandle>) {
    handle.0 = asset_server.load("item_spawns.ron");
}

fn load_player_asset(asset_server: Res<AssetServer>, mut handle: ResMut<PlayerAssetHandle>) {
    handle.0 = asset_server.load("player.ron");
}

fn load_monster_sprites(
    asset_server: Res<AssetServer>,
    monster_manifest_handle: Res<MonsterManifestHandle>,
    monster_manifests: Res<Assets<MonsterManifest>>,
    mut monster_sprite_assets: ResMut<MonsterSpriteAssets>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    if monster_sprite_assets.handles.is_empty() {
        if let Some(manifest) = monster_manifests.get(&monster_manifest_handle.0) {
            for monster_asset in manifest.monsters.values() {
                let sprite_path_parts: Vec<&str> = monster_asset.sprite.split('#').collect();
                let texture_path = sprite_path_parts[0];
                let texture_path_string = texture_path.to_string();

                if !monster_sprite_assets
                    .handles
                    .contains_key(&texture_path_string)
                {
                    let texture_handle = asset_server.load::<Image>(texture_path_string.clone());
                    monster_sprite_assets
                        .handles
                        .insert(texture_path_string.clone(), texture_handle);

                    let tile_size = monster_asset.tile_size.unwrap_or(UVec2::new(32, 32));
                    let grid_size = monster_asset.grid_size.unwrap_or(UVec2::new(1, 1));

                    let layout_handle = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
                        tile_size,
                        grid_size.x,
                        grid_size.y,
                        None,
                        None,
                    ));
                    monster_sprite_assets
                        .layouts
                        .insert(texture_path_string, layout_handle);
                }
            }
        }
    }
}

fn load_tile_sprites(
    asset_server: Res<AssetServer>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_manifests: Res<Assets<TileManifest>>,
    player_asset_handle: Res<PlayerAssetHandle>,
    player_assets: Res<Assets<PlayerAsset>>,
    mut tile_sprite_assets: ResMut<TileSpriteAssets>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    if tile_sprite_assets.handles.is_empty() {
        if let Some(manifest) = tile_manifests.get(&tile_manifest_handle.0) {
            for tile_asset in manifest.tiles.values() {
                let sprite_path_parts: Vec<&str> = tile_asset.sprite.split('#').collect();
                let texture_path = sprite_path_parts[0];
                let texture_path_string = texture_path.to_string();

                if !tile_sprite_assets
                    .handles
                    .contains_key(&texture_path_string)
                {
                    let texture_handle = asset_server.load::<Image>(texture_path_string.clone());
                    tile_sprite_assets
                        .handles
                        .insert(texture_path_string.clone(), texture_handle);

                    let tile_size = tile_asset.tile_size.unwrap_or(UVec2::new(16, 16));
                    let grid_size = tile_asset.grid_size.unwrap_or(UVec2::new(1, 1));

                    let layout_handle = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
                        tile_size,
                        grid_size.x,
                        grid_size.y,
                        None,
                        None,
                    ));
                    tile_sprite_assets
                        .layouts
                        .insert(texture_path_string, layout_handle);
                }
            }
        }

        // Ensure player sprite is also loaded
        if let Some(player_asset) = player_assets.get(&player_asset_handle.0) {
            let sprite_path_parts: Vec<&str> = player_asset.sprite.split('#').collect();
            let texture_path = sprite_path_parts[0];
            let texture_path_string = texture_path.to_string();

            if !tile_sprite_assets
                .handles
                .contains_key(&texture_path_string)
            {
                let texture_handle = asset_server.load::<Image>(texture_path_string.clone());
                tile_sprite_assets
                    .handles
                    .insert(texture_path_string.clone(), texture_handle);

                // For individual player sprites like hero.png, assume 32x32 based on request
                let tile_size = UVec2::new(32, 32);
                let grid_size = UVec2::new(1, 1);

                let layout_handle = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
                    tile_size,
                    grid_size.x,
                    grid_size.y,
                    None,
                    None,
                ));
                tile_sprite_assets
                    .layouts
                    .insert(texture_path_string, layout_handle);
            }
        }
    }
}

fn load_item_sprites(
    asset_server: Res<AssetServer>,
    item_manifest_handle: Res<ItemManifestHandle>,
    item_manifests: Res<Assets<ItemManifest>>,
    mut item_sprite_assets: ResMut<ItemSpriteAssets>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    if item_sprite_assets.handles.is_empty() {
        if let Some(manifest) = item_manifests.get(&item_manifest_handle.0) {
            for item_asset in manifest.items.values() {
                let sprite_path_parts: Vec<&str> = item_asset.sprite.split('#').collect();
                let texture_path = sprite_path_parts[0];
                let texture_path_string = texture_path.to_string();

                if !item_sprite_assets
                    .handles
                    .contains_key(&texture_path_string)
                {
                    let texture_handle = asset_server.load::<Image>(texture_path_string.clone());
                    item_sprite_assets
                        .handles
                        .insert(texture_path_string.clone(), texture_handle);

                    let tile_size = item_asset.tile_size.unwrap_or(UVec2::new(32, 32));
                    let grid_size = item_asset.grid_size.unwrap_or(UVec2::new(1, 1));

                    let layout_handle = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
                        tile_size,
                        grid_size.x,
                        grid_size.y,
                        None,
                        None,
                    ));
                    item_sprite_assets
                        .layouts
                        .insert(texture_path_string, layout_handle);
                }
            }
        }
    }
}

// Groups the overflow resources so check_assets_loaded stays within Bevy's
// 16-SystemParam limit.
#[derive(bevy::ecs::system::SystemParam)]
struct ExtraLoadingParams<'w> {
    item_spawn_table_handle: Res<'w, ItemSpawnTableHandle>,
    item_spawn_tables: Res<'w, Assets<ItemSpawnTable>>,
    player_asset_handle: Res<'w, PlayerAssetHandle>,
    player_assets: Res<'w, Assets<PlayerAsset>>,
    next_state: ResMut<'w, NextState<AppState>>,
}

fn check_assets_loaded(
    asset_server: Res<AssetServer>,
    candle_spritesheet: Res<CandleSpritesheet>,
    monster_manifest_handle: Res<MonsterManifestHandle>,
    monster_manifests: Res<Assets<MonsterManifest>>,
    monster_spawn_table_handle: Res<MonsterSpawnTableHandle>,
    monster_spawn_tables: Res<Assets<MonsterSpawnTable>>,
    monster_sprite_assets: Res<MonsterSpriteAssets>,
    tile_manifest_handle: Res<TileManifestHandle>,
    tile_manifests: Res<Assets<TileManifest>>,
    tile_sprite_assets: Res<TileSpriteAssets>,
    item_manifest_handle: Res<ItemManifestHandle>,
    item_manifests: Res<Assets<ItemManifest>>,
    item_sprite_assets: Res<ItemSpriteAssets>,
    mut extra: ExtraLoadingParams,
) {
    let core_textures_loaded = asset_server.is_loaded_with_dependencies(&candle_spritesheet.texture);

    if !core_textures_loaded {
        return;
    }

    if monster_manifests.get(&monster_manifest_handle.0).is_none() {
        return;
    }

    if monster_spawn_tables
        .get(&monster_spawn_table_handle.0)
        .is_none()
    {
        return;
    }

    if tile_manifests.get(&tile_manifest_handle.0).is_none() {
        return;
    }

    if item_manifests.get(&item_manifest_handle.0).is_none() {
        return;
    }

    if extra.item_spawn_tables.get(&extra.item_spawn_table_handle.0).is_none() {
        return;
    }

    if extra.player_assets.get(&extra.player_asset_handle.0).is_none() {
        return;
    }

    if monster_sprite_assets.handles.is_empty()
        || tile_sprite_assets.handles.is_empty()
        || item_sprite_assets.handles.is_empty()
    {
        return;
    }

    for handle in monster_sprite_assets.handles.values() {
        if !asset_server.is_loaded_with_dependencies(handle) {
            return;
        }
    }

    for handle in tile_sprite_assets.handles.values() {
        if !asset_server.is_loaded_with_dependencies(handle) {
            return;
        }
    }

    for handle in item_sprite_assets.handles.values() {
        if !asset_server.is_loaded_with_dependencies(handle) {
            return;
        }
    }

    extra.next_state.set(AppState::Menu);
}

fn set_clear_color(mut clear_color: ResMut<ClearColor>) {
    clear_color.0 = Color::srgb_u8(37, 19, 26);
}
