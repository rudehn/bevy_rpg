use bevy::{prelude::*, time::Timer};
use bevy_ecs_tilemap::prelude::*;

use crate::{
    components::{Collider, Position, Viewshed},
    constants::ENTITY_INDEX,
    game::DungeonTileset,
    map::{
        map::{DungeonMap, GRID_SIZE, MAP_SIZE, PlayerSpawnPoint, spawn_dungeon},
        tile::{SOLDIER, TileType},
    },
};

pub struct PlayerPlugin;

#[derive(Resource)]
pub struct MovementTimer(Timer);

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MovementTimer(Timer::from_seconds(
            0.1,
            TimerMode::Repeating,
        )))
        .add_systems(Startup, spawn_player.after(spawn_dungeon)) // Ensure map is spawned first
        .add_systems(Update, (move_player, move_camera).chain());
    }
}

#[derive(Component)]
pub struct Player;

fn spawn_player(
    mut commands: Commands,
    tileset: Res<DungeonTileset>,
    spawn_point: Res<PlayerSpawnPoint>,
) {
    let spawn_pos = Transform::from_xyz(
        spawn_point.0.x as f32 * GRID_SIZE.x,
        spawn_point.0.y as f32 * GRID_SIZE.y,
        ENTITY_INDEX,
    );

    commands.spawn((
        Player,
        Collider,
        Position {
            x: spawn_point.0.x,
            y: spawn_point.0.y,
        },
        Viewshed::new(20),
        Sprite::from_atlas_image(
            tileset.texture.clone(),
            TextureAtlas {
                index: SOLDIER,
                layout: tileset.layout.clone(),
            },
        ),
        spawn_pos,
    ));
}

pub fn move_player(
    time: Res<Time>,
    mut timer: ResMut<MovementTimer>,
    keys: Res<ButtonInput<KeyCode>>,
    mut q_player: Query<(&mut Transform, &mut Position), With<Player>>,
    // Query the map to check for collisions
    q_map: Query<&TileStorage, With<DungeonMap>>,
    // Query tiles to check if they are walls
    q_blocked_tiles: Query<&TileType, With<Collider>>,
) {
    let Ok((mut player_tf, mut player_pos)) = q_player.single_mut() else {
        return;
    };
    let Ok(tile_storage) = q_map.single() else {
        return;
    };

    timer.0.tick(time.delta());

    let mut delta = IVec2::ZERO;
    if keys.pressed(KeyCode::ArrowUp) {
        delta.y = 1;
    }
    if keys.pressed(KeyCode::ArrowDown) {
        delta.y = -1;
    }
    if keys.pressed(KeyCode::ArrowLeft) {
        delta.x = -1;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        delta.x = 1;
    }

    if delta == IVec2::ZERO {
        return;
    }

    if timer.0.is_finished() {
        // 1. Calculate current grid position
        // Bevy ECS Tilemap provides helpers, but simple math works for square grids
        let current_grid_x = (player_tf.translation.x / GRID_SIZE.x).floor() as i32;
        let current_grid_y = (player_tf.translation.y / GRID_SIZE.y).floor() as i32;

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
        player_tf.translation.x = target_x as f32 * GRID_SIZE.x;
        player_tf.translation.y = target_y as f32 * GRID_SIZE.y;
        player_pos.x = target_x;
        player_pos.y = target_y;
    }
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
