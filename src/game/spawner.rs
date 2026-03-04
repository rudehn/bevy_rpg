use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::{
    assets::{MonsterAsset, MonsterManifest, MonsterManifestHandle, MonsterSpriteAssets},
    components::{Collider, GameEntityMarker, Monster, Name, Position, Viewshed},
    constants::{TILE_SIZE_X, TILE_SIZE_Y, Z_MONSTER},
    game::{
        MonsterAI, TurnManager,
        actions::SpeedStats,
        combat::{Damage, Health, HealthRegen},
        stats::{AttributeModifiers, Attributes, CombatStats, Level, MonsterBaseHealth},
        level::ExperienceReward,
    }, // Added combat::Damage
    map::map::GRID_SIZE,
};

pub fn spawn_monster(
    commands: &mut Commands,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
    monster_asset: &MonsterAsset,
    monster_sprite_assets: &Res<MonsterSpriteAssets>,
) {
    let tile_size = monster_asset.tile_size.unwrap_or(UVec2::new(32, 32));
    let scale_x = TILE_SIZE_X as f32 / tile_size.x as f32;
    let scale_y = TILE_SIZE_Y as f32 / tile_size.y as f32;

    let new_pos = Transform {
        translation: Vec3::new(
            spawn_point.x as f32 * GRID_SIZE.x,
            spawn_point.y as f32 * GRID_SIZE.y,
            Z_MONSTER,
        ),
        scale: Vec3::new(scale_x, scale_y, 1.0), // Use calculated scale
        ..Default::default()
    };
    let new_grid_pos = Position {
        x: spawn_point.x,
        y: spawn_point.y,
    };

    let sprite_path_parts: Vec<&str> = monster_asset.sprite.split('#').collect();
    let texture_path = sprite_path_parts[0];
    let index = sprite_path_parts[1].parse::<usize>().unwrap_or_default();

    let texture_handle = monster_sprite_assets
        .handles
        .get(texture_path)
        .unwrap()
        .clone();
    let layout_handle = monster_sprite_assets
        .layouts
        .get(texture_path)
        .unwrap()
        .clone();

    // Calculate XP reward: Base 10 + (Level * 5) + (Base HP / 2)
    let xp_reward = 10 + (monster_asset.level * 5) + (monster_asset.base_hp / 2);

    // Use multiple insert calls to avoid large tuple bundle limit (15)
    let monster_entity = commands
        .spawn((
            Monster,
            GameEntityMarker,
            Name(monster_asset.name.clone()),
            MonsterAI::default(),
            Collider,
            new_grid_pos,
            new_pos,
            Viewshed::new(monster_asset.vision_range as i32),
        ))
        .insert((
            Health {
                current: 10, // Initial value, recalculated by stats system
                max: 10,
            },
            Damage(monster_asset.damage.clone()),
            SpeedStats::default(),
            Attributes {
                strength: monster_asset.strength,
                dexterity: monster_asset.dexterity,
                constitution: monster_asset.constitution,
                agility: monster_asset.agility,
            },
            AttributeModifiers::default(),
            Level {
                value: monster_asset.level,
            },
            MonsterBaseHealth {
                value: monster_asset.base_hp,
            },
            CombatStats::default(),
            ExperienceReward(xp_reward),
        ))
        .insert((
            Sprite::from_atlas_image(
                texture_handle,
                TextureAtlas {
                    index,
                    layout: layout_handle,
                },
            ),
            RenderLayers::layer(1),
        ))
        .id();

    if let Some(regen_rate) = monster_asset.regen {
        commands.entity(monster_entity).insert(HealthRegen {
            regen_rate,
            regen_accumulator: 0,
        });
    }

    turn_manager.add_entity(monster_entity);
}

pub fn spawn_monster_by_name(
    commands: &mut Commands,
    monster_name: &str,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
    monster_manifests: &Res<Assets<MonsterManifest>>,
    monster_manifest_handle: &Res<MonsterManifestHandle>,
    monster_sprite_assets: &Res<MonsterSpriteAssets>,
) {
    if let Some(manifest) = monster_manifests.get(&monster_manifest_handle.0) {
        if let Some(monster_asset) = manifest.monsters.get(monster_name) {
            spawn_monster(
                commands,
                spawn_point,
                turn_manager,
                monster_asset,
                monster_sprite_assets,
            );
        } else {
            warn!("Monster '{}' not found in manifest.", monster_name);
        }
    } else {
        error!("Monster manifest not loaded.");
    }
}
