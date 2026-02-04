use bevy::{color::palettes::css::YELLOW, prelude::*};
use bevy_light_2d::prelude::*;
use rand::{self, Rng};

use crate::constants::{ENTITY_INDEX, TILE_SIZE_X, TILE_SIZE_Y};

pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CandleSpritesheet>()
            .add_plugins(Light2dPlugin)
            // .add_systems(Startup, , spawn_candles).chain()) // Removed spawn_candles
            .add_systems(Update, animate_candles);
    }
}

#[derive(Resource, Default)]
pub struct CandleSpritesheet {
    // Made public
    pub layout: Handle<TextureAtlasLayout>, // Made public
    pub texture: Handle<Image>,             // Made public
}

#[derive(Component)]
pub struct Candle;

#[derive(Component, Deref, DerefMut)]
pub struct AnimationTimer(pub Timer); // Inner field made public

fn animate_candles(
    time: Res<Time>,
    mut query: Query<(&mut AnimationTimer, &mut Sprite), With<Candle>>,
    mut light_query: Query<&mut PointLight2d>,
) {
    for (mut timer, mut sprite) in &mut query {
        timer.tick(time.delta());
        if timer.is_finished()
            && let Some(ref mut texture_atlas) = sprite.texture_atlas
        {
            texture_atlas.index = (texture_atlas.index + 1) % 4;
        }
    }
    // println!("animating");
    // for mut light in light_query.iter_mut() {
    //     // This overwrites your visibility code!
    //     if light.intensity > 0.0 {
    //         let pct_change = rand::rng().random_range(-30.0..30.0) / 100.0;
    //         println!("Pct change {}", pct_change);
    //         println!("initial {}", light.intensity);
    //         light.intensity += light.intensity * pct_change;
    //         println!("final {}", light.intensity);
    //     }
    // }
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
