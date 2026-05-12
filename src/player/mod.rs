use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::time::Timer;

use bracket_lib::prelude::Point;

use crate::{
    assets::{
        ItemManifest, ItemManifestHandle, ItemSpriteAssets, PlayerAsset, PlayerAssetHandle,
    },
    character::{
        compose_attributes, derive_stats, CharacterChoice, ClassManifest, ClassManifestHandle,
        RaceManifest, RaceManifestHandle, Race, RaceTrait,
    },
    components::{
        Collider, Faction, FactionKind, FloorEntityMarker, GameEntityMarker, InInventory, Inventory, Name, Position,
        VeiledTyrantFactions, Viewshed,
    },
    constants::Z_PLAYER,
    game::{
        TurnManager,
        actions::SpeedStats,
        combat::{Damage, DamageType, Health, HealthRegen, Resistances},
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
    character_choice: Res<CharacterChoice>,
    race_manifest_handle: Res<RaceManifestHandle>,
    race_manifests: Res<Assets<RaceManifest>>,
    class_manifest_handle: Res<ClassManifestHandle>,
    class_manifests: Res<Assets<ClassManifest>>,
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

        // Resolve race + class assets from the player's character-creation
        // choice. The manifests are guaranteed loaded by the time we get here
        // because check_assets_loaded gates the Menu→InGame transition on them.
        let race_manifest = race_manifests
            .get(&race_manifest_handle.0)
            .expect("Race manifest not loaded");
        let class_manifest = class_manifests
            .get(&class_manifest_handle.0)
            .expect("Class manifest not loaded");
        let race_id = character_choice.race.name().to_lowercase();
        let class_id = character_choice.class.name().to_lowercase();
        let race_asset = race_manifest
            .races
            .get(&race_id)
            .unwrap_or_else(|| panic!("races.ron missing entry for {race_id}"));
        let class_asset = class_manifest
            .classes
            .get(&class_id)
            .unwrap_or_else(|| panic!("classes.ron missing entry for {class_id}"));

        // Spawn starting items from the chosen class's kit. The legacy
        // `player.ron` `starting_items` field is now unused by the live
        // character-creation flow — kept around only so player.ron stays
        // parseable; the runtime kit is class-driven.
        let starting_items = spawn_starting_items(
            &mut commands,
            &class_asset.starting_kit,
            &item_manifests,
            &item_manifest_handle,
            &item_sprite_assets,
        );

        let attributes =
            compose_attributes(race_asset, class_asset, character_choice.free_points);
        let derived = derive_stats(class_asset, &attributes);

        // Apply Elf's Keen Senses (+2 vision) at spawn. Other race effects
        // (Stoneblood poison resist, Halfling Lucky) are applied below via
        // their respective components / d20 helper.
        let mut viewshed_range = if player_asset.viewshed_range > 0 {
            player_asset.viewshed_range
        } else {
            8
        };
        if character_choice.race.racial_trait() == RaceTrait::KeenSenses {
            viewshed_range += 2;
        }

        // Resistances inherit the player-asset defaults, then Stoneblood
        // stacks 50% poison resistance on top.
        let mut resistances = Resistances::default();
        if character_choice.race.racial_trait() == RaceTrait::Stoneblood {
            *resistances.0.entry(DamageType::Poison).or_insert(0) += 50;
        }

        let player_entity = commands
            .spawn((
                Player,
                Name(player_asset.name.clone()),
                GameEntityMarker,
                Collider,
                new_grid_pos,
                Viewshed::new(viewshed_range),
                roguelike_engine::components::FovRevealsMap,
                Inventory {
                    items: starting_items,
                    capacity: 20,
                },
                Equipment::default(),
            ))
            .insert((
                Health {
                    current: derived.max_hp,
                    max: derived.max_hp,
                },
                HealthRegen {
                    regen_rate: player_asset.regen_rate,
                    regen_accumulator: 0,
                },
                Damage(player_asset.damage.clone()),
                Armor(player_asset.armor),
                Dodge(player_asset.dodge + derived.dodge),
                // Attribute mods are NOT baked into HitBonus / DamageBonus.
                // The hit-check and damage-roll systems read the attacker's
                // `Attributes` + the `AttackIntentMessage.source` and add
                // STR_mod for Melee, DEX_mod for Ranged. Only the static
                // `class_attack_bonus` lives in HitBonus at spawn.
                HitBonus(class_asset.class_attack_bonus),
                DamageBonus(0),
                SpeedStats::default(),
            ))
            .insert((
                StatusEffects::default(),
                Faction(VeiledTyrantFactions::player()),
                resistances,
                character_choice.race,
                character_choice.class,
                attributes,
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

        commands.entity(player_entity).insert(crate::game::ascii_mode::AsciiDisplay {
            ch: if player_asset.ascii_char.is_empty() { "@".to_string() } else { player_asset.ascii_char.clone() },
            color: player_asset.ascii_fg,
        });

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
                .remove::<FloorEntityMarker>()
                .remove::<Position>();
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
