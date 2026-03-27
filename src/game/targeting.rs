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
}

impl Default for TargetingContext {
    fn default() -> Self {
        Self {
            mode: TargetingMode::default(),
            cursor: Position { x: 0, y: 0 },
        }
    }
}

// --- Spell targeting decision ---

use crate::game::spells::{SpellData, SpellEffect, SpellTarget};

/// Result of determining the targeting mode for a spell.
pub enum SpellTargetingResult {
    /// Enter targeting UI with this mode.
    EnterTargeting(TargetingMode),
    /// Cast immediately (self-targeting spell).
    CastImmediate { slot: usize },
}

/// Determines the appropriate targeting mode for a spell based on its effects and target type.
/// Centralizes the decision so `handle_player_input` doesn't need to know about spell internals.
pub fn targeting_mode_for_spell(spell: &SpellData, slot: usize) -> SpellTargetingResult {
    // Tile-targeted spells (e.g., Blink: Teleport with range > 0).
    let tile_range = spell.effects.iter().find_map(|e| match e {
        SpellEffect::Teleport { range } if *range > 0 => Some(*range),
        _ => None,
    });

    if let Some(range) = tile_range {
        return SpellTargetingResult::EnterTargeting(
            TargetingMode::Tile { slot, range, radius: 0 },
        );
    }

    match spell.target {
        SpellTarget::Enemy => SpellTargetingResult::EnterTargeting(TargetingMode::Spell { slot }),
        SpellTarget::Ally => SpellTargetingResult::EnterTargeting(
            TargetingMode::SpellAlly { slot, include_self: false },
        ),
        SpellTarget::AllyOrSelf => SpellTargetingResult::EnterTargeting(
            TargetingMode::SpellAlly { slot, include_self: true },
        ),
        SpellTarget::Castor => SpellTargetingResult::CastImmediate { slot },
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

                if let Some(entity) = found {
                    pending.0 = Some(Action::CastSpell { slot: *slot, target: Some(entity), target_pos: None });
                    next_turn_state.set(TurnState::Processing);
                    next_ingame_state.set(InGameState::Running);
                } else {
                    log_writer.write(GameLogMessage("No valid target at cursor.".to_string()));
                }
            }
            TargetingMode::SpellAlly { slot, include_self } => {
                // Ally targeting: find an allied entity at cursor (or self).
                let player_faction = player_query.single().ok().map(|(_, _, f)| f.0.clone());

                let found = if let Some(pf) = &player_faction {
                    // Check if player is targeting themselves
                    let self_target = player_query.single().ok().and_then(|(e, pos, _)| {
                        if *include_self && pos.x == target_pos.x && pos.y == target_pos.y {
                            Some(e)
                        } else {
                            None
                        }
                    });

                    self_target.or_else(|| {
                        faction_entities.iter()
                            .find(|(_, pos, faction)| {
                                pos.x == target_pos.x && pos.y == target_pos.y && faction_matrix.is_allied_to(&pf.0, &faction.0.0)
                            })
                            .map(|(e, _, _)| e)
                    })
                } else {
                    None
                };

                if let Some(entity) = found {
                    pending.0 = Some(Action::CastSpell { slot: *slot, target: Some(entity), target_pos: None });
                    next_turn_state.set(TurnState::Processing);
                    next_ingame_state.set(InGameState::Running);
                } else {
                    log_writer.write(GameLogMessage("No valid ally at cursor.".to_string()));
                }
            }
            TargetingMode::Tile { slot, range, .. } => {
                // Tile targeting: validate walkable tile within range.
                let player_pos = player_query.single().ok().map(|(_, p, _)| *p);
                let in_range = player_pos.map(|pp| {
                    (target_pos.x - pp.x).abs() + (target_pos.y - pp.y).abs() <= *range
                }).unwrap_or(false);

                let idx = map.xy_idx(target_pos.x, target_pos.y);
                let walkable = is_walkable(map.tiles[idx]);

                // Check no entity is standing on the tile (for blink).
                let occupied = faction_entities.iter().any(|(_, pos, _)| {
                    pos.x == target_pos.x && pos.y == target_pos.y
                }) || player_pos.map(|pp| pp.x == target_pos.x && pp.y == target_pos.y).unwrap_or(false);

                if in_range && walkable && !occupied {
                    let player_entity = player_query.single().ok().map(|(e, _, _)| e);
                    pending.0 = Some(Action::CastSpell {
                        slot: *slot,
                        target: player_entity,
                        target_pos: Some((target_pos.x, target_pos.y)),
                    });
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
    use crate::game::combat::DamageType;

    fn make_spell(target: SpellTarget, effects: Vec<SpellEffect>) -> SpellData {
        SpellData {
            name: "Test".to_string(),
            mana_cost: 5,
            cooldown: 0,
            description: String::new(),
            target,
            range: 5,
            effects,
            damage_type: DamageType::Physical,
        }
    }

    #[test]
    fn castor_spell_returns_cast_immediate() {
        let spell = make_spell(SpellTarget::Castor, vec![SpellEffect::ApplyHaste { duration: 5 }]);
        let result = targeting_mode_for_spell(&spell, 2);
        assert!(matches!(result, SpellTargetingResult::CastImmediate { slot: 2 }));
    }

    #[test]
    fn enemy_spell_enters_targeting() {
        let spell = make_spell(SpellTarget::Enemy, vec![SpellEffect::Damage { dice: "2d6".into(), int_scaling: false }]);
        let result = targeting_mode_for_spell(&spell, 0);
        assert!(matches!(result, SpellTargetingResult::EnterTargeting(TargetingMode::Spell { slot: 0 })));
    }

    #[test]
    fn ally_spell_excludes_self() {
        let spell = make_spell(SpellTarget::Ally, vec![SpellEffect::Heal { dice: "1d8".into(), int_scaling: false }]);
        let result = targeting_mode_for_spell(&spell, 1);
        assert!(matches!(
            result,
            SpellTargetingResult::EnterTargeting(TargetingMode::SpellAlly { slot: 1, include_self: false })
        ));
    }

    #[test]
    fn ally_or_self_spell_includes_self() {
        let spell = make_spell(SpellTarget::AllyOrSelf, vec![SpellEffect::Heal { dice: "1d8".into(), int_scaling: false }]);
        let result = targeting_mode_for_spell(&spell, 3);
        assert!(matches!(
            result,
            SpellTargetingResult::EnterTargeting(TargetingMode::SpellAlly { slot: 3, include_self: true })
        ));
    }

    #[test]
    fn teleport_with_range_enters_tile_targeting() {
        let spell = make_spell(SpellTarget::Castor, vec![SpellEffect::Teleport { range: 5 }]);
        let result = targeting_mode_for_spell(&spell, 4);
        assert!(matches!(
            result,
            SpellTargetingResult::EnterTargeting(TargetingMode::Tile { slot: 4, range: 5, radius: 0 })
        ));
    }

    #[test]
    fn teleport_zero_range_is_immediate() {
        let spell = make_spell(SpellTarget::Castor, vec![SpellEffect::Teleport { range: 0 }]);
        let result = targeting_mode_for_spell(&spell, 0);
        assert!(matches!(result, SpellTargetingResult::CastImmediate { slot: 0 }));
    }

    #[test]
    fn teleport_overrides_target_type() {
        let spell = make_spell(SpellTarget::Enemy, vec![
            SpellEffect::Damage { dice: "1d4".into(), int_scaling: false },
            SpellEffect::Teleport { range: 3 },
        ]);
        let result = targeting_mode_for_spell(&spell, 1);
        assert!(matches!(
            result,
            SpellTargetingResult::EnterTargeting(TargetingMode::Tile { slot: 1, range: 3, .. })
        ));
    }
}
