use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::time::Timer;

use bracket_lib::prelude::Point;

use crate::{
    assets::{
        ItemManifest, ItemManifestHandle, ItemSpriteAssets, PlayerAsset, PlayerAssetHandle,
        TileSpriteAssets,
    },
    components::{
        Collider, FloorEntityMarker, GameEntityMarker, InInventory, Inventory, Name, Position,
        Viewshed,
    },
    constants::Z_PLAYER,
    game::{
        TurnManager,
        abilities::{Faction, FactionKind},
        actions::SpeedStats,
        combat::{Damage, Health, HealthRegen},
        items::Equipment,
        level::{AvailableStatPoints, Experience},
        magic::{ActiveSpells, KnownSpells, ManaRegen, SpellCooldowns},
        spawn_item,
        stats::{AttributeModifiers, Attributes, CombatStats, Level, Mana, RolledHp},
    },
    map::dungeon::{PlayerSpawnPoint, SpawnDungeonMessage, SpawnDungeonSet},
    map::map::GRID_SIZE,
};

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
    tile_sprite_assets: Res<TileSpriteAssets>,
    item_manifest_handle: Res<ItemManifestHandle>,
    item_manifests: Res<Assets<ItemManifest>>,
    item_sprite_assets: Res<ItemSpriteAssets>,
    spawn_point: Res<PlayerSpawnPoint>,
    mut q_player: Query<(Entity, &mut Transform, &mut Position), With<Player>>,
    mut turn_manager: ResMut<TurnManager>,
) {
    let player_asset = player_assets
        .get(&player_asset_handle.0)
        .expect("Player asset not loaded");

    let new_grid_pos = Position {
        x: spawn_point.0.x,
        y: spawn_point.0.y,
    };

    if let Ok((_player_entity, mut player_tf, mut player_pos)) = q_player.single_mut() {
        // Update Transform immediately so move_camera snaps this frame without
        // waiting for sync_entity_transforms, which may run before this system.
        player_tf.translation.x = spawn_point.0.x as f32 * GRID_SIZE.x;
        player_tf.translation.y = spawn_point.0.y as f32 * GRID_SIZE.y;
        *player_pos = new_grid_pos;
    } else {
        let (texture_path, index) = crate::assets::parse_sprite_path(&player_asset.sprite);

        let texture_handle = tile_sprite_assets
            .handles
            .get(texture_path)
            .unwrap()
            .clone();
        let layout_handle = tile_sprite_assets
            .layouts
            .get(texture_path)
            .unwrap()
            .clone();

        // Determine scale to fit one game map tile (GRID_SIZE)
        // Default to 32x32 for new assets
        let tile_size = UVec2::new(32, 32);
        let scale_x = GRID_SIZE.x / tile_size.x as f32;
        let scale_y = GRID_SIZE.y / tile_size.y as f32;

        // Spawn starting Short Bow + Arrows and collect entity IDs for the inventory.
        let starting_items: Vec<Entity> = {
            use crate::game::items::ItemStack;
            let mut items = Vec::new();
            if let Some(bow_entity) = spawn_item(
                &mut commands,
                "Short Bow",
                &Point::new(0, 0),
                &item_manifests,
                &item_manifest_handle,
                &item_sprite_assets,
            ) {
                commands
                    .entity(bow_entity)
                    .insert(InInventory)
                    .insert(Visibility::Hidden)
                    .remove::<FloorEntityMarker>();
                items.push(bow_entity);
            }
            if let Some(arrow_entity) = spawn_item(
                &mut commands,
                "Arrow",
                &Point::new(0, 0),
                &item_manifests,
                &item_manifest_handle,
                &item_sprite_assets,
            ) {
                commands
                    .entity(arrow_entity)
                    .insert(ItemStack {
                        count: 20,
                        max_stack: 30,
                    })
                    .insert(InInventory)
                    .insert(Visibility::Hidden)
                    .remove::<FloorEntityMarker>();
                items.push(arrow_entity);
            }
            // Starting spellbooks: Fire Dart and Spark
            for tome_name in &["Tome of Fire Dart", "Tome of Spark"] {
                if let Some(tome_entity) = spawn_item(
                    &mut commands,
                    tome_name,
                    &Point::new(0, 0),
                    &item_manifests,
                    &item_manifest_handle,
                    &item_sprite_assets,
                ) {
                    commands
                        .entity(tome_entity)
                        .insert(InInventory)
                        .insert(Visibility::Hidden)
                        .remove::<FloorEntityMarker>();
                    items.push(tome_entity);
                }
            }
            items
        };

        let player_entity = commands
            .spawn((
                Player,
                Name(player_asset.name.clone()),
                GameEntityMarker,
                Collider,
                new_grid_pos,
                Viewshed::new(8), // Initial range; recalculated by stat_recalculation_system via PER
                Inventory {
                    items: starting_items,
                    capacity: 20,
                },
                Equipment::default(),
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
                Level {
                    value: player_asset.level,
                },
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
                ManaRegen::default(),
            ))
            .insert((
                KnownSpells::default(),
                ActiveSpells::new(),
                SpellCooldowns::default(),
                Faction(FactionKind::Player),
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
