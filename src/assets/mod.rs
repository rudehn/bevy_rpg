use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    constants::{TILE_MAP_PATH, TILE_SIZE_X, TILE_SIZE_Y},
    game::{AppState, camera},
};

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DungeonTileset>()
            .init_resource::<CandleSpritesheet>()
            .init_resource::<MonsterManifestHandle>() // Initialize the resource
            .add_systems(
                OnEnter(AppState::Loading),
                (
                    setup_dungeon_tileset,
                    setup_candle_spritesheet,
                    load_monster_manifest,
                ),
            );
    }
}

pub struct LoadingPlugin;
impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AssetsPlugin,
            RonAssetPlugin::<MonsterManifest>::new(&["monsters.ron"]),
        ))
        .add_systems(Startup, (camera::setup_camera, set_clear_color))
        .add_systems(
            Update,
            check_assets_loaded.run_if(in_state(AppState::Loading)),
        )
        // .add_systems(OnEnter(AppState::Loading), spawn_monsters_from_manifest)
        .init_resource::<MonsterSpriteAssets>();
    }
}

#[derive(Resource, Default)]
pub struct MonsterSpriteAssets {
    pub handles: Vec<Handle<Image>>,
    pub layouts: Vec<Handle<TextureAtlasLayout>>,
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
}

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct MonsterManifest {
    pub monsters: HashMap<String, MonsterAsset>,
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

fn load_monster_manifest(
    asset_server: Res<AssetServer>,
    mut monster_manifest_handle: ResMut<MonsterManifestHandle>,
) {
    monster_manifest_handle.0 = asset_server.load("monsters.ron");
}

fn check_assets_loaded(
    asset_server: Res<AssetServer>,
    dungeon_tileset: Res<DungeonTileset>,
    candle_spritesheet: Res<CandleSpritesheet>,
    monster_manifest_handle: Res<MonsterManifestHandle>,
    monster_manifests: Res<Assets<MonsterManifest>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let all_textures_loaded = asset_server.is_loaded_with_dependencies(&dungeon_tileset.texture)
        && asset_server.is_loaded_with_dependencies(&candle_spritesheet.texture)
        && monster_manifests.get(&monster_manifest_handle.0).is_some();

    if all_textures_loaded {
        next_state.set(AppState::Menu);
    }
}

fn set_clear_color(mut clear_color: ResMut<ClearColor>) {
    clear_color.0 = Color::srgb_u8(37, 19, 26);
}
