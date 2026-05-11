//! Gas layer system — Brogue-style gas redistribution.
//!
//! Gas is entity-based: `GasMarker` components + `GasTiles` spatial index.
//! Each turn, gas redistributes to cardinal neighbors (sharing concentration),
//! then decays by 10%. This creates natural diffusion — gas fills rooms from
//! a point source, flows through corridors, and dissipates in open spaces.

use std::collections::HashMap;

use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};
use bracket_lib::random::RandomNumberGenerator;

use crate::components::{FloorEntityMarker, GameEntityMarker, Position};
use crate::game::combat::{DamageEvent, DamageSource, DamageType};
use crate::game::fire::FireTiles;
use crate::game::magic::{GameStatusEffectsExt, StatusEffectKind, StatusEffects};
use crate::game::turns::TurnEndEvent;
use crate::map::map::Map;
use crate::map::tile::{Decoration, TerrainType};
use crate::ui::game_log::GameLogMessage;

// =====================================================================
// Gas Type — behavior-carrying enum
// =====================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GasType {
    Poison,
    Steam,
}

impl GasType {
    /// Minimum concentration to cause damage effects.
    const EFFECT_THRESHOLD: u16 = 100;

    /// Status effect applied to creatures standing in this gas.
    /// Returns `(kind, duration, magnitude)` for the effect applied when
    /// a creature stands in gas of this type. `None` if concentration is
    /// below the effect threshold.
    pub fn on_step_effect(&self, concentration: u16) -> Option<(StatusEffectKind, u32, i32)> {
        if concentration < Self::EFFECT_THRESHOLD {
            return None;
        }
        match self {
            GasType::Poison => Some((StatusEffectKind::Poisoned, 3, 1)),
            GasType::Steam => Some((StatusEffectKind::Burning, 3, 2)),
        }
    }

    /// Whether a creature with the given status effects is immune to this gas.
    pub fn is_immune(&self, effects: &StatusEffects) -> bool {
        match self {
            GasType::Poison => effects.is_poison_resistant(),
            GasType::Steam => effects.is_fire_resistant(),
        }
    }

    /// Whether fire ignites this gas type.
    pub fn flammable(&self) -> bool {
        match self {
            GasType::Poison => true,
            GasType::Steam => false,
        }
    }

    /// Base ASCII background color [r, g, b].
    pub fn ascii_bg_color(&self) -> [f32; 3] {
        match self {
            GasType::Poison => [0.05, 0.15, 0.02],
            GasType::Steam => [0.85, 0.85, 0.90],
        }
    }

    /// Fire ignition AoE damage, scaled by concentration.
    pub fn ignition_damage(&self, concentration: u16) -> i32 {
        match self {
            GasType::Poison => ((concentration / 50) as i32).clamp(1, 10),
            GasType::Steam => 0,
        }
    }

    /// Player-facing name for log messages, scaled by concentration.
    pub fn name(&self, concentration: u16) -> &'static str {
        if concentration >= Self::EFFECT_THRESHOLD {
            match self {
                GasType::Poison => "poisonous gas",
                GasType::Steam => "scalding steam",
            }
        } else {
            match self {
                GasType::Poison => "faint gas",
                GasType::Steam => "steam",
            }
        }
    }

    /// Description for hover info, scaled by concentration.
    pub fn description(&self, concentration: u16) -> &'static str {
        if concentration >= Self::EFFECT_THRESHOLD {
            match self {
                GasType::Poison => "poisonous gas",
                GasType::Steam => "scalding steam",
            }
        } else {
            match self {
                GasType::Poison => "faint gas",
                GasType::Steam => "steam",
            }
        }
    }
}

// =====================================================================
// Components & Resources
// =====================================================================

/// Marker for gas entities on the map.
#[derive(Component)]
pub struct GasMarker {
    pub gas_type: GasType,
    pub concentration: u16,
}

/// Per-tile gas data stored in the spatial index.
pub struct GasTileData {
    pub gas_type: GasType,
    pub concentration: u16,
    pub entity: Entity,
}

/// Spatial index of gas tile positions. Updated when gas spawns/despawns/changes.
#[derive(Resource, Default)]
pub struct GasTiles(pub HashMap<(i32, i32), GasTileData>);

// =====================================================================
// Constants
// =====================================================================

/// Chance per turn (out of 100) for each fungus tile to emit gas.
pub const GAS_EMISSION_CHANCE: i32 = 12;

// =====================================================================
// Pure Helpers (testable without ECS)
// =====================================================================

/// Apply 10% decay. Returns None if gas should dissipate (concentration reaches 0).
pub fn decay_concentration(concentration: u16) -> Option<u16> {
    let new = concentration * 9 / 10;
    if new == 0 { None } else { Some(new) }
}

/// Whether gas can exist on this tile (non-wall, non-empty).
pub fn can_gas_occupy(tile: crate::map::tile::Tile) -> bool {
    !matches!(tile.terrain, TerrainType::Wall | TerrainType::Empty)
}

// =====================================================================
// Spawn/Despawn Helpers
// =====================================================================

/// Spawn or add gas at (x, y). If same type exists, concentration is ADDED.
/// If different type exists, they neutralize (both lose the smaller amount).
pub fn spawn_gas(
    commands: &mut Commands,
    x: i32,
    y: i32,
    gas_type: GasType,
    volume: u16,
    gas_tiles: &mut GasTiles,
) {
    if volume == 0 {
        return;
    }

    if let Some(existing) = gas_tiles.0.get_mut(&(x, y)) {
        if existing.gas_type == gas_type {
            // Same type: add concentration
            existing.concentration = existing.concentration.saturating_add(volume);
            if let Ok(mut entity_commands) = commands.get_entity(existing.entity) {
                entity_commands.insert(GasMarker {
                    gas_type,
                    concentration: existing.concentration,
                });
            }
            return;
        } else {
            // Different type: neutralize
            let neutralize = existing.concentration.min(volume);
            existing.concentration -= neutralize;
            let remaining_new = volume - neutralize;
            if existing.concentration == 0 {
                // Old gas fully neutralized — replace with new type if any remains
                if let Ok(mut entity_commands) = commands.get_entity(existing.entity) {
                    entity_commands.despawn();
                }
                gas_tiles.0.remove(&(x, y));
                if remaining_new > 0 {
                    let entity = commands
                        .spawn((
                            GasMarker { gas_type, concentration: remaining_new },
                            Position { x, y },
                            FloorEntityMarker,
                            GameEntityMarker,
                        ))
                        .id();
                    gas_tiles.0.insert((x, y), GasTileData {
                        gas_type, concentration: remaining_new, entity,
                    });
                }
            } else {
                // Old gas survived — update its concentration
                commands.entity(existing.entity).insert(GasMarker {
                    gas_type: existing.gas_type,
                    concentration: existing.concentration,
                });
            }
            return;
        }
    }

    // No existing gas — spawn new
    let entity = commands
        .spawn((
            GasMarker { gas_type, concentration: volume },
            Position { x, y },
            FloorEntityMarker,
            GameEntityMarker,
        ))
        .id();
    gas_tiles.0.insert((x, y), GasTileData {
        gas_type, concentration: volume, entity,
    });
}

/// Despawn a gas entity and remove from spatial index.
fn despawn_gas(commands: &mut Commands, x: i32, y: i32, gas_tiles: &mut GasTiles) {
    if let Some(data) = gas_tiles.0.remove(&(x, y)) {
        if let Ok(mut entity_commands) = commands.get_entity(data.entity) {
            entity_commands.despawn();
        }
    }
}

// =====================================================================
// Gas Tick System
// =====================================================================

/// Processes gas emission, redistribution, fire interaction, decay, and creature effects once per turn.
pub fn gas_tick_system(
    mut commands: Commands,
    mut turn_end: MessageReader<TurnEndEvent>,
    mut gas_tiles: ResMut<GasTiles>,
    map: Res<Map>,
    fire_tiles: Res<FireTiles>,
    mut creature_query: Query<(Entity, &Position, &mut StatusEffects, &crate::components::Name)>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    let count = turn_end.read().count();
    if count == 0 {
        return;
    }

    let mut rng = RandomNumberGenerator::new();

    // Pass 1: Emission — fungus tiles emit gas on their own tile
    let mut emissions: Vec<(i32, i32, GasType, u16)> = Vec::new();
    for y in 0..map.height {
        for x in 0..map.width {
            let idx = map.xy_idx(x, y);
            if map.tiles[idx].decoration != Decoration::Fungus {
                continue;
            }
            if rng.range(0, 100) >= GAS_EMISSION_CHANCE {
                continue;
            }
            // Emit on the fungus tile itself — redistribution spreads it
            emissions.push((x, y, GasType::Poison, 200));
        }
    }

    for (x, y, gas_type, volume) in &emissions {
        spawn_gas(&mut commands, *x, *y, *gas_type, *volume, &mut gas_tiles);
    }

    // Pass 2: Redistribution — each gas tile shares concentration with neighbors
    // Snapshot current state, then compute new state.
    let snapshot: Vec<((i32, i32), GasType, u16)> = gas_tiles
        .0
        .iter()
        .map(|(&pos, data)| (pos, data.gas_type, data.concentration))
        .collect();

    // Accumulate new concentrations per tile
    let mut new_state: HashMap<(i32, i32), (GasType, u16)> = HashMap::new();

    for &((x, y), gas_type, concentration) in &snapshot {
        let share = concentration / 5;
        if share == 0 {
            // Too thin to redistribute — keep in place
            let entry = new_state.entry((x, y)).or_insert((gas_type, 0));
            if entry.0 == gas_type {
                entry.1 = entry.1.saturating_add(concentration);
            }
            continue;
        }

        // Keep 1 share for self
        let entry = new_state.entry((x, y)).or_insert((gas_type, 0));
        if entry.0 == gas_type {
            entry.1 = entry.1.saturating_add(share);
        }

        // Send 1 share to each cardinal neighbor (if passable)
        for &(dx, dy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
            let (nx, ny) = (x + dx, y + dy);
            if !map.in_bounds(Point::new(nx, ny)) {
                continue;
            }
            let nidx = map.xy_idx(nx, ny);
            if !can_gas_occupy(map.tiles[nidx]) {
                continue; // share lost to wall
            }
            let entry = new_state.entry((nx, ny)).or_insert((gas_type, 0));
            if entry.0 == gas_type {
                entry.1 = entry.1.saturating_add(share);
            }
            // Different gas type at neighbor: neutralize (don't add, both gas types
            // cancel out naturally since each only contributes to its own type)
        }
    }

    // Apply new state: despawn old entities, spawn new ones
    // First, despawn all existing gas entities
    let old_positions: Vec<(i32, i32)> = gas_tiles.0.keys().copied().collect();
    for (x, y) in old_positions {
        despawn_gas(&mut commands, x, y, &mut gas_tiles);
    }

    // Spawn new gas from accumulated state
    for ((x, y), (gas_type, concentration)) in &new_state {
        if *concentration > 0 {
            let entity = commands
                .spawn((
                    GasMarker { gas_type: *gas_type, concentration: *concentration },
                    Position { x: *x, y: *y },
                    FloorEntityMarker,
                    GameEntityMarker,
                ))
                .id();
            gas_tiles.0.insert((*x, *y), GasTileData {
                gas_type: *gas_type,
                concentration: *concentration,
                entity,
            });
        }
    }

    // Pass 3: Fire interaction — flammable gas on fire tiles ignites
    let ignited: Vec<(i32, i32, GasType, u16)> = gas_tiles
        .0
        .iter()
        .filter(|((x, y), data)| data.gas_type.flammable() && fire_tiles.0.contains(&(*x, *y)))
        .map(|((x, y), data)| (*x, *y, data.gas_type, data.concentration))
        .collect();

    for (x, y, gas_type, concentration) in &ignited {
        let dmg = gas_type.ignition_damage(*concentration);

        // AoE fire damage in 3x3 area
        for (entity, pos, _, name) in creature_query.iter() {
            let dx = (pos.x - x).abs();
            let dy = (pos.y - y).abs();
            if dx <= 1 && dy <= 1 {
                damage_writer.write(DamageEvent {
                    attacker: None,
                    target: entity,
                    amount: dmg,
                    damage_type: DamageType::Fire,
                    source: DamageSource::Environment,
                    armor: 0,
                });
                log_writer.write(GameLogMessage(format!(
                    "The gas ignites! {} takes {} fire damage!",
                    name.0, dmg
                )));
            }
        }

        despawn_gas(&mut commands, *x, *y, &mut gas_tiles);
    }

    // Pass 4: Decay — all gas loses 10% concentration
    let decay_targets: Vec<(i32, i32, u16, Entity)> = gas_tiles
        .0
        .iter()
        .map(|((x, y), data)| (*x, *y, data.concentration, data.entity))
        .collect();

    for (x, y, concentration, entity) in decay_targets {
        match decay_concentration(concentration) {
            None => {
                despawn_gas(&mut commands, x, y, &mut gas_tiles);
            }
            Some(new_conc) => {
                if let Some(data) = gas_tiles.0.get_mut(&(x, y)) {
                    data.concentration = new_conc;
                }
                commands.entity(entity).insert(GasMarker {
                    gas_type: gas_tiles
                        .0
                        .get(&(x, y))
                        .map(|d| d.gas_type)
                        .unwrap_or(GasType::Poison),
                    concentration: new_conc,
                });
            }
        }
    }

    // Pass 5: Affect creatures standing in gas
    for (_, pos, mut effects, name) in creature_query.iter_mut() {
        if let Some(data) = gas_tiles.0.get(&(pos.x, pos.y)) {
            if let Some((effect_kind, duration, magnitude)) = data.gas_type.on_step_effect(data.concentration) {
                if !data.gas_type.is_immune(&effects) {
                    effects.add_effect_with_magnitude(effect_kind, duration, magnitude, None);
                    log_writer.write(GameLogMessage(format!(
                        "{} inhales {}!",
                        name.0,
                        data.gas_type.name(data.concentration)
                    )));
                }
            }
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- decay_concentration ---

    #[test]
    fn decay_100_becomes_90() {
        assert_eq!(decay_concentration(100), Some(90));
    }

    #[test]
    fn decay_10_becomes_9() {
        assert_eq!(decay_concentration(10), Some(9));
    }

    #[test]
    fn decay_1_dissipates() {
        // 1 * 9 / 10 = 0 in integer math
        assert_eq!(decay_concentration(1), None);
    }

    #[test]
    fn decay_0_dissipates() {
        assert_eq!(decay_concentration(0), None);
    }

    #[test]
    fn decay_500_becomes_450() {
        assert_eq!(decay_concentration(500), Some(450));
    }

    // --- can_gas_occupy ---

    #[test]
    fn gas_cannot_occupy_wall() {
        use crate::map::tile::{Tile, LiquidType};
        let tile = Tile {
            terrain: TerrainType::Wall,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        };
        assert!(!can_gas_occupy(tile));
    }

    #[test]
    fn gas_can_occupy_floor() {
        use crate::map::tile::{Tile, LiquidType};
        let tile = Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        };
        assert!(can_gas_occupy(tile));
    }

    #[test]
    fn gas_can_occupy_floor_with_water() {
        use crate::map::tile::{Tile, LiquidType};
        let tile = Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::Water,
            decoration: Decoration::None,
        };
        assert!(can_gas_occupy(tile));
    }

    #[test]
    fn gas_cannot_occupy_empty() {
        use crate::map::tile::{Tile, LiquidType};
        let tile = Tile {
            terrain: TerrainType::Empty,
            liquid: LiquidType::None,
            decoration: Decoration::None,
        };
        assert!(!can_gas_occupy(tile));
    }

    // --- GasType behavior ---

    #[test]
    fn poison_gas_is_flammable() {
        assert!(GasType::Poison.flammable());
    }

    #[test]
    fn poison_gas_no_effect_below_threshold() {
        assert!(GasType::Poison.on_step_effect(50).is_none());
    }

    #[test]
    fn poison_gas_has_on_step_effect_above_threshold() {
        let effect = GasType::Poison.on_step_effect(150);
        assert!(effect.is_some());
        let (kind, duration, magnitude) = effect.unwrap();
        assert_eq!(kind, StatusEffectKind::Poisoned);
        assert_eq!(duration, 3);
        assert_eq!(magnitude, 1);
    }

    #[test]
    fn poison_gas_immunity_checks_poison_resistance() {
        let mut effects = StatusEffects::default();
        assert!(!GasType::Poison.is_immune(&effects));

        effects.add_effect(
            StatusEffectKind::Custom { id: crate::game::magic::STATUS_POISON_RESISTANCE },
            5,
        );
        assert!(GasType::Poison.is_immune(&effects));
    }

    #[test]
    fn poison_gas_immunity_ignores_fire_resistance() {
        let mut effects = StatusEffects::default();
        effects.add_effect(
            StatusEffectKind::Custom { id: crate::game::magic::STATUS_FIRE_RESISTANCE },
            5,
        );
        assert!(!GasType::Poison.is_immune(&effects));
    }

    // --- Steam GasType behavior ---

    #[test]
    fn steam_is_not_flammable() {
        assert!(!GasType::Steam.flammable());
    }

    #[test]
    fn steam_no_effect_below_threshold() {
        assert!(GasType::Steam.on_step_effect(50).is_none());
    }

    #[test]
    fn steam_has_on_step_effect_above_threshold() {
        let effect = GasType::Steam.on_step_effect(150);
        assert!(effect.is_some());
        let (kind, duration, magnitude) = effect.unwrap();
        assert_eq!(kind, StatusEffectKind::Burning);
        assert_eq!(duration, 3);
        assert_eq!(magnitude, 2);
    }

    #[test]
    fn steam_immunity_checks_fire_resistance() {
        let mut effects = StatusEffects::default();
        assert!(!GasType::Steam.is_immune(&effects));

        effects.add_effect(
            StatusEffectKind::Custom { id: crate::game::magic::STATUS_FIRE_RESISTANCE },
            5,
        );
        assert!(GasType::Steam.is_immune(&effects));
    }

    #[test]
    fn steam_immunity_ignores_poison_resistance() {
        let mut effects = StatusEffects::default();
        effects.add_effect(
            StatusEffectKind::Custom { id: crate::game::magic::STATUS_POISON_RESISTANCE },
            5,
        );
        assert!(!GasType::Steam.is_immune(&effects));
    }

    #[test]
    fn steam_ignition_damage_is_zero() {
        assert_eq!(GasType::Steam.ignition_damage(500), 0);
    }

    #[test]
    fn poison_ignition_damage_scales() {
        assert_eq!(GasType::Poison.ignition_damage(50), 1);
        assert_eq!(GasType::Poison.ignition_damage(200), 4);
        assert_eq!(GasType::Poison.ignition_damage(500), 10);
        assert_eq!(GasType::Poison.ignition_damage(1000), 10); // clamped
    }
}
