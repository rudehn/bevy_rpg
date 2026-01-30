use bevy::prelude::*;

use crate::{
    DungeonTileset, constants::{TILE_INDEX, TILE_SIZE_X, TILE_SIZE_Y}, map::tile::{FLOOR, WALL}
};

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_tiles);
    }
}

const MAP_WIDTH: i32 = 15;
const MAP_HEIGHT: i32 = 15;

fn spawn_tiles(mut commands: Commands, tileset: Res<DungeonTileset>) {
    let mut spawn_tile = |position: (i32, i32), index: usize| {
        spawn_from_atlas(
            &mut commands,
            tile_translation(position.0, position.1).extend(TILE_INDEX),
            index,
            tileset.layout.clone(),
            tileset.texture.clone(),
        );
    };

    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let pos_x = x - MAP_WIDTH / 2;
            let pos_y = y - MAP_HEIGHT / 2;


            // Spawn walls around the perimeter
            if x == 0 {
                // Left wall
                spawn_tile((pos_x, pos_y), WALL);
            } else if x == MAP_WIDTH - 1 {
                // Right wall
                spawn_tile((pos_x, pos_y), WALL);
            } else if y == 0 {
                // Bottom wall
                spawn_tile((pos_x, pos_y), WALL);
            } else if y == MAP_HEIGHT - 1 {
                // Top wall
                spawn_tile((pos_x, pos_y), WALL);
            } else {
                // Floor
                spawn_tile((pos_x, pos_y), FLOOR);
            }
        }
    }
}

fn tile_translation(x: i32, y: i32) -> Vec2 {
    Vec2::new(
        x as f32 * TILE_SIZE_X as f32,
        y as f32 * TILE_SIZE_Y as f32,
    )
}

fn spawn_from_atlas(
    commands: &mut Commands,
    translation: Vec3,
    sprite_index: usize,
    atlas_handle: Handle<TextureAtlasLayout>,
    texture: Handle<Image>,
) {
    commands.spawn((
        Sprite::from_atlas_image(
            texture,
            TextureAtlas {
                index: sprite_index,
                layout: atlas_handle,
            },
        ),
        Transform::from_translation(translation),
    ));
}
