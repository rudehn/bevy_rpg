use bevy::ecs::component::Component;

pub const FLOOR: usize = 49;
pub const WALL: usize = 40;
pub const SOLDIER: usize = 97;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Wall,
    Floor,
    DownStairs,
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
    }
}

pub fn is_opaque(tile: TileType) -> bool {
    matches!(tile, TileType::Wall)
}
