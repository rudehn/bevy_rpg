use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::{Faction, GameEntityMarker, Monster, Position, Viewshed},
    game::{
        actions::Action,
        actions::PendingPlayerAction,
        combat::Health,
        factions::FactionMatrix,
        turns::TurnState,
        InGameState,
    },
    map::{Map, tile::is_walkable},
    player::Player,
    ui::game_log::{GameLog, GameLogMessage},
};

// --- Resources ---

/// What triggered the targeting mode.
#[derive(Clone, Debug, PartialEq)]
pub enum TargetingMode {
    /// Player is selecting an enemy target for a spell slot.
    Spell { slot: usize },
    /// Player is selecting an allied target for a spell slot.
    /// `include_self`: if true, the player can also target themselves (AllyOrSelf).
    SpellAlly { slot: usize, include_self: bool },
    /// Player is selecting a tile for a spell (blink, AoE).
    /// `range`: max Manhattan distance from caster. `radius`: AoE blast radius (0 for blink).
    Tile { slot: usize, range: i32, radius: i32 },
    /// Player is selecting a target for a ranged weapon attack.
    RangedAttack,
    /// Player is selecting an enemy target for a staff zap.
    Staff { staff_entity: Entity },
}

impl Default for TargetingMode {
    fn default() -> Self {
        TargetingMode::Spell { slot: 0 }
    }
}

/// Tracks what triggered targeting and where the cursor currently is.
#[derive(Resource)]
pub struct TargetingContext {
    pub mode: TargetingMode,
    pub cursor: Position,
    /// Staff entity being zapped (set by handle_use_item, consumed by targeting confirm).
    pub staff_entity: Option<Entity>,
}

impl Default for TargetingContext {
    fn default() -> Self {
        Self {
            mode: TargetingMode::default(),
            cursor: Position { x: 0, y: 0 },
            staff_entity: None,
        }
    }
}

// Spell targeting decision removed — spell system replaced by monster abilities.

// --- Pure Helpers ---
//
// Pure geometry helpers now live in `roguelike_engine::geometry` and are
// re-exported here so game code can continue to import them from their
// original module path.
pub use roguelike_engine::geometry::{
    chebyshev_distance, clamp_cursor, is_adjacent, manhattan_distance, tiles_in_aoe,
};

/// Returns true when `target` is within `max_distance` (Manhattan) of `origin`.
/// A `max_distance` of 0 means self-only (only the origin tile itself).
///
/// This wrapper takes the game's `Position` type; the underlying distance
/// calculation is the engine's [`manhattan_distance`].
pub fn is_in_range(origin: &Position, target: &Position, max_distance: i32) -> bool {
    manhattan_distance(origin.x, origin.y, target.x, target.y) <= max_distance
}

// --- Components ---

/// Marker component for the targeting cursor entity.
#[derive(Component)]
pub struct SpellCursor;

// --- Systems ---

/// Spawns the targeting cursor when entering Targeting state.
fn setup_targeting(
    mut commands: Commands,
    ctx: Res<TargetingContext>,
    player_query: Query<(&Position, &Viewshed, &Faction), With<Player>>,
    monsters: Query<&Position, With<Monster>>,
    allies_query: Query<(&Position, &Faction, &Health), Without<Player>>,
    mut game_log: ResMut<GameLog>,
    faction_matrix: Res<FactionMatrix>,
) {
    let Ok((player_pos, viewshed, player_faction)) = player_query.single() else {
        return;
    };

    let initial_pos = match &ctx.mode {
        TargetingMode::SpellAlly { include_self, .. } => {
            // Snap to the most-wounded visible ally, or self.
            let best_ally = allies_query
                .iter()
                .filter(|(pos, faction, health)| {
                    faction_matrix.is_allied_to(&faction.0.0, &player_faction.0.0)
                        && health.current < health.max
                        && viewshed.visible_tiles.contains(&Point::new(pos.x, pos.y))
                })
                .max_by_key(|(_, _, health)| health.max - health.current)
                .map(|(pos, _, _)| *pos);

            if *include_self && best_ally.is_none() {
                *player_pos
            } else {
                best_ally.unwrap_or(*player_pos)
            }
        }
        TargetingMode::Tile { .. } => {
            // Tile targeting: start cursor at player position.
            *player_pos
        }
        _ => {
            // Enemy targeting: snap to nearest visible monster.
            monsters
                .iter()
                .filter(|mpos| viewshed.visible_tiles.contains(&Point::new(mpos.x, mpos.y)))
                .min_by_key(|mpos| (mpos.x - player_pos.x).pow(2) + (mpos.y - player_pos.y).pow(2))
                .copied()
                .unwrap_or(*player_pos)
        }
    };

    let cursor_color = match &ctx.mode {
        TargetingMode::SpellAlly { .. } => Color::srgba(0.2, 1.0, 0.2, 0.4), // Green for ally
        TargetingMode::Tile { .. } => Color::srgba(0.4, 0.8, 1.0, 0.4), // Blue for tile
        _ => Color::srgba(1.0, 1.0, 0.0, 0.4), // Yellow for enemy/ranged
    };

    commands.spawn((
        initial_pos,
        Transform::from_xyz(0.0, 0.0, 5.0),
        Sprite {
            color: cursor_color,
            custom_size: Some(Vec2::splat(16.0)),
            ..default()
        },
        Visibility::Visible,
        RenderLayers::layer(1),
        GameEntityMarker,
        SpellCursor,
    ));

    let prompt = match &ctx.mode {
        TargetingMode::SpellAlly { .. } => "Choose ally: arrows to move, Enter to confirm, Esc to cancel.",
        TargetingMode::Tile { .. } => "Choose tile: arrows to move, Enter to confirm, Esc to cancel.",
        _ => "Choose target: arrows to move, Enter to confirm, Esc to cancel.",
    };
    game_log.status_message = Some(prompt.to_string());
}

/// Syncs `TargetingContext.cursor` from the spawned `SpellCursor` entity's position.
/// This runs after setup so the input system always has the current position.
fn sync_cursor_to_context(
    mut ctx: ResMut<TargetingContext>,
    cursor_query: Query<&Position, With<SpellCursor>>,
) {
    if let Ok(pos) = cursor_query.single() {
        ctx.cursor = *pos;
    }
}

/// Despawns the targeting cursor and clears the ephemeral status message.
fn teardown_targeting(
    mut commands: Commands,
    cursor_query: Query<Entity, With<SpellCursor>>,
    mut game_log: ResMut<GameLog>,
) {
    for entity in cursor_query.iter() {
        commands.entity(entity).despawn();
    }
    game_log.status_message = None;
}

/// Handles input while in Targeting state.
fn handle_targeting_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut ctx: ResMut<TargetingContext>,
    mut cursor_query: Query<&mut Position, With<SpellCursor>>,
    monsters: Query<(Entity, &Position), (With<Monster>, Without<SpellCursor>, Without<Player>)>,
    faction_entities: Query<(Entity, &Position, &Faction), (Without<SpellCursor>, Without<Player>)>,
    player_query: Query<(Entity, &Position, &Faction), (With<Player>, Without<SpellCursor>)>,
    map: Res<Map>,
    mut pending: ResMut<PendingPlayerAction>,
    mut next_turn_state: ResMut<NextState<TurnState>>,
    mut next_ingame_state: ResMut<NextState<InGameState>>,
    mut log_writer: MessageWriter<GameLogMessage>,
    faction_matrix: Res<FactionMatrix>,
) {
    let Ok(mut cursor_pos) = cursor_query.single_mut() else {
        return;
    };

    let mut dx = 0i32;
    let mut dy = 0i32;

    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) {
        dy = 1;
    }
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) {
        dy = -1;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) {
        dx = -1;
    }
    if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD) {
        dx = 1;
    }

    if dx != 0 || dy != 0 {
        let new_x = cursor_pos.x + dx;
        let new_y = cursor_pos.y + dy;
        if map.in_bounds(Point::new(new_x, new_y)) {
            cursor_pos.x = new_x;
            cursor_pos.y = new_y;
            ctx.cursor = *cursor_pos;
        }
        return; // Don't process confirm/cancel on same frame as movement.
    }

    // Confirm targeting.
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        let target_pos = *cursor_pos;

        match &ctx.mode {
            TargetingMode::Spell { slot } => {
                // Enemy targeting: find a monster at cursor.
                let found = monsters
                    .iter()
                    .find(|(_, mpos)| mpos.x == target_pos.x && mpos.y == target_pos.y)
                    .map(|(e, _)| e);

                if found.is_some() {
                    // Spell targeting removed — spell system replaced by monster abilities.
                    log_writer.write(GameLogMessage("Spell targeting is no longer available.".to_string()));
                } else {
                    log_writer.write(GameLogMessage("No valid target at cursor.".to_string()));
                }
            }
            TargetingMode::SpellAlly { .. } => {
                // Spell ally targeting removed — spell system replaced by monster abilities.
                log_writer.write(GameLogMessage("Spell targeting is no longer available.".to_string()));
            }
            TargetingMode::Tile { slot, range, radius } => {
                // Tile targeting: validate tile within range.
                let player_pos = player_query.single().ok().map(|(_, p, _)| *p);
                let in_range = player_pos
                    .map(|pp| is_in_range(&pp, &target_pos, *range))
                    .unwrap_or(false);

                let idx = map.xy_idx(target_pos.x, target_pos.y);
                let is_aoe = *radius > 0;

                // AoE staves (fire) can target any in-bounds tile.
                // Blink (radius=0) requires walkable + unoccupied.
                let walkable = is_aoe || is_walkable(map.tiles[idx]);
                let occupied = if is_aoe {
                    false // AoE doesn't care about occupancy
                } else {
                    faction_entities.iter().any(|(_, pos, _)| {
                        pos.x == target_pos.x && pos.y == target_pos.y
                    }) || player_pos.map(|pp| pp.x == target_pos.x && pp.y == target_pos.y).unwrap_or(false)
                };

                if in_range && walkable && !occupied {
                    let player_entity = player_query.single().ok().map(|(e, _, _)| e);
                    // Check if this is a staff-zap tile targeting (blinking)
                    if let Some(staff_entity) = ctx.staff_entity {
                        if let Some(pe) = player_entity {
                            pending.0 = Some(Action::ZapStaff {
                                staff_entity,
                                target: pe,
                                target_pos: Some((target_pos.x, target_pos.y)),
                            });
                        }
                    } else {
                        // Spell tile targeting removed — spell system replaced by monster abilities.
                        log_writer.write(GameLogMessage("Spell targeting is no longer available.".to_string()));
                    }
                    next_turn_state.set(TurnState::Processing);
                    next_ingame_state.set(InGameState::Running);
                } else if !in_range {
                    log_writer.write(GameLogMessage("Target is out of range.".to_string()));
                } else if !walkable {
                    log_writer.write(GameLogMessage("Can't target that tile.".to_string()));
                } else {
                    log_writer.write(GameLogMessage("That tile is occupied.".to_string()));
                }
            }
            TargetingMode::RangedAttack => {
                let found = monsters
                    .iter()
                    .find(|(_, mpos)| mpos.x == target_pos.x && mpos.y == target_pos.y)
                    .map(|(e, _)| e);

                if let Some(entity) = found {
                    pending.0 = Some(Action::RangedAttack { target: entity });
                    next_turn_state.set(TurnState::Processing);
                    next_ingame_state.set(InGameState::Running);
                } else {
                    log_writer.write(GameLogMessage("No valid target at cursor.".to_string()));
                }
            }
            TargetingMode::Staff { staff_entity } => {
                let found = monsters
                    .iter()
                    .find(|(_, mpos)| mpos.x == target_pos.x && mpos.y == target_pos.y)
                    .map(|(e, _)| e);

                if let Some(entity) = found {
                    pending.0 = Some(Action::ZapStaff {
                        staff_entity: *staff_entity,
                        target: entity,
                        target_pos: Some((target_pos.x, target_pos.y)),
                    });
                    next_turn_state.set(TurnState::Processing);
                    next_ingame_state.set(InGameState::Running);
                } else {
                    log_writer.write(GameLogMessage("No valid target at cursor.".to_string()));
                }
            }
        }
        return;
    }

    // Cancel targeting — player stays in TurnState::PlayerInput.
    if keys.just_pressed(KeyCode::Escape) {
        next_ingame_state.set(InGameState::Running);
    }
}

// --- Plugin ---

pub struct TargetingPlugin;

impl Plugin for TargetingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TargetingContext>()
            .add_systems(OnEnter(InGameState::Targeting), setup_targeting)
            .add_systems(OnExit(InGameState::Targeting), teardown_targeting)
            .add_systems(
                Update,
                (
                    sync_cursor_to_context,
                    handle_targeting_input,
                )
                    .chain()
                    .run_if(in_state(InGameState::Targeting)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- helpers ---

    fn pos(x: i32, y: i32) -> Position {
        Position { x, y }
    }

    // Pure distance/adjacency/clamp/AoE tests now live in
    // `roguelike_engine::geometry::tests`. Only game-side wrappers
    // (is_in_range takes &Position) and stateful targeting tests
    // remain in this module.

    // ========================
    // Range validation
    // ========================

    #[test]
    fn in_range_within() {
        let origin = pos(10, 10);
        let target = pos(12, 11); // Manhattan = 3
        assert!(is_in_range(&origin, &target, 5));
    }

    #[test]
    fn in_range_beyond() {
        let origin = pos(10, 10);
        let target = pos(20, 10); // Manhattan = 10
        assert!(!is_in_range(&origin, &target, 5));
    }

    #[test]
    fn in_range_exact_boundary() {
        let origin = pos(0, 0);
        let target = pos(3, 5); // Manhattan = 8
        assert!(is_in_range(&origin, &target, 8));
        assert!(!is_in_range(&origin, &target, 7));
    }

    #[test]
    fn in_range_same_tile() {
        let p = pos(5, 5);
        assert!(is_in_range(&p, &p, 0));
    }

    #[test]
    fn in_range_zero_distance_rejects_neighbour() {
        let origin = pos(5, 5);
        let target = pos(5, 6); // Manhattan = 1
        assert!(!is_in_range(&origin, &target, 0));
    }

    // ========================
    // Self-targeting (range 0)
    // ========================

    #[test]
    fn self_target_only_accepts_self() {
        let p = pos(40, 30);
        // Self-targeting staves (like Healing) have range 0.
        assert!(is_in_range(&p, &p, 0));
        // Any neighbour is out of range.
        assert!(!is_in_range(&p, &pos(40, 31), 0));
        assert!(!is_in_range(&p, &pos(41, 30), 0));
        assert!(!is_in_range(&p, &pos(41, 31), 0));
    }

    // Adjacency tests also live in `roguelike_engine::geometry::tests`;
    // the `is_adjacent` import here is the re-export, so the coverage
    // is already owned by the engine crate.

    // ========================
    // TargetingContext defaults
    // ========================

    #[test]
    fn targeting_context_default_cursor_at_origin() {
        let ctx = TargetingContext::default();
        assert_eq!(ctx.cursor, pos(0, 0));
        assert!(ctx.staff_entity.is_none());
    }

    #[test]
    fn targeting_mode_default_is_spell_slot_zero() {
        assert_eq!(TargetingMode::default(), TargetingMode::Spell { slot: 0 });
    }
}
