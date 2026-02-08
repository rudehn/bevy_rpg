use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::TilePos;
use bevy_ecs_tilemap::tiles::TileStorage; // Added

use crate::{
    AppState,
    map::map::{DungeonMap, MAP_SIZE, SpawnDungeonMessage}, // Added MAP_SIZE
    player::Player,
};

#[derive(Resource)]
pub struct Floor(pub u32);

impl Default for Floor {
    fn default() -> Self {
        Floor(1) // Start at floor 1
    }
}
pub struct DungeonPlugin;

impl Plugin for DungeonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Floor>()
            .add_message::<MapTransitionMessage>()
            .add_systems(
                Update,
                map_transition_system.run_if(on_message::<MapTransitionMessage>),
            );
    }
}

#[derive(Message)]
pub struct MapTransitionMessage;

fn map_transition_system(
    mut commands: Commands,
    mut floor: ResMut<Floor>,
    q_map: Query<(Entity, &TileStorage), With<DungeonMap>>,
    mut message_writer: MessageWriter<SpawnDungeonMessage>,
) {
    // Despawn old map entities and their tiles
    for (map_entity, tile_storage) in q_map.iter() {
        for x in 0..MAP_SIZE.x {
            for y in 0..MAP_SIZE.y {
                if let Some(tile_entity) = tile_storage.get(&TilePos { x, y }) {
                    commands.entity(tile_entity).despawn();
                }
            }
        }
        commands.entity(map_entity).despawn();
    }

    // Increment floor
    floor.0 += 1;
    println!("Entering floor {}", floor.0);

    message_writer.write(SpawnDungeonMessage);
}
