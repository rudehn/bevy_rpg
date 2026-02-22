use bevy::ecs::component::Component;
use bracket_lib::prelude::Point;

#[derive(Component)]
pub struct Collider;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Position {
    pub fn to_point(&self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn from_point(point: Point) -> Self {
        Position {
            x: point.x,
            y: point.y,
        }
    }
}

#[derive(Component, Clone, Default)]
pub struct Viewshed {
    pub visible_tiles: Vec<Point>,
    pub range: i32,
}

impl Viewshed {
    pub fn new(range: i32) -> Self {
        Self {
            visible_tiles: Vec::new(),
            range,
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct BlocksVisibility;

#[derive(Component, Debug, Clone)]
pub struct Hidden;

#[derive(Component)]
pub struct Monster;

#[derive(Component)]
pub struct Name(pub String);
