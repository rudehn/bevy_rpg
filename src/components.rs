use bevy::ecs::component::Component;

#[derive(Component)]
pub struct Collider;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}
