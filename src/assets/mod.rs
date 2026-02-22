use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    constants::{TILE_MAP_PATH, TILE_SIZE_X, TILE_SIZE_Y},
    game::{AppState, camera},
};

mod serde_helpers {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize_f32_as_option<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Some(f32::deserialize(deserializer)?))
    }
}

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DungeonTileset>()
            .init_resource::<CandleSpritesheet>()
            .init_resource::<MonsterManifestHandle>()
            .init_resource::<MonsterSpawnTableHandle>()
            .add_systems(
                OnEnter(AppState::Loading),
                (
                    setup_dungeon_tileset,
                    setup_candle_spritesheet,
                    load_monster_manifest,
                    load_monster_spawn_table,
                ),
            )
            .add_systems(
                Update,
                (
                    load_monster_sprites,
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
        ))
        .add_systems(Startup, (camera::setup_camera, set_clear_color))
        // .add_systems(OnEnter(AppState::Loading), spawn_monsters_from_manifest)
        .init_resource::<MonsterSpriteAssets>();
    }
}

#[derive(Resource, Default)]
pub struct MonsterSpriteAssets {
    pub handles: HashMap<String, Handle<Image>>,
    pub layouts: HashMap<String, Handle<TextureAtlasLayout>>,
}
#[derive(Resource, Default)]
pub struct CandleSpritesheet {
    // Made public
    pub layout: Handle<TextureAtlasLayout>, // Made public
    pub texture: Handle<Image>,             // Made public
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterAsset {
    pub name: String,
    pub vision_range: f32,
    pub sprite: String,
    pub grid_size: UVec2, // New field
    pub tile_size: UVec2, // New field
    pub health: i32,      // New field for monster health
    pub damage: String,   // New field for monster damage
    #[serde(default, deserialize_with = "serde_helpers::deserialize_f32_as_option")]
    pub move_delay: Option<f32>,
    #[serde(default, deserialize_with = "serde_helpers::deserialize_f32_as_option")]
    pub action_delay: Option<f32>,
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

#[derive(Resource, Default)]
pub struct DungeonTileset {
    pub layout: Handle<TextureAtlasLayout>,
    pub texture: Handle<Image>,
}

fn setup_dungeon_tileset(
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut dungeon_tileset: ResMut<DungeonTileset>,
) {
    dungeon_tileset.texture = asset_server.load(TILE_MAP_PATH);
    dungeon_tileset.layout = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(16, 16),
        12,
        11,
        None,
        None,
    ));
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

fn load_monster_sprites(
    // mut commands: Commands, // Removed commands since we don't remove resource
    asset_server: Res<AssetServer>,
    monster_manifest_handle: Res<MonsterManifestHandle>,
    monster_manifests: Res<Assets<MonsterManifest>>,
    mut monster_sprite_assets: ResMut<MonsterSpriteAssets>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // Only run if the manifest is loaded AND monster_sprite_assets is currently empty (meaning it hasn't been populated yet)
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

                    let layout_handle = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
                        monster_asset.tile_size,   // Use tile_size from MonsterAsset
                        monster_asset.grid_size.x, // Use grid_size.x from MonsterAsset
                        monster_asset.grid_size.y, // Use grid_size.y from MonsterAsset
                        None,
                        None,
                    ));
                    monster_sprite_assets
                        .layouts
                        .insert(texture_path_string, layout_handle);
                }
            }
            // No commands.remove_resource::<MonsterManifestHandle>();
        }
    }
}

fn check_assets_loaded(
    asset_server: Res<AssetServer>,
    dungeon_tileset: Res<DungeonTileset>,
    candle_spritesheet: Res<CandleSpritesheet>,
    monster_manifest_handle: Res<MonsterManifestHandle>, // No longer Option
    monster_manifests: Res<Assets<MonsterManifest>>,
    monster_spawn_table_handle: Res<MonsterSpawnTableHandle>,
    monster_spawn_tables: Res<Assets<MonsterSpawnTable>>,
    monster_sprite_assets: Res<MonsterSpriteAssets>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    // 1. Check if core textures are loaded
    let core_textures_loaded = asset_server.is_loaded_with_dependencies(&dungeon_tileset.texture)
        && asset_server.is_loaded_with_dependencies(&candle_spritesheet.texture);

    if !core_textures_loaded {
        return; // Still waiting for core textures
    }

    // 2. Check if the MonsterManifest itself is loaded
    let monster_manifest_loaded = monster_manifests.get(&monster_manifest_handle.0).is_some();
    if !monster_manifest_loaded {
        return; // Still waiting for monster manifest to load
    }

    // 3. Check if the MonsterSpawnTable is loaded
    let spawn_table_loaded = monster_spawn_tables
        .get(&monster_spawn_table_handle.0)
        .is_some();
    if !spawn_table_loaded {
        return; // Still waiting for spawn table to load
    }

    // 4. Check if load_monster_sprites has populated monster_sprite_assets, and all contained sprites are loaded.
    // If monster_sprite_assets.handles is empty, it means load_monster_sprites hasn't run yet.
    if monster_sprite_assets.handles.is_empty() {
        return; // Still waiting for monster sprites to be populated/loaded
    }

    for handle in monster_sprite_assets.handles.values() {
        if !asset_server.is_loaded_with_dependencies(handle) {
            return; // Still waiting for some individual monster sprites
        }
    }

    // If we reach here, all assets are loaded.
    next_state.set(AppState::Menu);
}

fn set_clear_color(mut clear_color: ResMut<ClearColor>) {
    clear_color.0 = Color::srgb_u8(37, 19, 26);
}
