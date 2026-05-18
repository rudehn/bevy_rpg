//! Brogue-aligned timed tile promotions.
//!
//! Each turn, every map cell with a `timed_promotion()` rule rolls
//! against a 0-10000 chance scale. Door auto-close skips cells with
//! creatures on them and cells that just mutated this turn (the
//! [`PromotionCooldown`] grace).
//!
//! On-step promotions (trample, entangle) live in the game's movement
//! handler — those are not scheduled by this tick.

use std::collections::HashSet;

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use bracket_lib::random::RandomNumberGenerator;

use crate::components::{Collider, Position};
use crate::map::map::Map;
use crate::map::mutation::{
    DecorationMutationMessage, LiquidMutationMessage, MapMutationSet, TileMutationMessage,
};
use crate::map::tile::PromotionTarget;
use crate::turn::TurnEndEvent;

/// Tiles that were mutated this turn skip promotion to avoid same-turn
/// close-after-open for doors (and similar instant-revert issues).
/// Inserted into by the apply systems; cleared each promotion tick.
#[derive(Resource, Default)]
pub struct PromotionCooldown(pub HashSet<(i32, i32)>);

/// Set marker for the promotion tick. Configured by [`TilePromotionPlugin`]
/// to run before [`MapMutationSet`] so this turn's mutations land before
/// the next turn begins.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TilePromotionSet;

pub struct TilePromotionPlugin;

impl Plugin for TilePromotionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PromotionCooldown>()
            .add_systems(
                Update,
                tile_promotion_tick_system
                    .in_set(TilePromotionSet)
                    .before(MapMutationSet),
            );
    }
}

/// Processes timed tile promotions once per batch of [`TurnEndEvent`]s.
/// Uses Brogue's 0-10000 probability scale (100 ≈ 1% per turn,
/// 10000 = 100% per turn).
pub fn tile_promotion_tick_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    map: Res<Map>,
    mut tile_mutation_writer: MessageWriter<TileMutationMessage>,
    mut decoration_mutation_writer: MessageWriter<DecorationMutationMessage>,
    mut liquid_mutation_writer: MessageWriter<LiquidMutationMessage>,
    collider_query: Query<&Position, With<Collider>>,
    mut cooldown: ResMut<PromotionCooldown>,
) {
    // Process at most once per batch of TurnEndEvents.
    if turn_end.read().count() == 0 {
        return;
    }

    let mut rng = RandomNumberGenerator::new();

    // Build a set of occupied positions for door-close blocking.
    let occupied: HashSet<(i32, i32)> = collider_query.iter().map(|p| (p.x, p.y)).collect();

    // Grab and clear the cooldown set — tiles mutated last turn get a one-turn grace.
    let cooled: HashSet<(i32, i32)> = std::mem::take(&mut cooldown.0);

    for y in 0..map.height {
        for x in 0..map.width {
            let idx = map.xy_idx(x, y);
            let tile = map.tiles[idx];

            // Decoration timed promotion (grass regrow, fungus regrow, cracked floor → chasm).
            if let Some(rule) = tile.decoration.timed_promotion() {
                if rng.range(0, 10000) < rule.chance_per_turn as i32 {
                    apply_promotion(
                        &rule.target,
                        Point::new(x, y),
                        &mut tile_mutation_writer,
                        &mut decoration_mutation_writer,
                        &mut liquid_mutation_writer,
                    );
                }
            }

            // Terrain timed promotion (open door auto-close).
            if let Some(rule) = tile.terrain.timed_promotion() {
                if cooled.contains(&(x, y)) {
                    continue;
                }
                if occupied.contains(&(x, y)) {
                    continue;
                }
                if rng.range(0, 10000) < rule.chance_per_turn as i32 {
                    apply_promotion(
                        &rule.target,
                        Point::new(x, y),
                        &mut tile_mutation_writer,
                        &mut decoration_mutation_writer,
                        &mut liquid_mutation_writer,
                    );
                }
            }
        }
    }
}

fn apply_promotion(
    target: &PromotionTarget,
    position: Point,
    tile_writer: &mut MessageWriter<TileMutationMessage>,
    decoration_writer: &mut MessageWriter<DecorationMutationMessage>,
    liquid_writer: &mut MessageWriter<LiquidMutationMessage>,
) {
    match target {
        PromotionTarget::Decoration(d) => {
            decoration_writer.write(DecorationMutationMessage {
                position,
                new_decoration: *d,
            });
        }
        PromotionTarget::Terrain(t) => {
            tile_writer.write(TileMutationMessage {
                position,
                new_terrain: *t,
            });
        }
        PromotionTarget::Liquid(l) => {
            liquid_writer.write(LiquidMutationMessage {
                position,
                new_liquid: *l,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::LightSources;
    use crate::map::mutation::MapMutationPlugin;
    use crate::map::tile::{Decoration, LiquidType, TerrainType, Tile};
    use crate::map::tile_entity_index::TileEntityIndex;

    fn make_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins((MapMutationPlugin, TilePromotionPlugin));
        app.add_message::<TurnEndEvent>();
        app.init_resource::<LightSources>();
        app.init_resource::<TileEntityIndex>();

        let mut map = Map::new(1, 5, 5, "test");
        for y in 1..4 {
            for x in 1..4 {
                let idx = map.xy_idx(x, y);
                map.tiles[idx] = Tile {
                    terrain: TerrainType::Floor,
                    liquid: LiquidType::None,
                    decoration: Decoration::None,
                };
            }
        }
        app.insert_resource(map);
        app
    }

    #[test]
    fn no_turn_end_means_no_tick() {
        let mut app = make_test_app();
        // Place an open door — it has a 100% chance to auto-close per turn.
        {
            let mut map = app.world_mut().resource_mut::<Map>();
            let idx = map.xy_idx(2, 2);
            map.tiles[idx].terrain = TerrainType::OpenDoor;
        }
        app.update();
        let map = app.world().resource::<Map>();
        let idx = map.xy_idx(2, 2);
        // Without a TurnEndEvent, the tick must not run.
        assert_eq!(map.tiles[idx].terrain, TerrainType::OpenDoor);
    }

    #[test]
    fn cooldown_prevents_same_turn_revert() {
        let mut app = make_test_app();
        // Place an open door with cooldown set — promotion must be skipped.
        {
            let mut map = app.world_mut().resource_mut::<Map>();
            let idx = map.xy_idx(2, 2);
            map.tiles[idx].terrain = TerrainType::OpenDoor;
        }
        app.world_mut()
            .resource_mut::<PromotionCooldown>()
            .0
            .insert((2, 2));
        app.world_mut()
            .resource_mut::<Messages<TurnEndEvent>>()
            .write(TurnEndEvent);
        app.update();

        // Cooldown was honoured, terrain unchanged.
        let map = app.world().resource::<Map>();
        let idx = map.xy_idx(2, 2);
        assert_eq!(map.tiles[idx].terrain, TerrainType::OpenDoor);
    }

    #[test]
    fn occupied_door_does_not_close() {
        let mut app = make_test_app();
        {
            let mut map = app.world_mut().resource_mut::<Map>();
            let idx = map.xy_idx(2, 2);
            map.tiles[idx].terrain = TerrainType::OpenDoor;
        }
        // Drop a colliding entity on the door tile.
        app.world_mut().spawn((Position { x: 2, y: 2 }, Collider));
        app.world_mut()
            .resource_mut::<Messages<TurnEndEvent>>()
            .write(TurnEndEvent);
        app.update();

        let map = app.world().resource::<Map>();
        let idx = map.xy_idx(2, 2);
        assert_eq!(
            map.tiles[idx].terrain,
            TerrainType::OpenDoor,
            "door must not close on a creature"
        );
    }
}
