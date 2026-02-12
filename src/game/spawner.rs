use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::{
    components::{Collider, Goblin, Position, Viewshed},
    constants::ENTITY_INDEX,
    game::DungeonTileset,
    map::{map::GRID_SIZE, tile::GOBLIN},
};

pub fn spawn_goblin(commands: &mut Commands, tileset: &Res<DungeonTileset>, spawn_point: &Point) {
    let new_pos = Transform::from_xyz(
        spawn_point.x as f32 * GRID_SIZE.x,
        spawn_point.y as f32 * GRID_SIZE.y,
        ENTITY_INDEX,
    );
    let new_grid_pos = Position {
        x: spawn_point.x,
        y: spawn_point.y,
    };

    commands.spawn((
        Goblin,
        Name::new(String::from("Goblin")),
        Collider,
        new_grid_pos,
        Viewshed::new(12),
        Sprite::from_atlas_image(
            tileset.texture.clone(),
            TextureAtlas {
                index: GOBLIN,
                layout: tileset.layout.clone(),
            },
        ),
        new_pos,
    ));
}
