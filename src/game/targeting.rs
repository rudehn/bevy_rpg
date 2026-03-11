use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, Point};

use crate::{
    components::{GameEntityMarker, Monster, Position, Viewshed},
    game::{
        actions::Action,
        turns::{TurnManager, TurnState},
        InGameState,
    },
    map::Map,
    player::Player,
    ui::game_log::GameLogMessage,
};

// --- Resources ---

/// Tracks which spell slot triggered targeting and where the cursor is.
#[derive(Resource)]
pub struct TargetingContext {
    pub slot: usize,
    pub cursor: Position,
}

impl Default for TargetingContext {
    fn default() -> Self {
        Self {
            slot: 0,
            cursor: Position { x: 0, y: 0 },
        }
    }
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
    player_query: Query<(&Position, &Viewshed), With<Player>>,
    monsters: Query<&Position, With<Monster>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    let Ok((player_pos, viewshed)) = player_query.single() else {
        return;
    };

    // Find the nearest visible monster as the initial cursor position.
    let initial_pos = monsters
        .iter()
        .filter(|mpos| viewshed.visible_tiles.contains(&Point::new(mpos.x, mpos.y)))
        .min_by_key(|mpos| (mpos.x - player_pos.x).pow(2) + (mpos.y - player_pos.y).pow(2))
        .copied()
        .unwrap_or(*player_pos);

    // Update the context cursor to the chosen initial position.
    // We can't mutate ctx here (it's Res), so we spawn with the computed pos.
    commands.spawn((
        initial_pos,
        Transform::from_xyz(0.0, 0.0, 5.0),
        Sprite {
            color: Color::srgba(1.0, 1.0, 0.0, 0.4),
            custom_size: Some(Vec2::splat(16.0)),
            ..default()
        },
        Visibility::Visible,
        RenderLayers::layer(1),
        GameEntityMarker,
        SpellCursor,
    ));

    // Also sync the context so input system knows where cursor starts.
    // We need ResMut for this — done via a separate one-shot approach below.
    // The input system reads the SpellCursor Position directly.
    let _ = ctx.slot; // ensure ctx is used

    log_writer.write(GameLogMessage(
        "Choose target: arrows to move, Enter to confirm, Esc to cancel.".to_string(),
    ));
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

/// Despawns the targeting cursor when exiting Targeting state.
fn teardown_targeting(
    mut commands: Commands,
    cursor_query: Query<Entity, With<SpellCursor>>,
) {
    for entity in cursor_query.iter() {
        commands.entity(entity).despawn();
    }
}

/// Handles input while in Targeting state.
fn handle_targeting_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut ctx: ResMut<TargetingContext>,
    mut cursor_query: Query<&mut Position, With<SpellCursor>>,
    monsters: Query<(Entity, &Position), With<Monster>>,
    map: Res<Map>,
    mut turn_manager: ResMut<TurnManager>,
    mut next_turn_state: ResMut<NextState<TurnState>>,
    mut next_ingame_state: ResMut<NextState<InGameState>>,
    mut log_writer: MessageWriter<GameLogMessage>,
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
        let found = monsters
            .iter()
            .find(|(_, mpos)| mpos.x == target_pos.x && mpos.y == target_pos.y)
            .map(|(e, _)| e);

        if let Some(entity) = found {
            turn_manager.player_action_pending = Some(Action::CastSpell {
                slot: ctx.slot,
                target: Some(entity),
            });
            next_turn_state.set(TurnState::Processing);
            next_ingame_state.set(InGameState::Running);
        } else {
            log_writer.write(GameLogMessage("No valid target at cursor.".to_string()));
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
