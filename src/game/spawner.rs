use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::{
    assets::{MonsterAsset, MonsterManifest, MonsterManifestHandle, MonsterSpriteAssets},
    components::{Collider, Monster, Position, Viewshed},
    constants::{ENTITY_INDEX, TILE_SIZE_X, TILE_SIZE_Y},
    game::{Actor, MonsterAI, TurnManager, combat::{Health, Damage}}, // Added combat::Damage
    map::map::GRID_SIZE,
};

pub fn spawn_monster(
    commands: &mut Commands,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
    monster_asset: &MonsterAsset,
    monster_sprite_assets: &Res<MonsterSpriteAssets>,
) {
    let scale_x = TILE_SIZE_X as f32 / monster_asset.tile_size.x as f32;
    let scale_y = TILE_SIZE_Y as f32 / monster_asset.tile_size.y as f32;

    let new_pos = Transform {
        translation: Vec3::new(
            spawn_point.x as f32 * GRID_SIZE.x,
            spawn_point.y as f32 * GRID_SIZE.y,
            ENTITY_INDEX,
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

    let texture_handle = monster_sprite_assets.handles.get(texture_path).unwrap().clone();
    let layout_handle = monster_sprite_assets.layouts.get(texture_path).unwrap().clone();

    let monster_entity = commands
        .spawn((
            Monster,
            Name::new(monster_asset.name.clone()),
            Actor {
                ai: Box::new(MonsterAI::default()),
            },
            Collider,
            new_grid_pos,
            new_pos,
            Viewshed::new(monster_asset.vision_range as i32),
            Health { current: monster_asset.health, max: monster_asset.health },
            Damage(monster_asset.damage.clone()), // Add Damage component
            Sprite::from_atlas_image(
                texture_handle,
                TextureAtlas {
                    index,
                    layout: layout_handle,
                },
            ),
        ))
        .id();

    turn_manager.turn_queue.push_back(monster_entity);
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
