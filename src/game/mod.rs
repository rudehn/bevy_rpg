use crate::{
    components::{GameEntityMarker, Position},
    game::{
        camera::move_camera,
        combat::CombatPlugin,
        systems::{fov_update_system, sync_entity_transforms, update_monster_visibility},
        turns::TurnOrderPlugin,
    },
    map::{
        dungeon::{DungeonPlugin, Floor},
        light::LightPlugin,
        map::{DungeonECSMap, MapPlugin}, // Corrected: Added DungeonECSMap
    },
    player::PlayerPlugin,
};
use bevy::prelude::*;
pub mod actions;
mod ai;
pub mod camera;
pub mod combat;
mod spawner;
mod systems;
mod turns;
pub use ai::*;
use bevy_ecs_tilemap::tiles::TileStorage;
pub use spawner::*;
pub use turns::*;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Loading,
    Menu,
    InGame,
    GameOver,
}

pub struct GamePlugin;
impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            LightPlugin,
            MapPlugin,
            PlayerPlugin,
            DungeonPlugin,
            TurnOrderPlugin,
            CombatPlugin,
        ))
        .add_systems(
            Update,
            (
                sync_entity_transforms,
                fov_update_system.after(sync_entity_transforms),
                update_monster_visibility
                    .run_if(|query: Query<(), Changed<Position>>| !query.is_empty())
                    .after(fov_update_system),
                move_camera.after(sync_entity_transforms),
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(AppState::GameOver), despawn_game_entities)
        .add_systems(OnExit(AppState::GameOver), despawn_map)
        .init_resource::<TurnManager>();
    }
}

fn despawn_game_entities(
    mut commands: Commands,
    game_entities_query: Query<Entity, With<GameEntityMarker>>,
    mut turn_manager: ResMut<TurnManager>,
    mut floor: ResMut<Floor>,
) {
    info!("Despawning all game entities...");

    for entity in game_entities_query.iter() {
        commands.entity(entity).despawn();
    }

    // Reset turn manager to clear any remaining entities in the queue
    *turn_manager = TurnManager::default();
    *floor = Floor::default();

    info!("Finished despawning game entities.");
}

fn despawn_map(mut commands: Commands, mut maps: Query<(Entity, &mut TileStorage, &Transform)>) {
    let Some((tilemap_entity, mut tile_storage, _)) = maps
        .iter_mut()
        .sort_by::<&Transform>(|a, b| b.translation.z.partial_cmp(&a.translation.z).unwrap())
        .next()
    else {
        return;
    };

    commands.entity(tilemap_entity).despawn();
    for entity in tile_storage.drain() {
        commands.entity(entity).despawn();
    }
}
