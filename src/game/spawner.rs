use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::{
    assets::{DungeonTileset, MonsterAsset},
    components::{Collider, Goblin, Position, Viewshed},
    constants::ENTITY_INDEX,
    game::{Actor, MonsterAI, TurnManager},
    map::{map::GRID_SIZE, tile::GOBLIN},
};

pub fn spawn_monster(
    commands: &mut Commands,
    tileset: &Res<DungeonTileset>,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
    monster_asset: &MonsterAsset,
    asset_server: &Res<AssetServer>,
) {
    let new_pos = Transform::from_xyz(
        spawn_point.x as f32 * GRID_SIZE.x,
        spawn_point.y as f32 * GRID_SIZE.y,
        ENTITY_INDEX,
    );
    let new_grid_pos = Position {
        x: spawn_point.x,
        y: spawn_point.y,
    };

    let sprite_path_parts: Vec<&str> = monster_asset.sprite.split('#').collect();
    let texture_path = sprite_path_parts[0];
    let index = sprite_path_parts[1].parse::<usize>().unwrap_or_default();

    let texture_handle = asset_server.load::<Image>(texture_path.to_string());
    let layout_handle = tileset.layout.clone(); // Assuming a common tileset layout for now

    let monster_entity = commands
        .spawn((
            Name::new(monster_asset.name.clone()),
            Actor {
                ai: Box::new(MonsterAI::default()),
            },
            Collider,
            new_grid_pos,
            new_pos,
            Viewshed::new(monster_asset.vision_range as i32),
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

// pub fn spawn_monsters_from_manifest(
//     mut commands: Commands,
//     asset_server: Res<AssetServer>,
//     monster_manifest_handle: Res<MonsterManifestHandle>,
//     monster_manifests: Res<Assets<MonsterManifest>>,
//     mut next_state: ResMut<NextState<AppState>>,
//     tileset: Res<DungeonTileset>,
//     mut turn_manager: ResMut<TurnManager>,
//     mut monster_sprite_assets: ResMut<MonsterSpriteAssets>,
// ) {
//     if let Some(manifest) = monster_manifests.get(&monster_manifest_handle.0) {
//         // Example: Spawn a goblin at a fixed point for now
//         if let Some(goblin_asset) = manifest.monsters.get("goblin") {
//             spawn_monster(
//                 &mut commands,
//                 &tileset,
//                 &Point::new(10, 10), // Example spawn point
//                 &mut turn_manager,
//                 goblin_asset,
//                 &mut monster_sprite_assets,
//                 &asset_server,
//             );
//         }
//         next_state.set(AppState::Menu); // Transition to Menu after spawning
//     }
// }

pub fn spawn_goblin(
    commands: &mut Commands,
    tileset: &Res<DungeonTileset>,
    spawn_point: &Point,
    turn_manager: &mut ResMut<TurnManager>,
) {
    let new_pos = Transform::from_xyz(
        spawn_point.x as f32 * GRID_SIZE.x,
        spawn_point.y as f32 * GRID_SIZE.y,
        ENTITY_INDEX,
    );
    let new_grid_pos = Position {
        x: spawn_point.x,
        y: spawn_point.y,
    };

    let goblin_entity = commands
        .spawn((
            Goblin,
            Name::new(String::from("Goblin")),
            Actor {
                ai: Box::new(MonsterAI::default()),
            },
            Collider,
            new_grid_pos,
            Viewshed::new(12),
            Sprite::from_atlas_image(
                tileset.texture.clone(),
                TextureAtlas {
                    index: GOBLIN,
                    layout: tileset.layout.clone(),
                },
            ),
            new_pos,
        ))
        .id(); // Get the entity ID

    turn_manager.turn_queue.push_back(goblin_entity);
}
