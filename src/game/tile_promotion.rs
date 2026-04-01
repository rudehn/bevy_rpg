//! Tile promotion system — Brogue-aligned timed tile transformations.
//!
//! Each turn, iterates all map tiles and checks for timed promotions:
//! - Trampled grass (Grass decoration) regrows into TallGrass (~1% per turn)
//! - Trampled fungus (DeadGrass decoration) regrows into Fungus (~1% per turn)
//! - Open doors close automatically (100% per turn, blocked by creatures)
//!
//! On-step promotions (trample, entangle) are handled in `handle_movement`.

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use bracket_lib::random::RandomNumberGenerator;

use crate::components::{Collider, Position};
use crate::game::turns::TurnEndEvent;
use crate::map::map::Map;
use crate::map::tile::{DecorationMutationMessage, PromotionTarget, TileMutationMessage};
use crate::ui::game_log::GameLogMessage;

/// Tiles that were mutated this turn skip promotion to avoid same-turn
/// close-after-open for doors (and similar instant-revert issues).
/// Populated by `apply_tile_mutations`; cleared each promotion tick.
#[derive(Resource, Default)]
pub struct PromotionCooldown(pub std::collections::HashSet<(i32, i32)>);

/// Processes timed tile promotions once per turn. Uses Brogue's 0-10000
/// probability scale (100 = ~1% per turn, 10000 = 100% per turn).
pub fn tile_promotion_tick_system(
    mut turn_end: MessageReader<TurnEndEvent>,
    map: Res<Map>,
    mut tile_mutation_writer: MessageWriter<TileMutationMessage>,
    mut decoration_mutation_writer: MessageWriter<DecorationMutationMessage>,
    mut log_writer: MessageWriter<GameLogMessage>,
    collider_query: Query<&Position, With<Collider>>,
    player_query: Query<&crate::components::Viewshed, With<crate::player::Player>>,
    mut cooldown: ResMut<PromotionCooldown>,
) {
    // Process at most once per batch of TurnEndEvents.
    let count = turn_end.read().count();
    if count == 0 {
        return;
    }

    let mut rng = RandomNumberGenerator::new();

    // Build a set of occupied positions for door-close blocking.
    let occupied: std::collections::HashSet<(i32, i32)> =
        collider_query.iter().map(|p| (p.x, p.y)).collect();

    // Grab and clear the cooldown set — tiles mutated last turn get a one-turn grace.
    let cooled: std::collections::HashSet<(i32, i32)> = std::mem::take(&mut cooldown.0);

    for y in 0..map.height {
        for x in 0..map.width {
            let idx = map.xy_idx(x, y);
            let tile = map.tiles[idx];

            // Decoration timed promotion (grass regrow, fungus regrow)
            if let Some(rule) = tile.decoration.timed_promotion() {
                if rng.range(0, 10000) < rule.chance_per_turn as i32 {
                    apply_promotion(
                        &rule.target,
                        Point::new(x, y),
                        &mut tile_mutation_writer,
                        &mut decoration_mutation_writer,
                    );
                }
            }

            // Terrain timed promotion (open door auto-close)
            if let Some(rule) = tile.terrain.timed_promotion() {
                // Skip tiles on cooldown (just mutated this turn — prevents same-turn revert)
                if cooled.contains(&(x, y)) {
                    continue;
                }
                // Don't close doors on creatures
                if occupied.contains(&(x, y)) {
                    continue;
                }
                if rng.range(0, 10000) < rule.chance_per_turn as i32 {
                    apply_promotion(
                        &rule.target,
                        Point::new(x, y),
                        &mut tile_mutation_writer,
                        &mut decoration_mutation_writer,
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
    }
}
