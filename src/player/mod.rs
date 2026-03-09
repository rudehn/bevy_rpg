use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::time::Timer;

use crate::{
    assets::{PlayerAsset, PlayerAssetHandle, TileSpriteAssets},
    components::{Collider, GameEntityMarker, Inventory, Name, Position, Viewshed},
    constants::Z_PLAYER,
    game::{
        TurnManager,
        actions::SpeedStats,
        combat::{Damage, Health, HealthRegen},
        stats::{AttributeModifiers, Attributes, CombatStats, Level, Mana, RolledHp},
        level::{Experience, AvailableStatPoints},
    },
    map::map::GRID_SIZE,
    map::dungeon::{PlayerSpawnPoint, SpawnDungeonMessage, SpawnDungeonSet},
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
        .add_systems(
            Update,
            player_spawn_or_move_system
                .run_if(on_message::<SpawnDungeonMessage>)
                .after(SpawnDungeonSet),
        );
    }
}

#[derive(Component)]
pub struct Player;

pub fn player_spawn_or_move_system(
    mut commands: Commands,
    player_asset_handle: Res<PlayerAssetHandle>,
    player_assets: Res<Assets<PlayerAsset>>,
    tile_sprite_assets: Res<TileSpriteAssets>,
    spawn_point: Res<PlayerSpawnPoint>,
    mut q_player: Query<(Entity, &mut Transform, &mut Position), With<Player>>,
    mut turn_manager: ResMut<TurnManager>,
) {
    let player_asset = player_assets.get(&player_asset_handle.0).expect("Player asset not loaded");
    
    let new_grid_pos = Position {
        x: spawn_point.0.x,
        y: spawn_point.0.y,
    };

    if let Ok((_player_entity, mut _player_tf, mut player_pos)) = q_player.single_mut() {
        *player_pos = new_grid_pos;
    } else {
        let sprite_path_parts: Vec<&str> = player_asset.sprite.split('#').collect();
        let texture_path = sprite_path_parts[0];
        let index = sprite_path_parts[1].parse::<usize>().unwrap_or_default();

        let texture_handle = tile_sprite_assets.handles.get(texture_path).unwrap().clone();
        let layout_handle = tile_sprite_assets.layouts.get(texture_path).unwrap().clone();

        // Determine scale to fit one game map tile (GRID_SIZE)
        // Default to 32x32 for new assets
        let tile_size = UVec2::new(32, 32);
        let scale_x = GRID_SIZE.x / tile_size.x as f32;
        let scale_y = GRID_SIZE.y / tile_size.y as f32;

        let player_entity = commands
            .spawn((
                Player,
                Name(player_asset.name.clone()),
                GameEntityMarker,
                Collider,
                new_grid_pos,
                Viewshed::new(8), // Initial range; recalculated by stat_recalculation_system via PER
                Inventory { items: Vec::new(), capacity: 20 },
            ))
            .insert((
                Health {
                    current: player_asset.base_hp,
                    max: player_asset.base_hp,
                },
                HealthRegen {
                    regen_rate: 10,
                    regen_accumulator: 0,
                },
                Damage(player_asset.damage.clone()),
                Attributes {
                    strength: player_asset.strength,
                    dexterity: player_asset.dexterity,
                    constitution: player_asset.constitution,
                    agility: player_asset.agility,
                    intelligence: player_asset.intelligence,
                    perception: player_asset.perception,
                },
                AttributeModifiers::default(),
                Level { value: player_asset.level },
                CombatStats::default(),
                SpeedStats::default(),
                Experience {
                    current: 0,
                    next_level: 100,
                    spell_slots_unlocked: 1,
                },
                AvailableStatPoints(0),
                RolledHp(0),
                Mana {
                    current: player_asset.intelligence * 5,
                    max: player_asset.intelligence * 5,
                },
            ))
            .insert((
                Sprite::from_atlas_image(
                    texture_handle,
                    TextureAtlas {
                        index,
                        layout: layout_handle,
                    },
                ),
                Transform {
                    translation: Vec3::new(0.0, 0.0, Z_PLAYER),
                    scale: Vec3::new(scale_x, scale_y, 1.0),
                    ..Default::default()
                },
                RenderLayers::layer(1),
            ))
            .id();
        turn_manager.add_entity(player_entity);
    }
}
