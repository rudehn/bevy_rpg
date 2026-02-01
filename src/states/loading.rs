use bevy::prelude::*;
use bevy_light_2d::light::{AmbientLight2d, Light2d};

use crate::{
    assets_plugin::{AssetsPlugin, DungeonTileset},
    states::AppState,
};

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
    mut next_state: ResMut<NextState<AppState>>,
) {
    let all_assets_loaded = asset_server.is_loaded_with_dependencies(&dungeon_tileset.texture)
        && asset_server.is_loaded_with_dependencies(&dungeon_tileset.layout);

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
