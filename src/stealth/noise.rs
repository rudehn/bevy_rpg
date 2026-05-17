//! Noise map stub. Full implementation lands in Task B3.

use bevy::prelude::*;
use bracket_lib::prelude::Point;

#[derive(Resource, Debug, Clone, Default)]
pub struct NoiseMap;

pub fn noise_modifier(_pos: Point, _map: &NoiseMap) -> i32 {
    0
}

pub fn noise_decay_system(_map: ResMut<NoiseMap>) {}
