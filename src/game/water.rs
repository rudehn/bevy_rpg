use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    components::{Drifting, FloorEntityMarker, InInventory, Inventory, Name, Position},
    game::{
        combat::GameRng,
        magic::{StatusEffectKind, StatusEffects},
        turns::TurnEndEvent,
        AppState,
    },
    map::{tile::{LiquidType, is_walkable}, Map},
    ui::game_log::GameLogMessage,
};

// --- Water shimmer resources ---

/// Spatial index of water tile positions and their liquid type.
/// Populated during floor materialization; used by the shimmer animation system.
#[derive(Resource, Default)]
pub struct WaterTiles(pub HashMap<(i32, i32), LiquidType>);

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                deep_water_item_sweep_system,
                item_drift_system.after(deep_water_item_sweep_system),
                water_extinguish_system,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Sweep items out of inventories of entities standing in deep water.
/// Each item has a 50% chance per turn to be swept away.
fn deep_water_item_sweep_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut actors: Query<(&Position, &mut Inventory, &Name)>,
    item_names: Query<&Name>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    mut log_writer: MessageWriter<GameLogMessage>,
    map: Res<Map>,
) {
    // Consume all turn events but only sweep once per frame to prevent
    // items being ejected multiple times when many turns resolve at once.
    if turn_end.read().count() == 0 {
        return;
    }

    for (pos, mut inventory, _actor_name) in actors.iter_mut() {
        let idx = map.xy_idx(pos.x, pos.y);
        if idx >= map.tiles.len() || map.tiles[idx].liquid != LiquidType::Water {
            continue;
        }

        // Iterate in reverse to avoid index invalidation when removing
        let mut i = inventory.items.len();
        while i > 0 {
            i -= 1;
            let roll = game_rng.0.range(0, 100);
            if roll < 50 {
                let item_entity = inventory.items.remove(i);
                let item_name = item_names
                    .get(item_entity)
                    .map(|n| n.0.clone())
                    .unwrap_or_else(|_| "item".to_string());

                commands.entity(item_entity)
                    .remove::<InInventory>()
                    .insert(Position { x: pos.x, y: pos.y })
                    .insert(FloorEntityMarker)
                    .insert(Drifting)
                    .insert(Visibility::Visible);

                log_writer.write(GameLogMessage(format!(
                    "Your {} is swept away by the current!",
                    item_name
                )));
            }
        }
    }
}

/// Drift items with the `Drifting` component 1 tile per turn.
/// Only drifts once per frame regardless of how many turn events fired.
/// If they reach a non-deep-water tile, they stop drifting.
fn item_drift_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    mut drifting_items: Query<(Entity, &mut Position), With<Drifting>>,
    mut commands: Commands,
    mut game_rng: ResMut<GameRng>,
    map: Res<Map>,
) {
    const OFFSETS: [(i32, i32); 8] = [
        (0, 1), (0, -1), (1, 0), (-1, 0),
        (1, 1), (1, -1), (-1, 1), (-1, -1),
    ];

    // Consume all turn events but only drift once per frame to prevent
    // items teleporting across the map when many turns resolve at once.
    if turn_end.read().count() == 0 {
        return;
    }

    for (entity, mut pos) in drifting_items.iter_mut() {
        // Collect walkable adjacent tiles
        let mut candidates: Vec<(i32, i32, bool)> = Vec::new();
        for &(dx, dy) in &OFFSETS {
            let nx = pos.x + dx;
            let ny = pos.y + dy;
            if nx < 0 || ny < 0 || nx >= map.width || ny >= map.height {
                continue;
            }
            let idx = map.xy_idx(nx, ny);
            if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                let is_deep = map.tiles[idx].liquid == LiquidType::Water;
                candidates.push((nx, ny, is_deep));
            }
        }

        if candidates.is_empty() {
            // Stuck — stop drifting
            commands.entity(entity).remove::<Drifting>();
        } else {
            let pick = game_rng.0.range(0, candidates.len() as i32) as usize;
            let (nx, ny, is_deep) = candidates[pick];
            pos.x = nx;
            pos.y = ny;
            if !is_deep {
                // Washed ashore
                commands.entity(entity).remove::<Drifting>();
            }
        }
    }
}

/// Extinguish burning status on entities standing in water.
fn water_extinguish_system(
    mut commands: Commands,
    mut turn_end: MessageReader<TurnEndEvent>,
    mut query: Query<(&Position, &mut StatusEffects)>,
    mut log_writer: MessageWriter<GameLogMessage>,
    map: Res<Map>,
    mut gas_tiles: ResMut<crate::game::gas::GasTiles>,
) {
    for _ in turn_end.read() {
        for (pos, mut effects) in query.iter_mut() {
            let idx = map.xy_idx(pos.x, pos.y);
            if idx >= map.tiles.len() {
                continue;
            }
            let liquid = map.tiles[idx].liquid;
            if liquid != LiquidType::Water && liquid != LiquidType::ShallowWater {
                continue;
            }
            let had_burning = effects.0.iter().any(|e| matches!(e.kind, StatusEffectKind::Burning { .. }));
            if had_burning {
                effects.remove_kind(|k| matches!(k, StatusEffectKind::Burning { .. }));
                log_writer.write(GameLogMessage("The water extinguishes the flames!".to_string()));
                // Burning creature extinguished by water → dramatic steam burst
                crate::game::gas::spawn_gas(
                    &mut commands, pos.x, pos.y,
                    crate::game::gas::GasType::Steam,
                    crate::game::gas::MAX_CONCENTRATION,
                    &mut gas_tiles,
                );
            }
        }
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn test_phase_offset_varies_by_position() {
        // Different positions should produce different phase values
        let phase_a = (0.0_f32 * 1.7 + 0.0_f32 * 2.3).fract();
        let phase_b = (1.0_f32 * 1.7 + 0.0_f32 * 2.3).fract();
        let phase_c = (0.0_f32 * 1.7 + 1.0_f32 * 2.3).fract();
        assert_ne!(phase_a, phase_b);
        assert_ne!(phase_a, phase_c);
        assert_ne!(phase_b, phase_c);
    }
}
