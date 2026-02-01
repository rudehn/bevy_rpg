use bevy::ecs::component::Component;

pub const FLOOR: usize = 49;
pub const WALL: usize = 40;
pub const SOLDIER: usize = 97;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Wall,
    Floor,
}
