use bevy::{color::palettes::css::YELLOW, prelude::*};
use bevy_ecs_tilemap::tiles::{TilePos, TileStorage};
use bevy_light_2d::prelude::*;
use bracket_lib::prelude::Point;
use rand::{self, Rng};

use crate::{
    constants::{ENTITY_INDEX, TILE_SIZE_X, TILE_SIZE_Y},
    map::{
        map::{DungeonMap, GRID_SIZE},
        tile::TileVisibility,
    },
};

pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CandleSpritesheet>()
            .add_plugins(Light2dPlugin)
            // .add_systems(Update, update_light_intensity);
            // .add_systems(Startup, , spawn_candles).chain()) // Removed spawn_candles
            .add_systems(Update, (update_candle_visibility, animate_candles).chain());
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

pub fn spawn_candle(
    commands: &mut Commands,
    candle_spritesheet: &Res<CandleSpritesheet>,
    pt: &Point,
) {
    commands.spawn((
        Candle,
        PointLight2d {
            radius: 200.0,
            color: Color::Srgba(YELLOW),
            intensity: 0.0, // Initially off
            falloff: 4.0,
            cast_shadows: true,
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
    // 1. We need TileStorage to look up tiles instantly (O(1)) instead of searching (O(N))
    map_query: Query<&TileStorage, With<DungeonMap>>,
    // 2. We check the visibility of the specific tile entity we find
    tile_vis_query: Query<&TileVisibility>,
    // 3. Candle components
    mut candle_query: Query<(&Transform, &mut Visibility, &mut PointLight2d), With<Candle>>,
    time: Res<Time>,
) {
    // Get the map storage. If the map isn't loaded yet, do nothing.
    let Ok(tile_storage) = map_query.single() else {
        return;
    };

    for (transform, mut candle_vis, mut light) in &mut candle_query {
        light.intensity = (time.elapsed_secs().sin() * 2.0 + 3.0).max(0.0);
        continue;
        // Calculate grid position
        let tile_pos = TilePos {
            x: (transform.translation.x / GRID_SIZE.x).floor() as u32,
            y: (transform.translation.y / GRID_SIZE.y).floor() as u32,
        };

        // --- THE CRITICAL FIX ---
        // Start with the assumption that the candle is HIDDEN.
        // If the map is culled, the tile is missing, or the coord is wrong,
        // this 'false' ensures the light turns off.
        let mut is_visible = false;

        // Try to get the tile entity from storage
        if let Some(tile_entity) = tile_storage.get(&tile_pos) {
            // If the tile exists, check its actual visibility component
            if let Ok(vis) = tile_vis_query.get(tile_entity) {
                is_visible = *vis == TileVisibility::Visible;
            }
        }

        // Apply visibility to the Candle Sprite
        *candle_vis = if is_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        light.intensity = if is_visible { 2.0 } else { 0.0 };
        println!("Is visible {}", is_visible);
        println!("intensity {}", light.intensity);

        // Apply visibility to the Light Child
        // for child in children.iter() {
        //     if let Ok(mut point_light) = light_query.get_mut(child) {
        //         // We ALWAYS set the intensity, ensuring it turns off if 'is_visible' is false
        //         point_light.intensity = if is_visible { 10.0 } else { 0.0 };
        //         println!("Is visible {}", is_visible);
        //         println!("intensity {}", point_light.intensity);
        //     }
        // }
    }
}
