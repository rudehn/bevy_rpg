use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::time::Timer;

use bracket_lib::prelude::Point;

use crate::{
    assets::{
        ItemManifest, ItemManifestHandle, ItemSpriteAssets, PlayerAsset, PlayerAssetHandle,
    },
    components::{
        Collider, Faction, FactionKind, FloorEntityMarker, GameEntityMarker, InInventory, Inventory, Name, Position,
        Viewshed,
    },
    constants::Z_PLAYER,
    game::{
        TurnManager,
        actions::SpeedStats,
        combat::{Damage, Health, HealthRegen},
        items::Equipment,
        magic::StatusEffects,
        spawn_item,
        stats::{Armor, DamageBonus, Dodge, HitBonus},
    },
    map::dungeon::{PlayerSpawnPoint, SpawnDungeonMessage, SpawnDungeonSet, StairCooldown},
    map::map::GRID_SIZE,
};

use crate::assets::StartingItemDef;
use crate::game::items::ItemStack;

pub struct PlayerPlugin;

#[derive(Resource)]
pub struct MovementTimer(pub Timer);

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MovementTimer(Timer::from_seconds(
            0.025,
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
    item_manifest_handle: Res<ItemManifestHandle>,
    item_manifests: Res<Assets<ItemManifest>>,
    item_sprite_assets: Res<ItemSpriteAssets>,
    spawn_point: Res<PlayerSpawnPoint>,
    mut q_player: Query<(Entity, &mut Transform, &mut Position), With<Player>>,
    mut turn_manager: ResMut<TurnManager>,
    ascii_font: Option<Res<crate::game::ascii_mode::AsciiFont>>,
) {
    let player_asset = player_assets
        .get(&player_asset_handle.0)
        .expect("Player asset not loaded");

    let new_grid_pos = Position {
        x: spawn_point.0.x,
        y: spawn_point.0.y,
    };

    if let Ok((player_entity, mut player_tf, mut player_pos)) = q_player.single_mut() {
        info!(
            "player_spawn_or_move: teleporting from ({}, {}) to ({}, {})",
            player_pos.x, player_pos.y, spawn_point.0.x, spawn_point.0.y
        );
        // Update Transform immediately so move_camera snaps this frame without
        // waiting for sync_entity_transforms, which may run before this system.
        player_tf.translation.x = spawn_point.0.x as f32 * GRID_SIZE.x;
        player_tf.translation.y = spawn_point.0.y as f32 * GRID_SIZE.y;
        *player_pos = new_grid_pos;
        // Prevent player_stair_system from immediately re-triggering if
        // the player spawns on a stair tile (floor transitions).
        commands.entity(player_entity).insert(StairCooldown);
    } else {
        let tile_size = UVec2::new(32, 32);
        let scale_x = GRID_SIZE.x / tile_size.x as f32;
        let scale_y = GRID_SIZE.y / tile_size.y as f32;

        // Spawn starting items from player.ron manifest.
        let starting_items = spawn_starting_items(
            &mut commands,
            &player_asset.starting_items,
            &item_manifests,
            &item_manifest_handle,
            &item_sprite_assets,
        );

        let viewshed_range = if player_asset.viewshed_range > 0 {
            player_asset.viewshed_range
        } else {
            8
        };

        let player_entity = commands
            .spawn((
                Player,
                Name(player_asset.name.clone()),
                GameEntityMarker,
                Collider,
                new_grid_pos,
                Viewshed::new(viewshed_range),
                Inventory {
                    items: starting_items,
                    capacity: 20,
                },
                Equipment::default(),
            ))
            .insert((
                Health {
                    current: player_asset.max_hp,
                    max: player_asset.max_hp,
                },
                HealthRegen {
                    regen_rate: player_asset.regen_rate,
                    regen_accumulator: 0,
                },
                Damage(player_asset.damage.clone()),
                Armor(player_asset.armor),
                Dodge(player_asset.dodge),
                HitBonus(0),
                DamageBonus(0),
                SpeedStats::default(),
            ))
            .insert((
                StatusEffects::default(),
                Faction(FactionKind::player()),
            ))
            .insert((
                Transform {
                    translation: Vec3::new(0.0, 0.0, Z_PLAYER),
                    scale: Vec3::new(scale_x, scale_y, 1.0),
                    ..Default::default()
                },
                RenderLayers::layer(1),
            ))
            .id();

        if let Some(ref font) = ascii_font {
            crate::game::spawner::attach_ascii_glyph(
                &mut commands,
                player_entity,
                &player_asset.ascii_char,
                player_asset.ascii_fg,
                &font.0,
                Vec3::new(scale_x, scale_y, 1.0),
            );
        }

        turn_manager.add_entity(player_entity);
    }
}

/// Spawn starting inventory items from the player asset manifest.
fn spawn_starting_items(
    commands: &mut Commands,
    item_defs: &[StartingItemDef],
    item_manifests: &Res<Assets<ItemManifest>>,
    item_manifest_handle: &Res<ItemManifestHandle>,
    item_sprite_assets: &Res<ItemSpriteAssets>,
) -> Vec<Entity> {
    let mut items = Vec::new();
    for def in item_defs {
        if let Some(entity) = spawn_item(
            commands,
            &def.name,
            &Point::new(0, 0),
            item_manifests,
            item_manifest_handle,
            item_sprite_assets,
            None,
            None,
        ) {
            commands
                .entity(entity)
                .insert(InInventory)
                .insert(Visibility::Hidden)
                .remove::<FloorEntityMarker>();
            if def.count > 1 {
                let max_stack = item_manifests
                    .get(&item_manifest_handle.0)
                    .and_then(|m| m.items.get(def.name.as_str()))
                    .map(|a| a.max_stack)
                    .unwrap_or(1);
                commands.entity(entity).insert(ItemStack {
                    count: def.count,
                    max_stack,
                });
            }
            items.push(entity);
        }
    }
    items
}
