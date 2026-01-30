use bevy::{
    math::{bounding::Aabb2d, bounding::IntersectsVolume},
    prelude::*,
};

use crate::{constants::{TILE_SIZE_X, TILE_SIZE_Y}, player::player::Player};

#[derive(Component)]
pub struct Collider;

pub struct CollisionPlugin;

impl Plugin for CollisionPlugin {
    fn build(&self, _app: &mut App) {
        // We can add collision detection systems here later
    }
}

pub fn collision_check(
    player_transform: &Transform,
    collider_query: &Query<&Transform,  (With<Collider>, Without<Player>)>,
    move_delta: Vec3,
) -> Vec3 {
    let player_size = Vec2::new(TILE_SIZE_X as f32, TILE_SIZE_Y as f32);
    let mut final_delta = move_delta;

    // Check for collisions in the X direction
    let new_x_translation = player_transform.translation + Vec3::new(move_delta.x, 0.0, 0.0);
    let player_aabb = Aabb2d::new(new_x_translation.truncate(), player_size / 2.0);
    for collider_transform in collider_query.iter() {
        let collider_aabb = Aabb2d::new(
            collider_transform.translation.truncate(),
            player_size / 2.0,
        );
        if player_aabb.intersects(&collider_aabb) {
            final_delta.x = 0.0;
            break;
        }
    }

    // Check for collisions in the Y direction
    let new_y_translation = player_transform.translation + Vec3::new(0.0, move_delta.y, 0.0);
    let player_aabb = Aabb2d::new(new_y_translation.truncate(), player_size / 2.0);
    for collider_transform in collider_query.iter() {
        let collider_aabb = Aabb2d::new(
            collider_transform.translation.truncate(),
            player_size / 2.0,
        );
        if player_aabb.intersects(&collider_aabb) {
            final_delta.y = 0.0;
            break;
        }
    }

    final_delta
}
