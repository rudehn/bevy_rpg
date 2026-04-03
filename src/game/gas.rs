//! Gas layer system — tile-based gas clouds that spread, decay, and affect creatures.
//!
//! Gas is entity-based (like fire): `GasMarker` components + `GasTiles` spatial index.
//! Each gas type (`GasType`) defines its own behavior: status effect, color, flammability,
//! FOV blocking, and spread rate. Fungal decorations emit Poison gas; future sources
//! (zombies, lava vents) can emit other types via `GasEmitter`.

use std::collections::HashMap;

use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};
use bracket_lib::random::RandomNumberGenerator;

use crate::components::{FloorEntityMarker, GameEntityMarker, Position, Viewshed};
use crate::game::combat::{ApplyDamageMessage, DamageSource, DamageType};
use crate::game::fire::FireTiles;
use crate::game::magic::{StatusEffectKind, StatusEffects};
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
    /// Status effect applied to creatures standing in this gas.
    /// Concentration-aware: some gas types are harmless at low concentration.
    pub fn on_step_effect(&self, concentration: u8) -> Option<(StatusEffectKind, u32)> {
        match self {
            GasType::Poison => Some((StatusEffectKind::Poisoned { damage_per_turn: 1 }, 3)),
            GasType::Steam => {
                if concentration >= 2 {
                    Some((StatusEffectKind::Burning { damage_per_turn: 2 }, 3))
                } else {
                    None // Thin steam is harmless
                }
            }
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

    /// Minimum concentration to block FOV.
    pub fn fov_block_threshold(&self) -> u8 {
        match self {
            GasType::Poison => 2,
            GasType::Steam => 2,
        }
    }

    /// Base ASCII background color [r, g, b].
    pub fn ascii_bg_color(&self) -> [f32; 3] {
        match self {
            GasType::Poison => [0.05, 0.15, 0.02],
            GasType::Steam => [0.55, 0.55, 0.65],
        }
    }

    /// Chance (0-100) to spread per cardinal neighbor per turn.
    pub fn spread_chance(&self) -> i32 {
        match self {
            GasType::Poison => 30,
            GasType::Steam => 60,
        }
    }

    /// Fire ignition AoE damage, scaled by concentration.
    pub fn ignition_damage(&self, concentration: u8) -> i32 {
        match self {
            GasType::Poison => match concentration {
                3 => 6,
                2 => 4,
                _ => 2,
            },
            GasType::Steam => 0,
        }
    }

    /// Player-facing name for log messages and tooltips.
    pub fn name(&self) -> &'static str {
        match self {
            GasType::Poison => "poisonous gas",
            GasType::Steam => "scalding steam",
        }
    }

    /// Concentration-qualified description for hover info.
    pub fn description(&self, concentration: u8) -> &'static str {
        match self {
            GasType::Poison => match concentration {
                3 => "thick poisonous gas",
                2 => "poisonous gas",
                _ => "thin poisonous gas",
            },
            GasType::Steam => match concentration {
                3 => "thick scalding steam",
                2 => "scalding steam",
                _ => "thin steam",
            },
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
    pub concentration: u8,
}

/// Per-tile gas data stored in the spatial index.
pub struct GasTileData {
    pub gas_type: GasType,
    pub concentration: u8,
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
/// Maximum gas concentration.
pub const MAX_CONCENTRATION: u8 = 3;

// =====================================================================
// Pure Helpers (testable without ECS)
// =====================================================================

/// Decay concentration by 1. Returns None if gas should dissipate.
pub fn decay_concentration(concentration: u8) -> Option<u8> {
    if concentration <= 1 {
        None
    } else {
        Some(concentration - 1)
    }
}

/// Whether gas can exist on this tile (non-wall, non-empty).
pub fn can_gas_occupy(tile: crate::map::tile::Tile) -> bool {
    !matches!(tile.terrain, TerrainType::Wall | TerrainType::Empty)
}

// =====================================================================
// Spawn/Despawn Helpers
// =====================================================================

/// Spawn a gas entity at (x, y) with the given type and concentration.
/// Updates `GasTiles` spatial index. If a gas entity already exists at this position,
/// it is replaced only if the new concentration is higher.
pub fn spawn_gas(
    commands: &mut Commands,
    x: i32,
    y: i32,
    gas_type: GasType,
    concentration: u8,
    gas_tiles: &mut GasTiles,
) {
    // Check existing gas at this position
    if let Some(existing) = gas_tiles.0.get(&(x, y)) {
        if existing.concentration >= concentration {
            return; // Existing gas is denser or equal — don't replace
        }
        // Replace: despawn old entity
        commands.entity(existing.entity).despawn();
    }

    let entity = commands
        .spawn((
            GasMarker {
                gas_type,
                concentration,
            },
            Position { x, y },
            FloorEntityMarker,
            GameEntityMarker,
        ))
        .id();

    gas_tiles.0.insert(
        (x, y),
        GasTileData {
            gas_type,
            concentration,
            entity,
        },
    );
}

/// Despawn a gas entity and remove from spatial index.
fn despawn_gas(commands: &mut Commands, x: i32, y: i32, gas_tiles: &mut GasTiles) {
    if let Some(data) = gas_tiles.0.remove(&(x, y)) {
        commands.entity(data.entity).despawn();
    }
}

// =====================================================================
// Gas Tick System
// =====================================================================

/// Processes gas emission, spread, fire interaction, decay, and creature effects once per turn.
pub fn gas_tick_system(
    mut commands: Commands,
    mut turn_end: MessageReader<TurnEndEvent>,
    mut gas_tiles: ResMut<GasTiles>,
    mut map: ResMut<Map>,
    fire_tiles: Res<FireTiles>,
    mut creature_query: Query<(Entity, &Position, &mut StatusEffects, &crate::components::Name)>,
    mut damage_writer: MessageWriter<ApplyDamageMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut viewshed_query: Query<&mut Viewshed>,
) {
    let count = turn_end.read().count();
    if count == 0 {
        return;
    }

    let mut rng = RandomNumberGenerator::new();

    // Pass 1: Emission — fungus tiles emit gas to adjacent tiles
    let mut emissions: Vec<(i32, i32, GasType)> = Vec::new();
    for y in 0..map.height {
        for x in 0..map.width {
            let idx = map.xy_idx(x, y);
            if map.tiles[idx].decoration != Decoration::Fungus {
                continue;
            }
            if rng.range(0, 100) >= GAS_EMISSION_CHANCE {
                continue;
            }

            // Pick a random adjacent non-wall tile for emission
            let dirs: [(i32, i32); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
            let start = rng.range(0, 4) as usize;
            for i in 0..4 {
                let (dx, dy) = dirs[(start + i) % 4];
                let (nx, ny) = (x + dx, y + dy);
                if !map.in_bounds(Point::new(nx, ny)) {
                    continue;
                }
                let nidx = map.xy_idx(nx, ny);
                if can_gas_occupy(map.tiles[nidx]) {
                    emissions.push((nx, ny, GasType::Poison));
                    break;
                }
            }
        }
    }

    for (x, y, gas_type) in &emissions {
        spawn_gas(&mut commands, *x, *y, *gas_type, MAX_CONCENTRATION, &mut gas_tiles);
    }

    // Pass 2: Spread — existing gas at concentration ≥2 spreads to neighbors
    let current_gas: Vec<(i32, i32, GasType, u8)> = gas_tiles
        .0
        .iter()
        .map(|(&(x, y), data)| (x, y, data.gas_type, data.concentration))
        .collect();

    let mut spreads: Vec<(i32, i32, GasType, u8)> = Vec::new();
    for &(x, y, gas_type, concentration) in &current_gas {
        if concentration < 2 {
            continue;
        }
        let spread_conc = concentration - 1;
        for &(dx, dy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
            let (nx, ny) = (x + dx, y + dy);
            if !map.in_bounds(Point::new(nx, ny)) {
                continue;
            }
            let nidx = map.xy_idx(nx, ny);
            if !can_gas_occupy(map.tiles[nidx]) {
                continue;
            }
            if rng.range(0, 100) >= gas_type.spread_chance() {
                continue;
            }
            // Only spread if neighbor has lower concentration (or no gas)
            let existing_conc = gas_tiles
                .0
                .get(&(nx, ny))
                .map(|d| d.concentration)
                .unwrap_or(0);
            if spread_conc > existing_conc {
                spreads.push((nx, ny, gas_type, spread_conc));
            }
        }
    }

    for (x, y, gas_type, concentration) in spreads {
        spawn_gas(&mut commands, x, y, gas_type, concentration, &mut gas_tiles);
    }

    // Pass 3: Fire interaction — flammable gas on fire tiles ignites
    let ignited: Vec<(i32, i32, GasType, u8)> = gas_tiles
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
                damage_writer.write(ApplyDamageMessage {
                    attacker: entity,
                    target: entity,
                    final_damage: dmg,
                    damage_type: DamageType::Fire,
                    source: DamageSource::Environment,
                });
                log_writer.write(GameLogMessage(format!(
                    "The gas ignites! {} takes {} fire damage!",
                    name.0, dmg
                )));
            }
        }

        despawn_gas(&mut commands, *x, *y, &mut gas_tiles);
    }

    // Pass 4: Decay — all gas loses 1 concentration
    let decay_targets: Vec<(i32, i32, u8, Entity)> = gas_tiles
        .0
        .iter()
        .map(|((x, y), data)| (*x, *y, data.concentration, data.entity))
        .collect();

    let has_decays = !decay_targets.is_empty();
    for (x, y, concentration, entity) in decay_targets {
        match decay_concentration(concentration) {
            None => {
                // Dissipates
                despawn_gas(&mut commands, x, y, &mut gas_tiles);
            }
            Some(new_conc) => {
                // Update concentration
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

    // Pass 5: Poison creatures standing in gas
    for (_, pos, mut effects, name) in creature_query.iter_mut() {
        if let Some(data) = gas_tiles.0.get(&(pos.x, pos.y)) {
            if let Some((effect_kind, duration)) = data.gas_type.on_step_effect(data.concentration) {
                if !data.gas_type.is_immune(&effects) {
                    effects.add(effect_kind, duration);
                    // Only log for newly poisoned (avoid spam)
                    if !effects.is_poisoned() || effects.poison_damage() == Some(1) {
                        log_writer.write(GameLogMessage(format!(
                            "{} inhales {}!",
                            name.0,
                            data.gas_type.name()
                        )));
                    }
                }
            }
        }
    }

    // Pass 6: Update gas_opaque array for FOV blocking
    let gas_changed = !emissions.is_empty()
        || !ignited.is_empty()
        || has_decays;

    for val in map.gas_opaque.iter_mut() {
        *val = false;
    }
    for (&(x, y), data) in gas_tiles.0.iter() {
        if data.concentration >= data.gas_type.fov_block_threshold() {
            let idx = map.xy_idx(x, y);
            if idx < map.gas_opaque.len() {
                map.gas_opaque[idx] = true;
            }
        }
    }

    // Mark viewsheds dirty if gas changed so FOV recalculates
    if gas_changed {
        for mut viewshed in viewshed_query.iter_mut() {
            viewshed.dirty = true;
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
    fn decay_at_one_dissipates() {
        assert_eq!(decay_concentration(1), None);
    }

    #[test]
    fn decay_at_two_becomes_one() {
        assert_eq!(decay_concentration(2), Some(1));
    }

    #[test]
    fn decay_at_three_becomes_two() {
        assert_eq!(decay_concentration(3), Some(2));
    }

    #[test]
    fn decay_at_zero_dissipates() {
        assert_eq!(decay_concentration(0), None);
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
    fn poison_gas_fov_threshold_is_two() {
        assert_eq!(GasType::Poison.fov_block_threshold(), 2);
    }

    #[test]
    fn poison_gas_ignition_damage_scales() {
        assert_eq!(GasType::Poison.ignition_damage(3), 6);
        assert_eq!(GasType::Poison.ignition_damage(2), 4);
        assert_eq!(GasType::Poison.ignition_damage(1), 2);
    }

    #[test]
    fn poison_gas_has_on_step_effect() {
        let effect = GasType::Poison.on_step_effect(1);
        assert!(effect.is_some());
        let (kind, duration) = effect.unwrap();
        assert!(matches!(kind, StatusEffectKind::Poisoned { damage_per_turn: 1 }));
        assert_eq!(duration, 3);
    }

    #[test]
    fn poison_gas_immunity_checks_poison_resistance() {
        let mut effects = StatusEffects::default();
        assert!(!GasType::Poison.is_immune(&effects));

        effects.add(StatusEffectKind::PoisonResistance, 5);
        assert!(GasType::Poison.is_immune(&effects));
    }

    #[test]
    fn poison_gas_immunity_ignores_fire_resistance() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::FireResistance, 5);
        assert!(!GasType::Poison.is_immune(&effects));
    }

    // --- Steam GasType behavior ---

    #[test]
    fn steam_is_not_flammable() {
        assert!(!GasType::Steam.flammable());
    }

    #[test]
    fn steam_no_effect_at_low_concentration() {
        assert!(GasType::Steam.on_step_effect(1).is_none());
    }

    #[test]
    fn steam_burns_at_concentration_two() {
        let effect = GasType::Steam.on_step_effect(2);
        assert!(effect.is_some());
        let (kind, duration) = effect.unwrap();
        assert!(matches!(kind, StatusEffectKind::Burning { damage_per_turn: 2 }));
        assert_eq!(duration, 3);
    }

    #[test]
    fn steam_burns_at_concentration_three() {
        assert!(GasType::Steam.on_step_effect(3).is_some());
    }

    #[test]
    fn steam_immunity_checks_fire_resistance() {
        let mut effects = StatusEffects::default();
        assert!(!GasType::Steam.is_immune(&effects));

        effects.add(StatusEffectKind::FireResistance, 5);
        assert!(GasType::Steam.is_immune(&effects));
    }

    #[test]
    fn steam_immunity_ignores_poison_resistance() {
        let mut effects = StatusEffects::default();
        effects.add(StatusEffectKind::PoisonResistance, 5);
        assert!(!GasType::Steam.is_immune(&effects));
    }

    #[test]
    fn steam_spread_chance_is_60() {
        assert_eq!(GasType::Steam.spread_chance(), 60);
    }

    #[test]
    fn steam_ignition_damage_is_zero() {
        assert_eq!(GasType::Steam.ignition_damage(3), 0);
    }
}
