use bevy::color::palettes::css::YELLOW; // New import for YELLOW
use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;
use bevy_light_2d::prelude::PointLight2d;
use bracket_lib::prelude::{Point, field_of_view}; // New import

use crate::{
    assets_plugin::DungeonTileset,
    components::Collider,
    constants::{ENTITY_INDEX, TILE_SIZE_X, TILE_SIZE_Y}, // New imports
    map::{
        builders::level_builder,
        ecs_map::EcsMap,                                    // Import EcsMap
        light::{AnimationTimer, Candle, CandleSpritesheet}, // New imports
        tile::{FLOOR, TileExplored, TileType, TileVisibility, WALL},
    },
    player::player::Player,
    player::player::move_player, // Import move_player
};

// --------------------------------------------------------------------------------
// CONFIGURATION
// --------------------------------------------------------------------------------
pub const TILE_SIZE: TilemapTileSize = TilemapTileSize { x: 16.0, y: 16.0 };
pub const GRID_SIZE: TilemapGridSize = TilemapGridSize { x: 16.0, y: 16.0 };
pub const MAP_SIZE: TilemapSize = TilemapSize { x: 80, y: 60 };
pub const PLAYER_FOV_RADIUS: i32 = 12;

#[derive(Resource)]
pub struct PlayerSpawnPoint(pub Point);

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TilemapPlugin) // Required by bevy_ecs_tilemap
            .add_systems(Startup, spawn_dungeon)
            .add_systems(
                Update,
                (
                    update_tile_visibility.after(move_player),
                    update_candle_visibility.after(update_tile_visibility),
                ),
            );
    }
}

// Tag for the entity that holds the map storage
#[derive(Component)]
pub struct DungeonMap;

// --------------------------------------------------------------------------------
// SYSTEMS
// --------------------------------------------------------------------------------

pub fn spawn_dungeon(
    mut commands: Commands,
    dungeon_tileset: Res<DungeonTileset>,
    candle_spritesheet: Res<CandleSpritesheet>, // New parameter
) {
    // Create the Tilemap entity
    let map_entity = commands.spawn(DungeonMap).id();

    let mut tile_storage = TileStorage::empty(MAP_SIZE);

    // Run the builder
    let mut builder = level_builder(1, MAP_SIZE.x as i32, MAP_SIZE.y as i32);
    builder.build_map();

    // Bake the map into the ECS
    for y in 0..builder.build_data.map.height() {
        for x in 0..builder.build_data.map.width() {
            let pt = Point::new(x, y);
            let tile_pos = TilePos {
                x: x as u32,
                y: y as u32,
            };
            let tile_type = builder.build_data.map.get_tile(pt).unwrap();

            let texture_index = match tile_type {
                TileType::Floor => FLOOR,
                TileType::Wall => WALL,
                TileType::DownStairs => FLOOR, // Placeholder
            };

            let mut command = commands.spawn((
                TileBundle {
                    position: tile_pos,
                    tilemap_id: TilemapId(map_entity),
                    texture_index: TileTextureIndex(texture_index as u32),
                    color: TileColor(Color::BLACK), // Initially black for fog of war
                    ..Default::default()
                },
                tile_type,
                TileVisibility::Hidden,
                TileExplored::Unexplored,
            ));

            if tile_type == TileType::Wall {
                command.insert(Collider);
            }

            let tile_entity = command.id();
            tile_storage.set(&tile_pos, tile_entity);
        }
    }

    // Spawn candles
    for pt in builder.build_data.candle_spawn_points.iter() {
        let light = commands
            .spawn((
                // When adding light as a child, its transform should be relative to parent
                Transform::default(),
                PointLight2d {
                    radius: 96.0,
                    color: Color::Srgba(YELLOW),
                    intensity: 0.0, // Initially off
                    falloff: 4.0,
                    ..default()
                },
            ))
            .id();

        commands
            .spawn((
                Candle,
                AnimationTimer(Timer::from_seconds(0.2, TimerMode::Repeating)),
                Sprite::from_atlas_image(
                    candle_spritesheet.texture.clone(),
                    TextureAtlas {
                        layout: candle_spritesheet.layout.clone(),
                        index: 0,
                    },
                ),
                Transform::from_xyz(
                    pt.x as f32 * GRID_SIZE.x,
                    pt.y as f32 * GRID_SIZE.y,
                    ENTITY_INDEX, // Increased Z-index for candle sprite
                ),
            ))
            .add_child(light);
    }

    // Add the tilemap components to the map entity
    commands.entity(map_entity).insert(TilemapBundle {
        grid_size: GRID_SIZE,
        map_type: TilemapType::Square,
        size: MAP_SIZE,
        storage: tile_storage,
        texture: TilemapTexture::Single(dungeon_tileset.texture.clone()),
        tile_size: TILE_SIZE,
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..Default::default()
    });

    // Insert the player spawn point as a resource
    let spawn_point = builder.build_data.starting_position.unwrap();
    commands.insert_resource(PlayerSpawnPoint(Point::new(spawn_point.x, spawn_point.y)));
}

pub fn update_tile_visibility(
    player_query: Query<&Transform, With<Player>>,
    mut tile_render_query: Query<(
        &TilePos,
        &mut TileColor,
        &mut TileVisibility,
        &mut TileExplored,
    )>,
    tile_type_query: Query<&TileType>, // Immutable query for EcsMap
    map_query: Query<(&DungeonMap, &TileStorage)>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let Ok((_, tile_storage)) = map_query.single() else {
        return;
    };

    // Convert player world position to tile position
    let player_tile_pos = TilePos {
        x: (player_tf.translation.x / GRID_SIZE.x).floor() as u32,
        y: (player_tf.translation.y / GRID_SIZE.y).floor() as u32,
    };
    let player_point = Point::new(player_tile_pos.x as i32, player_tile_pos.y as i32);

    let ecs_map = EcsMap {
        tile_storage,
        tile_query: &tile_type_query, // Use the new query here
        map_size: MAP_SIZE,
    };

    // Calculate FOV
    let fov_tiles = field_of_view(player_point, PLAYER_FOV_RADIUS, &ecs_map);

    // Update tile visibility and color
    for (tile_pos, mut tile_color, mut tile_visibility, mut tile_explored) in
        tile_render_query.iter_mut()
    {
        let current_point = Point::new(tile_pos.x as i32, tile_pos.y as i32);

        if fov_tiles.contains(&current_point) {
            *tile_visibility = TileVisibility::Visible;
            *tile_explored = TileExplored::Explored;
            tile_color.0 = Color::WHITE; // Visible tiles are full bright
        } else {
            *tile_visibility = TileVisibility::Hidden;
            if *tile_explored == TileExplored::Explored {
                tile_color.0 = Color::srgb(0.5, 0.5, 0.5); // Explored but not visible are dim
            } else {
                tile_color.0 = Color::BLACK; // Unexplored and not visible are black
            }
        }
    }
}

pub fn update_candle_visibility(
    // 1. We need TileStorage to look up tiles instantly (O(1)) instead of searching (O(N))
    map_query: Query<&TileStorage, With<DungeonMap>>,
    // 2. We check the visibility of the specific tile entity we find
    tile_vis_query: Query<&TileVisibility>,
    // 3. Candle components
    mut candle_query: Query<(&Transform, &mut Visibility, &Children), With<Candle>>,
    // 4. Light components
    mut light_query: Query<&mut PointLight2d>,
) {
    // Get the map storage. If the map isn't loaded yet, do nothing.
    let Ok(tile_storage) = map_query.single() else {
        return;
    };

    for (transform, mut candle_vis, children) in candle_query.iter_mut() {
        // Calculate grid position
        let tile_pos = TilePos {
            x: (transform.translation.x / GRID_SIZE.x).floor() as u32,
            y: (transform.translation.y / GRID_SIZE.y).floor() as u32,
        };

        // --- THE CRITICAL FIX ---
        // Start with the assumption that the candle is HIDDEN.
        // If the map is culled, the tile is missing, or the coord is wrong,
        // this 'false' ensures the light turns off.
        let mut is_visible = false;

        // Try to get the tile entity from storage
        if let Some(tile_entity) = tile_storage.get(&tile_pos) {
            // If the tile exists, check its actual visibility component
            if let Ok(vis) = tile_vis_query.get(tile_entity) {
                is_visible = *vis == TileVisibility::Visible;
            }
        }

        // Apply visibility to the Candle Sprite
        *candle_vis = if is_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        // Apply visibility to the Light Child
        for child in children.iter() {
            if let Ok(mut point_light) = light_query.get_mut(child) {
                // We ALWAYS set the intensity, ensuring it turns off if 'is_visible' is false
                point_light.intensity = if is_visible { 10.0 } else { 0.0 };
                println!("Is visible {}", is_visible);
                println!("intensity {}", point_light.intensity);
            }
        }
    }
}
