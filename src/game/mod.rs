use bevy::prelude::*;

use crate::{
    constants::{TILE_MAP_PATH, TILE_SIZE_X, TILE_SIZE_Y},
    game::systems::fov_update_system,
    map::{light::LightPlugin, map::MapPlugin},
    player::player::PlayerPlugin,
};

use bevy_light_2d::light::{AmbientLight2d, Light2d};

use crate::map::light::CandleSpritesheet;

mod systems;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Loading,
    InGame,
}

pub struct GamePlugin;
impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((LightPlugin, MapPlugin, PlayerPlugin))
            .add_systems(Update, fov_update_system);
    }
}

pub struct LoadingPlugin;
impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(AssetsPlugin)
            .add_systems(Startup, (setup_camera, set_clear_color))
            .add_systems(
                Update,
                check_assets_loaded.run_if(in_state(AppState::Loading)),
            );
    }
}

fn check_assets_loaded(
    asset_server: Res<AssetServer>,
    dungeon_tileset: Res<DungeonTileset>,
    candle_spritesheet: Res<CandleSpritesheet>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let all_assets_loaded = asset_server.is_loaded_with_dependencies(&dungeon_tileset.texture)
        && asset_server.is_loaded_with_dependencies(&dungeon_tileset.layout)
        && asset_server.is_loaded_with_dependencies(&candle_spritesheet.layout)
        && asset_server.is_loaded_with_dependencies(&candle_spritesheet.texture);

    if all_assets_loaded {
        next_state.set(AppState::InGame);
    }
}

fn setup_camera(mut commands: Commands) {
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = 0.25;
    commands.spawn((
        Camera2d,
        Projection::Orthographic(projection),
        Light2d {
            ambient_light: AmbientLight2d {
                brightness: 0.1,
                ..default()
            },
        },
    ));
}

fn set_clear_color(mut clear_color: ResMut<ClearColor>) {
    clear_color.0 = Color::srgb_u8(37, 19, 26);
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

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DungeonTileset>().add_systems(
            OnEnter(AppState::Loading),
            (setup_dungeon_tileset, setup_candle_spritesheet),
        );
    }
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
