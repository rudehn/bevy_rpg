use bevy::prelude::*;

use crate::{
    constants::{ENTITY_INDEX, TILE_SIZE_X, TILE_SIZE_Y},
    map::tile::SOLDIER,
    DungeonTileset,
};

const PLAYER_SPEED: f32 = 100.0;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player)
            .add_systems(Update, (move_player, move_camera));
    }
}

#[derive(Component)]
pub struct Player;

fn spawn_player(mut commands: Commands, tileset: Res<DungeonTileset>) {
    commands.spawn((
        Player,
        Sprite::from_atlas_image(
            tileset.texture.clone(),
            TextureAtlas {
                index: SOLDIER,
                layout: tileset.layout.clone(),
            },
        ),
        Transform::from_xyz(
            TILE_SIZE_X as f32 / 2.0,
            TILE_SIZE_Y as f32 / 2.0,
            ENTITY_INDEX,
        ),
    ));
}

fn move_player(
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut query: Query<&mut Transform, With<Player>>,
) {
    if let Ok(mut transform) = query.single_mut() {
        let mut direction = Vec3::ZERO;

        if input.pressed(KeyCode::KeyW) {
            direction.y += 1.0;
        }
        if input.pressed(KeyCode::KeyS) {
            direction.y -= 1.0;
        }
        if input.pressed(KeyCode::KeyA) {
            direction.x -= 1.0;
        }
        if input.pressed(KeyCode::KeyD) {
            direction.x += 1.0;
        }

        if direction.length() > 0.0 {
            direction = direction.normalize();
        }

        transform.translation += direction * PLAYER_SPEED * time.delta_secs();
    }
}

fn move_camera(
    player_query: Query<&Transform, With<Player>>,
    mut camera_query: Query<&mut Transform, (With<Camera>, Without<Player>)>,
) {
    if let Ok(player_transform) = player_query.single() {
        if let Ok(mut camera_transform) = camera_query.single_mut() {
            camera_transform.translation = player_transform.translation;
        }
    }
}
