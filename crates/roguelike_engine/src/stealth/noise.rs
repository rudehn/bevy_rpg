//! Per-tile noise map. V1 ships the decay system; no source writes to
//! it. The game's `compute_stealth_mod` calls `noise_modifier(pos, map)`
//! which currently returns 0 because the map stays at zero. The V2
//! noise phase will add a producer that writes positive values from
//! action events (movement, attacks, staff zaps).

use bevy::prelude::*;
use bracket_lib::prelude::Point;

#[derive(Resource, Debug, Clone, Default)]
pub struct NoiseMap {
    pub tiles: Vec<i32>,
    pub width: usize,
    pub height: usize,
}

impl NoiseMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            tiles: vec![0; width * height],
            width,
            height,
        }
    }

    pub fn at(&self, pos: Point) -> i32 {
        if pos.x < 0 || pos.y < 0 {
            return 0;
        }
        let (x, y) = (pos.x as usize, pos.y as usize);
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.tiles[y * self.width + x]
    }
}

/// V1: returns a negative penalty proportional to the noise level at
/// the target's tile (loud tile → less stealthy). With no producer in
/// V1, this always returns 0 in practice. V2 noise phase populates
/// `NoiseMap` and this function automatically becomes meaningful.
pub fn noise_modifier(pos: Point, map: &NoiseMap) -> i32 {
    -map.at(pos)
}

/// Runs once per game turn. Decrements every cell by 1, clamped to 0.
pub fn noise_decay_system(mut map: ResMut<NoiseMap>) {
    for cell in &mut map.tiles {
        *cell = (*cell - 1).max(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_bounds_check_returns_zero() {
        let m = NoiseMap::new(4, 4);
        assert_eq!(m.at(Point::new(-1, 0)), 0);
        assert_eq!(m.at(Point::new(0, -1)), 0);
        assert_eq!(m.at(Point::new(4, 0)), 0);
        assert_eq!(m.at(Point::new(0, 4)), 0);
    }

    #[test]
    fn noise_modifier_returns_negative_of_tile_value() {
        let mut m = NoiseMap::new(4, 4);
        m.tiles[4 + 2] = 5; // (x=2, y=1)
        assert_eq!(noise_modifier(Point::new(2, 1), &m), -5);
    }

    #[test]
    fn decay_in_isolation() {
        let mut m = NoiseMap::new(2, 2);
        m.tiles = vec![3, 1, 0, 5];
        for cell in &mut m.tiles {
            *cell = (*cell - 1).max(0);
        }
        assert_eq!(m.tiles, vec![2, 0, 0, 4]);
    }

    #[test]
    fn decay_floors_at_zero() {
        let mut m = NoiseMap::new(1, 1);
        m.tiles = vec![0];
        for cell in &mut m.tiles {
            *cell = (*cell - 1).max(0);
        }
        assert_eq!(m.tiles, vec![0]);
    }
}
