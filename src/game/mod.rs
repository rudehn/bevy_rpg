use crate::{
    components::{GameEntityMarker, Position},
    game::{
        camera::move_camera,
        combat::CombatPlugin, // Added CombatPlugin import
        systems::{fov_update_system, sync_entity_transforms, update_monster_visibility},
        turns::TurnOrderPlugin,
    },
    map::{
        dungeon::{DungeonPlugin, Floor},
        light::LightPlugin,
        map::MapPlugin,
    },
    player::PlayerPlugin,
};
use bevy::prelude::*;

pub mod actions; // Declare the new actions module
mod ai;
pub mod camera;
pub mod combat; // Added combat module declaration
mod spawner;
mod systems;
mod turns;
pub use ai::*;
pub use spawner::*;
pub use turns::*; // Expose the turns module

// Removed: MinimapCamera and MainCamera component definitions

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
            CombatPlugin, // Added CombatPlugin here
        ))
        .add_systems(
            Update,
            (
                sync_entity_transforms, // Run first to update transforms immediately after position changes
                fov_update_system.after(sync_entity_transforms), // FOV updates after transforms are synced
                update_monster_visibility
                    .run_if(|query: Query<(), Changed<Position>>| !query.is_empty()) // Re-added run_if
                    .after(fov_update_system), // Visibility updates after FOV and transforms are synced
                move_camera.after(sync_entity_transforms), // Move camera after transforms are synced
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(AppState::GameOver), despawn_game_entities)
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
