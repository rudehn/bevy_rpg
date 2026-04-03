//! Entity-based fire system — fire entities with LightSource for unified lighting.
//!
//! Fire is spawned as ECS entities with Position + LightSource + FireMarker.
//! The light system automatically picks them up. Fire spreads to adjacent
//! flammable tiles each turn and decays to embers.

use std::collections::HashSet;

use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};
use bracket_lib::random::RandomNumberGenerator;

use crate::components::{FloorEntityMarker, GameEntityMarker, Position};
use crate::game::magic::{StatusEffectKind, StatusEffects};
use crate::game::turns::TurnEndEvent;
use crate::map::light::{LightSourceData, LightSources};
use crate::map::map::Map;
use crate::map::tile::{
    Decoration, DecorationMutationMessage, LiquidType, TerrainType, TileMutationMessage,
};
use crate::ui::game_log::GameLogMessage;

/// Fire decay chance per turn out of 100. ~20% = fire lasts ~5 turns on average.
const FIRE_DECAY_CHANCE: i32 = 20;
/// Duration of burning status applied to creatures standing in fire.
const BURN_DURATION: u32 = 5;
/// Damage per turn for burning from standing in fire.
const BURN_DAMAGE: i32 = 3;

// --- Components & Resources ---

/// Marker for fire entities on the map.
#[derive(Component)]
pub struct FireMarker;

/// Spatial index of burning tile positions. Updated when fire spawns/despawns.
/// Allows fast "is this tile on fire?" checks without querying all fire entities.
#[derive(Resource, Default)]
pub struct FireTiles(pub HashSet<(i32, i32)>);

// --- Fire tick system ---

/// Processes fire spread, decay, and creature ignition once per turn.
pub fn fire_tick_system(
    mut commands: Commands,
    mut turn_end: MessageReader<TurnEndEvent>,
    fire_query: Query<(Entity, &Position), With<FireMarker>>,
    mut fire_tiles: ResMut<FireTiles>,
    map: Res<Map>,
    mut decoration_writer: MessageWriter<DecorationMutationMessage>,
    mut tile_writer: MessageWriter<TileMutationMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut creature_query: Query<(&Position, &mut StatusEffects, &crate::components::Name)>,
    mut light_sources: ResMut<LightSources>,
    mut gas_tiles: ResMut<crate::game::gas::GasTiles>,
    player_query: Query<&crate::components::Viewshed, With<crate::player::Player>>,
) {
    let count = turn_end.read().count();
    if count == 0 {
        return;
    }

    let mut rng = RandomNumberGenerator::new();

    let current_fires: Vec<(Entity, i32, i32)> = fire_query
        .iter()
        .map(|(e, p)| (e, p.x, p.y))
        .collect();

    let mut new_fires: Vec<(i32, i32)> = Vec::new();
    let mut decayed: Vec<(Entity, i32, i32)> = Vec::new();
    let mut steam_spawns: Vec<(i32, i32)> = Vec::new();

    // Pass 1: determine spread and decay
    for &(entity, x, y) in &current_fires {
        if rng.range(0, 100) < FIRE_DECAY_CHANCE {
            decayed.push((entity, x, y));
            continue;
        }
        // Spread to cardinal neighbors
        for &(dx, dy) in &[(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let (nx, ny) = (x + dx, y + dy);
            if !map.in_bounds(Point::new(nx, ny)) {
                continue;
            }
            if fire_tiles.0.contains(&(nx, ny)) {
                continue;
            }
            let nidx = map.xy_idx(nx, ny);
            let ntile = map.tiles[nidx];
            // Fire + water = steam
            if ntile.liquid == LiquidType::Water || ntile.liquid == LiquidType::ShallowWater {
                steam_spawns.push((nx, ny));
                continue;
            }
            if ntile.liquid != LiquidType::None {
                continue; // Lava, chasm — no spread, no steam
            }
            let flammability = ntile
                .decoration
                .flammability()
                .max(ntile.terrain.flammability());
            if flammability > 0 && rng.range(0, 100) < flammability as i32 {
                new_fires.push((nx, ny));
            }
        }
    }

    // Spawn steam where fire met water — large billowing cloud.
    // Steam appears on the water tile itself plus its cardinal water neighbors.
    let mut all_steam: Vec<(i32, i32, u8)> = Vec::new();
    for &(x, y) in &steam_spawns {
        all_steam.push((x, y, crate::game::gas::MAX_CONCENTRATION));
        // Spread to cardinal neighbors that are walkable
        for &(dx, dy) in &[(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let (nx, ny) = (x + dx, y + dy);
            if !map.in_bounds(Point::new(nx, ny)) {
                continue;
            }
            let nidx = map.xy_idx(nx, ny);
            if crate::map::tile::is_passable(map.tiles[nidx]) {
                all_steam.push((nx, ny, crate::game::gas::MAX_CONCENTRATION - 1));
            }
        }
    }
    for (x, y, conc) in &all_steam {
        crate::game::gas::spawn_gas(
            &mut commands, *x, *y,
            crate::game::gas::GasType::Steam,
            *conc,
            &mut gas_tiles,
        );
    }

    // Pass 2: decay — despawn fire entity, remove light, place embers
    for (entity, x, y) in &decayed {
        fire_tiles.0.remove(&(*x, *y));
        light_sources.remove_at(*x, *y);
        decoration_writer.write(DecorationMutationMessage {
            position: Point::new(*x, *y),
            new_decoration: Decoration::Embers,
        });
        commands.entity(*entity).despawn();
    }

    // Pass 3: spawn new fire entities + register light
    for (x, y) in &new_fires {
        if fire_tiles.0.contains(&(*x, *y)) {
            continue;
        }
        spawn_fire(&mut commands, *x, *y, &mut fire_tiles, &mut light_sources);

        let idx = map.xy_idx(*x, *y);
        if map.tiles[idx].decoration.flammability() > 0 {
            decoration_writer.write(DecorationMutationMessage {
                position: Point::new(*x, *y),
                new_decoration: Decoration::None,
            });
        }
        if map.tiles[idx].terrain.flammability() > 0 {
            tile_writer.write(TileMutationMessage {
                position: Point::new(*x, *y),
                new_terrain: TerrainType::Floor,
            });
        }
    }

    // Pass 4: burn creatures standing in fire
    for (pos, mut effects, name) in creature_query.iter_mut() {
        if fire_tiles.0.contains(&(pos.x, pos.y)) && !effects.is_burning() {
            effects.add(
                StatusEffectKind::Burning {
                    damage_per_turn: BURN_DAMAGE,
                },
                BURN_DURATION,
            );
            log_writer.write(GameLogMessage(format!("{} catches fire!", name.0)));
        }
    }

    // Log visible fire spread
    if !new_fires.is_empty() {
        if let Ok(viewshed) = player_query.single() {
            for (x, y) in &new_fires {
                if viewshed.visible_tiles.contains(&Point::new(*x, *y)) {
                    log_writer.write(GameLogMessage("Fire spreads!".to_string()));
                    break;
                }
            }
        }
    }

}

/// Fire light properties.
const FIRE_LIGHT_RADIUS: f32 = 15.0;
const FIRE_LIGHT_INTENSITY: f32 = 1.0;
const FIRE_LIGHT_COLOR: [f32; 3] = [1.0, 0.4, 0.1];

/// Spawn a fire entity and register its light source.
pub fn spawn_fire(
    commands: &mut Commands,
    x: i32,
    y: i32,
    fire_tiles: &mut FireTiles,
    light_sources: &mut LightSources,
) {
    commands.spawn((
        FireMarker,
        FloorEntityMarker,
        GameEntityMarker,
        Position { x, y },
    ));
    fire_tiles.0.insert((x, y));
    light_sources.add(LightSourceData {
        x, y,
        radius: FIRE_LIGHT_RADIUS,
        intensity: FIRE_LIGHT_INTENSITY,
        color: FIRE_LIGHT_COLOR,
        on_wall: false,
    });
}

/// Ignite a tile. Returns true if successfully ignited.
/// Spawns a fire entity + consumes flammable decorations/terrain.
pub fn ignite_tile_at(
    commands: &mut Commands,
    x: i32,
    y: i32,
    map: &Map,
    fire_tiles: &mut FireTiles,
    decoration_writer: &mut MessageWriter<DecorationMutationMessage>,
    tile_writer: &mut MessageWriter<TileMutationMessage>,
    light_sources: &mut LightSources,
    gas_tiles: &mut crate::game::gas::GasTiles,
) -> bool {
    let idx = map.xy_idx(x, y);
    if idx >= map.tiles.len() {
        return false;
    }
    let tile = map.tiles[idx];

    if fire_tiles.0.contains(&(x, y)) {
        return false;
    }
    // Fire + water = steam instead of ignition
    if tile.liquid == LiquidType::Water || tile.liquid == LiquidType::ShallowWater {
        crate::game::gas::spawn_gas(
            commands, x, y,
            crate::game::gas::GasType::Steam,
            crate::game::gas::MAX_CONCENTRATION,
            gas_tiles,
        );
        return false;
    }
    if tile.liquid != LiquidType::None {
        return false; // Lava, chasm — no ignition, no steam
    }

    let flammability = tile
        .decoration
        .flammability()
        .max(tile.terrain.flammability());
    if flammability == 0 {
        return false;
    }

    spawn_fire(commands, x, y, fire_tiles, light_sources);

    if tile.decoration.flammability() > 0 {
        decoration_writer.write(DecorationMutationMessage {
            position: Point::new(x, y),
            new_decoration: Decoration::None,
        });
    }
    if tile.terrain.flammability() > 0 {
        tile_writer.write(TileMutationMessage {
            position: Point::new(x, y),
            new_terrain: TerrainType::Floor,
        });
    }

    true
}

