use bevy::prelude::*;
use bevy_ecs_tilemap::{
    map::TilemapGridSize,
    tiles::{TilePos, TileStorage},
};

use crate::{
    assets_plugin::DungeonTileset,
    components::Collider,
    constants::ENTITY_INDEX,
    map::{
        map::{DungeonMap, MAP_SIZE},
        tile::{SOLDIER, TileType},
    },
};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player)
            .add_systems(Update, (move_player, move_camera).chain());
    }
}

#[derive(Component)]
pub struct Player;

fn spawn_player(mut commands: Commands, tileset: Res<DungeonTileset>) {
    commands.spawn((
        Player,
        Collider,
        Sprite::from_atlas_image(
            tileset.texture.clone(),
            TextureAtlas {
                index: SOLDIER,
                layout: tileset.layout.clone(),
            },
        ),
        Transform::from_xyz(16.0, 16.0, ENTITY_INDEX),
    ));
}

// fn move_player(
//     input: Res<ButtonInput<KeyCode>>,
//     mut player_query: Query<&mut Transform, With<Player>>,
//     collider_query: Query<&Transform, (With<Collider>, Without<Player>)>,
// ) {
//     if let Ok(mut player_transform) = player_query.single_mut() {
//         let mut direction = Vec3::ZERO;

//         if input.just_pressed(KeyCode::KeyW) {
//             direction.y = TILE_SIZE_Y as f32;
//         } else if input.just_pressed(KeyCode::KeyS) {
//             direction.y = -(TILE_SIZE_Y as f32);
//         } else if input.just_pressed(KeyCode::KeyA) {
//             direction.x = -(TILE_SIZE_X as f32);
//         } else if input.just_pressed(KeyCode::KeyD) {
//             direction.x = TILE_SIZE_X as f32;
//         }

//         if direction.length() == 0.0 {
//             return;
//         }

//         let target = player_transform.translation + direction;

//         let mut collision = false;
//         for transform in collider_query.iter() {
//             // AABB collision check
//             if transform.translation.x == target.x && transform.translation.y == target.y {
//                 collision = true;
//                 break;
//             }
//         }

//         if !collision {
//             player_transform.translation = target;
//         }
//     }
// }

fn move_player(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut q_player: Query<&mut Transform, With<Player>>,
    // Query the map to check for collisions
    q_map: Query<(&TileStorage, &TilemapGridSize), With<DungeonMap>>,
    // Query tiles to check if they are walls
    q_blocked_tiles: Query<(&TileType, &Collider)>,
) {
    let Ok(mut player_tf) = q_player.single_mut() else {
        return;
    };
    let Ok((tile_storage, grid_size)) = q_map.single() else {
        return;
    };

    let mut delta = IVec2::ZERO;
    if keys.just_pressed(KeyCode::ArrowUp) {
        delta.y = 1;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        delta.y = -1;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        delta.x = -1;
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        delta.x = 1;
    }

    if delta == IVec2::ZERO {
        return;
    }

    // 1. Calculate current grid position
    // Bevy ECS Tilemap provides helpers, but simple math works for square grids
    let current_grid_x = (player_tf.translation.x / grid_size.x).floor() as i32;
    let current_grid_y = (player_tf.translation.y / grid_size.y).floor() as i32;

    let target_x = current_grid_x + delta.x;
    let target_y = current_grid_y + delta.y;

    // 2. Check Bounds
    if target_x < 0
        || target_y < 0
        || target_x >= MAP_SIZE.x as i32
        || target_y >= MAP_SIZE.y as i32
    {
        return; // Out of bounds
    }

    let target_pos = TilePos {
        x: target_x as u32,
        y: target_y as u32,
    };

    // 3. Check Collision via TileStorage
    // We ask the map: "What entity is at this position?"
    if let Some(tile_entity) = tile_storage.get(&target_pos) {
        // We found a tile entity, now let's check its component (TileType)
        if q_blocked_tiles.get(tile_entity).is_ok() {
            return; // Block movement  
        }
    }

    // 4. Move Transform
    // Center the player in the new tile
    player_tf.translation.x = (target_x as f32 * grid_size.x); // + grid_size.x;// / 2.0);
    player_tf.translation.y = (target_y as f32 * grid_size.y); // + grid_size.y;// / 2.0);
}

fn move_camera(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    player_query: Query<&Transform, With<Player>>,
    mut camera_query: Query<(&mut Transform, &mut Projection), (With<Camera>, Without<Player>)>,
) {
    if let Ok((mut camera_transform, mut camera_projection)) = camera_query.single_mut() {
        if let Ok(player_transform) = player_query.single() {
            camera_transform.translation.x = player_transform.translation.x;
            camera_transform.translation.y = player_transform.translation.y;
        }

        // Scale camera zoom
        let Projection::Orthographic(ortho) = &mut *camera_projection else {
            return;
        };

        if keyboard_input.pressed(KeyCode::KeyZ) {
            ortho.scale += 0.1;
        }

        if keyboard_input.pressed(KeyCode::KeyX) {
            ortho.scale -= 0.1;
        }

        ortho.scale = ortho.scale.clamp(0.25, 1.0);

        let z = camera_transform.translation.z;
        // Important! We need to restore the Z values when moving the camera around.
        // Bevy has a specific camera setup and this can mess with how our layers are shown.
        camera_transform.translation.z = z;
    }
}
