use crate::{
    components::{GameEntityMarker, Position},
    game::{
        camera::{move_camera, toggle_main_camera_visibility},
        combat::{CombatPlugin, death_system},
        level::{LevelPlugin, xp_award_system},
        stats::StatsPlugin,
        systems::{fov_update_system, sync_entity_transforms, update_monster_visibility},
        turns::TurnOrderPlugin,
    },
    map::{
        dungeon::{DungeonPlugin, Floor},
        light::LightPlugin,
        map::MapPlugin,
    },
    player::PlayerPlugin,
    ui::game_log::GameLog,
};
use bevy::prelude::*;
pub mod actions;
mod ai;
pub mod camera;
pub mod combat;
pub mod level;
mod spawner;
pub mod stats;
mod systems;
pub mod turns;
pub use ai::*;
pub use spawner::*;
pub use turns::*;

use crate::map::map::DungeonECSMap;
use crate::map::tile::TileMarker;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Loading,
    Menu,
    InGame,
    GameOver,
}

#[derive(SubStates, Debug, Clone, PartialEq, Eq, Hash, Default)]
#[source(AppState = AppState::InGame)]
pub enum InGameState {
    #[default]
    Running,
    CharacterInfo,
}

pub struct GamePlugin;
impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_sub_state::<InGameState>()
            .add_plugins((
                LightPlugin,
                MapPlugin,
                PlayerPlugin,
                DungeonPlugin,
                TurnOrderPlugin,
                CombatPlugin,
                StatsPlugin,
                LevelPlugin,
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
                    death_system.after(xp_award_system),
                )
                    .run_if(in_state(InGameState::Running)),
            )
            .add_systems(
                Update,
                toggle_main_camera_visibility.run_if(state_changed::<AppState>),
            )
            .add_systems(OnEnter(AppState::GameOver), (despawn_game_entities, despawn_map))
            .init_resource::<TurnManager>();
    }
}

fn despawn_game_entities(
    mut commands: Commands,
    game_entities_query: Query<Entity, With<GameEntityMarker>>,
    mut turn_manager: ResMut<TurnManager>,
    mut floor: ResMut<Floor>,
    mut game_log: ResMut<GameLog>,
) {
    info!("Despawning all game entities...");

    for entity in game_entities_query.iter() {
        commands.entity(entity).despawn();
    }

    // Reset turn manager to clear any remaining entities in the queue
    *turn_manager = TurnManager::default();
    *floor = Floor::default();
    game_log.entries.clear();

    info!("Finished despawning game entities.");
}

fn despawn_map(
    mut commands: Commands, 
    q_map: Query<Entity, With<DungeonECSMap>>,
    q_tiles: Query<Entity, With<TileMarker>>,
) {
    for entity in q_map.iter() {
        commands.entity(entity).despawn();
    }
    for entity in q_tiles.iter() {
        commands.entity(entity).despawn();
    }
}
