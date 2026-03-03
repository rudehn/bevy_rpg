use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::time::Timer;

use crate::{
    assets::DungeonTileset,
    components::{Collider, GameEntityMarker, Name, Position, Viewshed},
    constants::Z_PLAYER,
    game::{
        TurnManager,
        actions::SpeedStats,
        combat::{Damage, Health, HealthRegen},
        stats::{AttributeModifiers, Attributes, CombatStats, Level},
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
        // Player already exists, move them
        *player_pos = new_grid_pos;
    } else {
        // No player exists, spawn a new one
        // Use multiple insert calls to avoid tuple bundle size limit (15)
        let player_entity = commands
            .spawn((
                Player,
                Name("You".to_string()),
                GameEntityMarker,
                Collider,
                new_grid_pos,
                Viewshed::new(20),
            ))
            .insert((
                Health {
                    current: 10,
                    max: 10,
                },
                HealthRegen {
                    regen_rate: 10,
                    regen_accumulator: 0,
                },
                Damage("1d6".to_string()),
                Attributes {
                    strength: 10,
                    dexterity: 10,
                    constitution: 10,
                    agility: 10,
                },
                AttributeModifiers::default(),
                Level { value: 1 },
                CombatStats::default(),
                SpeedStats::default(),
            ))
            .insert((
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
            .id();
        turn_manager.add_entity(player_entity); // Add player to the turn queue
    }
}
