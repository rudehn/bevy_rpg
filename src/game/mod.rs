use crate::{
    components::Position,
    game::{
        camera::move_camera,
        systems::{fov_update_system, sync_entity_transforms, update_goblin_visibility},
        turns::TurnOrderPlugin, // Import TurnOrderPlugin
    },
    map::{dungeon::DungeonPlugin, light::LightPlugin, map::MapPlugin},
    player::PlayerPlugin,
};
use bevy::prelude::*;

pub mod actions; // Declare the new actions module
mod ai;
pub mod camera;
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
        )) // Add TurnOrderPlugin here
        .add_systems(
            Update,
            (
                sync_entity_transforms, // Run first to update transforms immediately after position changes
                fov_update_system.after(sync_entity_transforms), // FOV updates after transforms are synced
                update_goblin_visibility
                    .run_if(|query: Query<(), Changed<Position>>| !query.is_empty())
                    .after(fov_update_system), // Visibility updates after FOV and transforms are synced
                move_camera.after(sync_entity_transforms), // Move camera after transforms are synced
            )
                .run_if(in_state(AppState::InGame)),
        )
        .init_resource::<TurnManager>();
    }
}
