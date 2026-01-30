use bevy::{color::palettes::css::YELLOW, prelude::*};
use bevy_light_2d::prelude::*;

use crate::constants::{ENTITY_INDEX, TILE_SIZE_X, TILE_SIZE_Y};

pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CandleSpritesheet>()
            .add_systems(Startup, (setup_candle_spritesheet, spawn_candles).chain())
            .add_systems(Update, animate_candles);
    }
}

#[derive(Resource, Default)]
struct CandleSpritesheet {
    layout: Handle<TextureAtlasLayout>,
    texture: Handle<Image>,
}

#[derive(Component)]
struct Candle;

#[derive(Component, Deref, DerefMut)]
struct AnimationTimer(Timer);

fn animate_candles(
    time: Res<Time>,
    mut query: Query<(&mut AnimationTimer, &mut Sprite), With<Candle>>,
) {
    for (mut timer, mut sprite) in &mut query {
        timer.tick(time.delta());
        if timer.just_finished()
            && let Some(ref mut texture_atlas) = sprite.texture_atlas
        {
            texture_atlas.index = (texture_atlas.index + 1) % 4;
        }
    }
}

fn spawn_candles(mut commands: Commands, spritesheet: Res<CandleSpritesheet>) {
    let light = commands
        .spawn((
            Transform::from_xyz(0.0, 4.0, ENTITY_INDEX),
            PointLight2d {
                radius: 96.0,
                color: Color::Srgba(YELLOW),
                intensity: 2.0,
                falloff: 4.0,
                ..default()
            },
        ))
        .id();

    commands
        .spawn((
            Candle,
            AnimationTimer(Timer::from_seconds(0.2, TimerMode::Repeating)),
            Sprite::from_atlas_image(
                spritesheet.texture.clone(),
                TextureAtlas {
                    layout: spritesheet.layout.clone(),
                    index: 0,
                },
            ),
            Transform::from_xyz(0., 2., ENTITY_INDEX),
        ))
        .add_child(light);
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
