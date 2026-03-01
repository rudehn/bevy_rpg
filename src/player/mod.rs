use bevy::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::time::Timer;

use crate::{
    assets::DungeonTileset,
    components::{Collider, GameEntityMarker, Name, Position, Viewshed},
    constants::Z_PLAYER,
    game::{
        TurnManager,
        combat::{Damage, Health},
    }, // Added combat::Damage
    map::{
        dungeon::{PlayerSpawnPoint, SpawnDungeonMessage},
        tile::SOLDIER,
    },
};

pub struct PlayerPlugin;

#[derive(Resource)]
pub struct MovementTimer(pub Timer);

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MovementTimer(Timer::from_seconds(
            0.1,
            TimerMode::Repeating,
        )))
        // Player spawn/move now happens on SpawnDungeonMessage, after the dungeon has been spawned
        .add_systems(
            Update,
            player_spawn_or_move_system
                .run_if(on_message::<SpawnDungeonMessage>)
                .after(crate::map::dungeon::spawn_dungeon), // Reference the system correctly
        );
    }
}

#[derive(Component)]
pub struct Player;

pub fn player_spawn_or_move_system(
    mut commands: Commands,
    tileset: Res<DungeonTileset>,
    spawn_point: Res<PlayerSpawnPoint>,
    mut q_player: Query<(Entity, &mut Transform, &mut Position), With<Player>>,
    mut turn_manager: ResMut<TurnManager>, // Added TurnManager
) {
    let new_grid_pos = Position {
        x: spawn_point.0.x,
        y: spawn_point.0.y,
    };

    if let Ok((_player_entity, mut _player_tf, mut player_pos)) = q_player.single_mut() {
        // _player_tf is no longer mutable
        // Player already exists, move them
        *player_pos = new_grid_pos;
    } else {
        // No player exists, spawn a new one
        let player_entity = commands
            .spawn((
                Player,
                Name("You".to_string()),
                GameEntityMarker, // Add GameEntityMarker here
                Collider,
                new_grid_pos,
                Viewshed::new(20),
                Health {
                    current: 20,
                    max: 20,
                },
                Damage("1d6".to_string()), // Add Damage component
                Sprite::from_atlas_image(
                    tileset.texture.clone(),
                    TextureAtlas {
                        index: SOLDIER,
                        layout: tileset.layout.clone(),
                    },
                ),
                // Provide an initial Transform with the correct Z-order.
                // X and Y will be set by sync_entity_transforms when Position changes.
                Transform::from_xyz(0.0, 0.0, Z_PLAYER),
                RenderLayers::layer(1),
            ))
            .id(); // Get the entity ID
        turn_manager.add_entity(player_entity); // Add player to the turn queue
    }
}
