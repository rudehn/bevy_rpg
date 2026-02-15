use bevy::{color::palettes::css::YELLOW, prelude::*};
use bevy_light_2d::prelude::*;
use bracket_lib::prelude::Point;

use crate::{
    components::{Position, Viewshed},
    constants::ENTITY_INDEX,
    game::AppState,
    map::map::GRID_SIZE,
    player::Player,
};

pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CandleSpritesheet>()
            .add_plugins(Light2dPlugin)
            // .add_systems(Update, update_light_intensity);
            // .add_systems(Startup, , spawn_candles).chain()) // Removed spawn_candles
            .add_systems(
                Update,
                (update_candle_visibility, animate_candles)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
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
) {
    for (mut timer, mut sprite) in &mut query {
        timer.tick(time.delta());
        if timer.is_finished()
            && let Some(ref mut texture_atlas) = sprite.texture_atlas
        {
            texture_atlas.index = (texture_atlas.index + 1) % 4;
        }
    }
}

pub fn spawn_candle(
    commands: &mut Commands,
    candle_spritesheet: &Res<CandleSpritesheet>,
    pt: &Point,
) {
    commands.spawn((
        Candle,
        Position { x: pt.x, y: pt.y },
        PointLight2d {
            radius: 200.0,
            color: Color::Srgba(YELLOW),
            intensity: 0.0, // Initially off
            falloff: 4.0,
            cast_shadows: false,
            ..default()
        },
        AnimationTimer(Timer::from_seconds(0.2, TimerMode::Repeating)),
        Sprite::from_atlas_image(
            candle_spritesheet.texture.clone(),
            TextureAtlas {
                layout: candle_spritesheet.layout.clone(),
                index: 0,
            },
        ),
        Transform::from_xyz(
            pt.x as f32 * GRID_SIZE.x,
            pt.y as f32 * GRID_SIZE.y,
            ENTITY_INDEX, // Increased Z-index for candle sprite
        ),
    ));
}

fn update_light_intensity(
    // Query for entities that have the PointLight2d component
    mut query: Query<&mut PointLight2d>,
    time: Res<Time>,
) {
    for mut light in &mut query {
        // Update the intensity using a sine wave for a pulsating effect
        // The intensity value can be any f32
        light.intensity = (time.elapsed_secs().sin() * 2.0 + 3.0).max(0.0);
    }
}

pub fn update_candle_visibility(
    // Query for the player's Viewshed, only when it changes
    player_query: Query<&Viewshed, With<Player>>,
    mut candle_query: Query<(&Position, &mut Visibility, &mut PointLight2d), With<Candle>>,
) {
    // Only run if the player's viewshed has changed
    let Ok(player_viewshed) = player_query.single() else {
        return; // No player or viewshed hasn't changed
    };

    for (position, mut candle_vis, mut light) in &mut candle_query {
        let candle_grid_pos = Point::new(position.x, position.y);
        let is_visible_to_player = player_viewshed.visible_tiles.contains(&candle_grid_pos);

        // Update sprite visibility
        *candle_vis = if is_visible_to_player {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        // Update light intensity
        // If the candle is not visible to the player, turn its light off (dim to 0)
        light.intensity = if is_visible_to_player { 1.0 } else { 0.0 };
    }
}
