//! DCSS-style ASI (Attribute Score Improvement) prompt.
//!
//! When a `PendingAsi` component appears on the player, the running game
//! transitions to `InGameState::AsiSelect`. A small overlay shows the
//! letters S/D/I (with disallowed ones greyed) and the points remaining.
//! Each S/D/I keypress applies +1 to that attribute and decrements
//! points. When points hit 0 the modal closes; the queue (`QueuedAsi`)
//! is drained into the next `PendingAsi` if any remain.

use bevy::prelude::*;

use crate::character::{Attribute, Attributes};
use crate::game::xp::{PendingAsi, QueuedAsi};
use crate::game::{AppState, InGameState};
use crate::player::Player;

const BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.85);
const PANEL_BG: Color = Color::srgb(0.12, 0.10, 0.18);
const PANEL_BORDER: Color = Color::srgb(0.5, 0.42, 0.20);
const GOLD: Color = Color::srgb(1.0, 0.85, 0.0);
const DIM: Color = Color::srgb(0.45, 0.45, 0.45);
const TEXT: Color = Color::srgb(0.92, 0.92, 0.92);

#[derive(Component)]
struct OnAsiScreen;

#[derive(Component)]
struct AsiTitleText;

#[derive(Component)]
struct AsiRemainingText;

#[derive(Component, Debug, Clone, Copy)]
struct AsiLetterText(Attribute);

pub struct AsiModalPlugin;

impl Plugin for AsiModalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            transition_to_modal.run_if(in_state(InGameState::Running)),
        )
        .add_systems(OnEnter(InGameState::AsiSelect), spawn_modal)
        .add_systems(
            Update,
            (refresh_modal, handle_keypress)
                .chain()
                .run_if(in_state(InGameState::AsiSelect)),
        )
        .add_systems(OnExit(InGameState::AsiSelect), despawn_modal);
    }
}

/// While the game's running, if the player has a `PendingAsi`, open the
/// modal. The opposite transition happens inside `handle_keypress` when
/// points reach 0.
fn transition_to_modal(
    pending_q: Query<(), (With<Player>, With<PendingAsi>)>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    if !pending_q.is_empty() {
        next_state.set(InGameState::AsiSelect);
    }
}

fn spawn_modal(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(BG),
            OnAsiScreen,
            ZIndex(1000),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(24.0)),
                    row_gap: Val::Px(12.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
                BorderColor::all(PANEL_BORDER),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Choose Attribute"),
                    TextFont { font: font.clone(), font_size: 28.0, ..default() },
                    TextColor(GOLD),
                    AsiTitleText,
                ));
                panel.spawn((
                    Text::new("Remaining: 0"),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(DIM),
                    AsiRemainingText,
                ));
                // Letter row
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(24.0),
                        margin: UiRect::top(Val::Px(8.0)),
                        ..default()
                    })
                    .with_children(|row| {
                        for attr in [Attribute::Str, Attribute::Dex, Attribute::Int] {
                            row.spawn((
                                Text::new(format!("({}){}", attr.letter(), &attr.name()[1..])),
                                TextFont { font: font.clone(), font_size: 22.0, ..default() },
                                TextColor(TEXT),
                                AsiLetterText(attr),
                            ));
                        }
                    });
                panel.spawn((
                    Text::new("Press the highlighted letter to spend a point."),
                    TextFont { font: font.clone(), font_size: 13.0, ..default() },
                    TextColor(DIM),
                    Node { margin: UiRect::top(Val::Px(8.0)), ..default() },
                ));
            });
        });
}

fn despawn_modal(mut commands: Commands, q: Query<Entity, With<OnAsiScreen>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn refresh_modal(
    pending_q: Query<&PendingAsi, With<Player>>,
    mut title_q: Query<&mut Text, (With<AsiTitleText>, Without<AsiRemainingText>, Without<AsiLetterText>)>,
    mut remaining_q: Query<&mut Text, (With<AsiRemainingText>, Without<AsiTitleText>, Without<AsiLetterText>)>,
    mut letters_q: Query<
        (&AsiLetterText, &mut TextColor),
        (Without<AsiTitleText>, Without<AsiRemainingText>),
    >,
) {
    let Ok(pending) = pending_q.single() else {
        return;
    };
    if let Ok(mut t) = title_q.single_mut() {
        *t = Text::new(pending.label.clone());
    }
    if let Ok(mut t) = remaining_q.single_mut() {
        *t = Text::new(format!("Remaining: {}", pending.points));
    }
    for (marker, mut color) in &mut letters_q {
        let allowed = pending.allowed.contains(&marker.0);
        *color = TextColor(if allowed { TEXT } else { DIM });
    }
}

fn handle_keypress(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut player_q: Query<
        (Entity, &mut Attributes, &mut PendingAsi, Option<&mut QueuedAsi>),
        With<Player>,
    >,
    mut next_state: ResMut<NextState<InGameState>>,
    mut log_writer: MessageWriter<crate::ui::game_log::GameLogMessage>,
) {
    let Ok((player_entity, mut attrs, mut pending, queued_opt)) = player_q.single_mut() else {
        return;
    };

    let pressed_attr = if keys.just_pressed(KeyCode::KeyS) {
        Some(Attribute::Str)
    } else if keys.just_pressed(KeyCode::KeyD) {
        Some(Attribute::Dex)
    } else if keys.just_pressed(KeyCode::KeyI) {
        Some(Attribute::Int)
    } else {
        None
    };

    let Some(attr) = pressed_attr else { return };
    if !pending.allowed.contains(&attr) || pending.points == 0 {
        return;
    }

    attrs.add(attr, 1);
    pending.points -= 1;
    log_writer.write(crate::ui::game_log::GameLogMessage(format!(
        "+1 {}",
        attr.name()
    )));

    if pending.points == 0 {
        // Drain: either pop the next from QueuedAsi or close the modal.
        commands.entity(player_entity).remove::<PendingAsi>();
        let next_pending = queued_opt.and_then(|mut q| {
            if q.0.is_empty() {
                None
            } else {
                Some(q.0.remove(0))
            }
        });
        match next_pending {
            Some(next) => {
                commands.entity(player_entity).insert(next);
                // Stay in AsiSelect; refresh_modal picks up the new pending.
            }
            None => {
                commands.entity(player_entity).remove::<QueuedAsi>();
                next_state.set(InGameState::Running);
            }
        }
    }
}
