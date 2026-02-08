use bevy::ecs::component::Component;

pub const FLOOR: usize = 49;
pub const WALL: usize = 40;
pub const DOWN_STAIRS: usize = 61;
pub const SOLDIER: usize = 97;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Wall,
    Floor,
    DownStairs,
    Empty,
}

#[derive(Component, Default, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TileVisibility {
    #[default]
    Hidden,
    Visible,
}

#[derive(Component, Default, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TileExplored {
    #[default]
    Unexplored,
    Explored,
}

pub fn is_walkable(tile: TileType) -> bool {
    match tile {
        TileType::Wall => false,
        TileType::Floor => true,
        TileType::DownStairs => true,
        TileType::Empty => false,
    }
}

pub fn is_opaque(tile: TileType) -> bool {
    matches!(tile, TileType::Wall)
}

pub fn tile_texture(tile: TileType) -> usize {
    match tile {
        TileType::Floor => FLOOR,
        TileType::Wall => WALL,
        TileType::DownStairs => DOWN_STAIRS,
        TileType::Empty => FLOOR,
    }
}
