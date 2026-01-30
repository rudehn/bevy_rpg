use bevy::prelude::*;
use bevy_light_2d::prelude::*;
use collision::collision::CollisionPlugin;
use map::{light::LightPlugin, map::MapPlugin};
use player::player::PlayerPlugin;

mod collision;
mod constants;
mod map;
mod player;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            Light2dPlugin,
            MapPlugin,
            LightPlugin,
            PlayerPlugin,
            CollisionPlugin,
        ))
        .init_resource::<DungeonTileset>()
        .add_systems(Startup, (setup_camera, set_clear_color))
        .add_systems(Startup, setup_dungeon_tileset)
        .run();
}

#[derive(Resource, Default)]
pub struct DungeonTileset {
    pub layout: Handle<TextureAtlasLayout>,
    pub texture: Handle<Image>,
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

fn setup_dungeon_tileset(
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut dungeon_tileset: ResMut<DungeonTileset>,
) {
    dungeon_tileset.texture = asset_server.load("tilemap_packed.png");
    dungeon_tileset.layout = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(16, 16),
        12,
        11,
        None,
        None,
    ));
}