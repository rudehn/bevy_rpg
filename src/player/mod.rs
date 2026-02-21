use bevy::{prelude::*, time::Timer};

use crate::{
    assets::DungeonTileset,
    components::{Collider, Position, Viewshed},
    constants::ENTITY_INDEX,
    game::{Actor, PlayerAI, TurnManager, combat::{Health, Damage}}, // Added combat::Damage
    map::{
        dungeon::{PlayerSpawnPoint, SpawnDungeonMessage},
        map::GRID_SIZE,
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
        // .add_systems(Update, move_player.run_if(in_state(AppState::InGame)));
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
    let new_pos = Transform::from_xyz(
        spawn_point.0.x as f32 * GRID_SIZE.x,
        spawn_point.0.y as f32 * GRID_SIZE.y,
        ENTITY_INDEX + 0.1, // Player Z slightly higher
    );
    let new_grid_pos = Position {
        x: spawn_point.0.x,
        y: spawn_point.0.y,
    };

    if let Ok((_player_entity, mut _player_tf, mut player_pos)) = q_player.single_mut() { // _player_tf is no longer mutable
        // Player already exists, move them
        *player_pos = new_grid_pos;
    } else {
        // No player exists, spawn a new one
        let player_entity = commands
            .spawn((
                Player,
                Actor {
                    ai: Box::new(PlayerAI::default()),
                },
                Collider,
                new_grid_pos,
                Viewshed::new(20),
                Health { current: 20, max: 20 },
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
                Transform::from_xyz(0.0, 0.0, ENTITY_INDEX + 0.1),
            ))
            .id(); // Get the entity ID
        turn_manager.turn_queue.push_front(player_entity); // Add player to the turn queue
    }
}

// pub fn move_player(
//     time: Res<Time>,
//     mut timer: ResMut<MovementTimer>,
//     keys: Res<ButtonInput<KeyCode>>,
//     mut q_player: Query<(&mut Transform, &mut Position), With<Player>>,
//     // Query the map to check for collisions
//     q_map: Query<&TileStorage, With<DungeonECSMap>>,
//     // Query tiles to check if they are walls
//     q_blocked_tiles: Query<&TileType, With<Collider>>,
//     q_collidable_entities: Query<&Position, (With<Collider>, Without<Player>)>, // New query for other collidable entities
//     q_tile_types: Query<&TileType>,
//     mut ev_map_transition: MessageWriter<MapTransitionMessage>,
// ) {
//     let Ok((mut player_tf, mut player_pos)) = q_player.single_mut() else {
//         return;
//     };
//     let Ok(tile_storage) = q_map.single() else {
//         return;
//     };

//     timer.0.tick(time.delta());

//     let mut delta = IVec2::ZERO;
//     if keys.pressed(KeyCode::ArrowUp) {
//         delta.y = 1;
//     }
//     if keys.pressed(KeyCode::ArrowDown) {
//         delta.y = 1;
//     }
//     if keys.pressed(KeyCode::ArrowLeft) {
//         delta.x = 1;
//     }
//     if keys.pressed(KeyCode::ArrowRight) {
//         delta.x = 1;
//     }

//     if delta == IVec2::ZERO {
//         return;
//     }

//     if timer.0.is_finished() {
//         // 1. Calculate current grid position
//         // Bevy ECS Tilemap provides helpers, but simple math works for square grids
//         let current_grid_x = (player_tf.translation.x / GRID_SIZE.x).floor() as i32;
//         let current_grid_y = (player_tf.translation.y / GRID_SIZE.y).floor() as i32;

//         let target_x = current_grid_x + delta.x;
//         let target_y = current_grid_y + delta.y;

//         // 2. Check Bounds
//         if target_x < 0
//             || target_y < 0
//             || target_x >= MAP_SIZE.x as i32
//             || target_y >= MAP_SIZE.y as i32
//         {
//             return; // Out of bounds
//         }

//         let target_pos = TilePos {
//             x: target_x as u32,
//             y: target_y as u32,
//         };

//         // 3. Check Collision via TileStorage
//         // We ask the map: "What entity is at this position?"
//         if let Some(tile_entity) = tile_storage.get(&target_pos) {
//             // Check for DownStairs before checking for general collision
//             if let Ok(tile_type) = q_tile_types.get(tile_entity) {
//                 if matches!(tile_type, TileType::DownStairs) {
//                     ev_map_transition.write(MapTransitionMessage);
//                     return;
//                 }
//             }
//             // We found a tile entity, now let's check its component (TileType)
//             if q_blocked_tiles.get(tile_entity).is_ok() {
//                 return; // Block movement
//             }
//         }

//         // Check for other collidable entities
//         for other_collider_pos in q_collidable_entities.iter() {
//             if other_collider_pos.x == target_x && other_collider_pos.y == target_y {
//                 return; // Block movement if another collidable entity is in the way
//             }
//         }

//         // 4. Move Transform
//         // Center the player in the new tile
//         player_tf.translation.x = target_x as f32 * GRID_SIZE.x;
//         player_tf.translation.y = target_y as f32 * GRID_SIZE.y;
//         player_pos.x = target_x;
//         player_pos.y = target_y;
//     }
// }
