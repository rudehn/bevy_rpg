use crate::{
    assets::{ItemManifest, ItemManifestHandle, ItemSpriteAssets},
    components::{GameEntityMarker, Name, Position, Viewshed},
    ui::game_log::GameLogMessage,
    game::{
        camera::{move_camera, toggle_main_camera_visibility},
        combat::{CombatDamageSet, CombatPlugin, DeathEvent, GameRng, death_system},
        items::{ItemsPlugin, LootTable},
        level::LevelPlugin,
        particles::ParticlesPlugin,
        stats::StatsPlugin,
        systems::{fov_update_system, sync_entity_transforms, update_monster_visibility, update_item_visibility},
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
pub mod effects;
pub mod items;
pub mod level;
pub mod particles;
pub mod spawner;
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
    Victory,
}

#[derive(SubStates, Debug, Clone, PartialEq, Eq, Hash, Default)]
#[source(AppState = AppState::InGame)]
pub enum InGameState {
    #[default]
    Running,
    CharacterInfo,
    Inventory,
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
                ItemsPlugin,
                ParticlesPlugin,
            ))
            .add_systems(
                Update,
                (
                    sync_entity_transforms,
                    fov_update_system.after(sync_entity_transforms),
                    update_monster_visibility
                        .run_if(|query: Query<(), Changed<Position>>| !query.is_empty())
                        .after(fov_update_system),
                    update_item_visibility
                        .run_if(|query: Query<(), Changed<Viewshed>>| !query.is_empty())
                        .after(fov_update_system),
                    move_camera.after(sync_entity_transforms),
                    loot_drop_system.after(CombatDamageSet),
                    death_system.after(loot_drop_system),
                )
                    .run_if(in_state(InGameState::Running)),
            )
            .add_systems(
                Update,
                toggle_main_camera_visibility.run_if(state_changed::<AppState>),
            )
            .add_systems(OnEnter(AppState::GameOver), (despawn_game_entities, despawn_map))
            .add_systems(OnEnter(AppState::Victory), (despawn_game_entities, despawn_map))
            .init_resource::<TurnManager>();
    }
}

fn loot_drop_system(
    mut death_events: MessageReader<DeathEvent>,
    loot_query: Query<(&Position, &LootTable, &Name)>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    mut log_writer: MessageWriter<GameLogMessage>,
    item_manifests: Res<Assets<ItemManifest>>,
    item_manifest_handle: Res<ItemManifestHandle>,
    item_sprite_assets: Res<ItemSpriteAssets>,
) {
    use bracket_lib::prelude::Point;
    for event in death_events.read() {
        let Ok((position, loot_table, name)) = loot_query.get(event.target) else {
            continue;
        };
        let spawn_point = Point::new(position.x, position.y);
        for entry in &loot_table.entries {
            let roll = game_rng.0.range(0, 100);
            if (roll as f32) < entry.spawn_chance * 100.0 {
                spawn_item(
                    &mut commands,
                    &entry.item,
                    &spawn_point,
                    &item_manifests,
                    &item_manifest_handle,
                    &item_sprite_assets,
                );
                log_writer.write(GameLogMessage(format!(
                    "{} dropped a {}.", name.0, entry.item
                )));
            }
        }
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
