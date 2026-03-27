use bevy::prelude::*;

use crate::game::InGameState;

// ---------------------------------------------------------------------------
// Toggle input helper
// ---------------------------------------------------------------------------

/// Shared screen toggle logic. Call from each screen's input system.
///
/// - `toggle_key` opens/closes the screen (toggles Running ↔ `screen_state`).
/// - Escape always closes when the screen is active.
/// - Returns `true` if a transition was triggered (caller should return early).
pub fn toggle_screen(
    keys: &ButtonInput<KeyCode>,
    state: &State<InGameState>,
    next_state: &mut NextState<InGameState>,
    toggle_key: KeyCode,
    screen_state: InGameState,
) -> bool {
    if keys.just_pressed(toggle_key) {
        if *state.get() == InGameState::Running {
            next_state.set(screen_state);
            return true;
        } else if *state.get() == screen_state {
            next_state.set(InGameState::Running);
            return true;
        }
    }
    if keys.just_pressed(KeyCode::Escape) && *state.get() == screen_state {
        next_state.set(InGameState::Running);
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Modal overlay spawn helper
// ---------------------------------------------------------------------------

/// Configuration for a modal overlay screen.
pub struct ModalConfig {
    pub title: &'static str,
    pub title_color: Color,
    pub footer: &'static str,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
}

impl Default for ModalConfig {
    fn default() -> Self {
        Self {
            title: "",
            title_color: Color::srgb(1.0, 0.84, 0.0),
            footer: "",
            width: 700.0,
            height: 520.0,
            opacity: 0.85,
        }
    }
}

/// Spawns a modal overlay with a centered panel containing a title and footer.
///
/// `marker` is a component attached to the root entity for despawn queries.
/// `body_fn` receives the inner panel's `ChildSpawnerCommands` to add screen-specific content
/// (the title and spacer are already spawned above; the footer is spawned below).
pub fn spawn_modal<M: Component>(
    commands: &mut Commands,
    marker: M,
    font: &Handle<Font>,
    config: &ModalConfig,
    body_fn: impl FnOnce(&mut ChildSpawnerCommands, &Handle<Font>),
) {
    let font = font.clone();
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, config.opacity)),
            ZIndex(200),
            marker,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(config.width),
                    height: Val::Px(config.height),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(20.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::BLACK),
                BorderColor::all(Color::WHITE),
            ))
            .with_children(|panel| {
                // Title
                panel.spawn((
                    Text::new(config.title),
                    TextFont { font: font.clone(), font_size: 28.0, ..default() },
                    TextColor(config.title_color),
                ));
                panel.spawn(Node { height: Val::Px(10.0), ..default() });

                // Screen-specific body content
                body_fn(panel, &font);

                // Footer
                panel.spawn(Node { height: Val::Px(10.0), ..default() });
                panel.spawn((
                    Text::new(config.footer),
                    TextFont { font: font.clone(), font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                ));
            });
        });
}

// ---------------------------------------------------------------------------
// Generic despawn system
// ---------------------------------------------------------------------------

/// Despawns all entities with the given marker component.
/// Use as: `.add_systems(OnExit(InGameState::Foo), despawn_screen::<OnFooScreen>)`
pub fn despawn_screen<M: Component>(
    mut commands: Commands,
    query: Query<Entity, With<M>>,
) {
    for entity in query.iter() {
        commands.entity(entity).despawn();
    }
}
