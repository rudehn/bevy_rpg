use crate::game::AppState;
use bevy::prelude::*;
use bevy::ui::{
    AlignItems, BackgroundColor, BorderColor, Interaction, JustifyContent, UiRect, Val,
};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuEntryTime>()
            .add_systems(
                OnEnter(AppState::Menu),
                (menu_setup, set_menu_entry_time),
            )
            .add_systems(
                Update,
                menu_action.run_if(in_state(AppState::Menu)),
            )
            .add_systems(OnExit(AppState::Menu), despawn_screen::<OnMainMenuScreen>);
    }
}

// Tag component to mark entities added by the setup_menu system.
#[derive(Component)]
struct OnMainMenuScreen;

#[derive(Resource, Default)]
struct MenuEntryTime(f64);

fn set_menu_entry_time(mut menu_entry_time: ResMut<MenuEntryTime>, time: Res<Time>) {
    menu_entry_time.0 = time.elapsed_secs_f64();
}

fn menu_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    println!("Setting up menu");
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::NONE), // No background for the root Node
            OnMainMenuScreen,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    // Spawn button with individual components
                    Button,
                    Node {
                        // Node for button styling
                        width: Val::Px(150.0),
                        height: Val::Px(65.0),
                        border: UiRect::all(Val::Px(5.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)), // Background color for the button
                    BorderColor::all(Color::BLACK),                 // Border color for the button
                ))
                .with_children(|parent| {
                    parent.spawn((
                        // Spawn text with individual components
                        Text::new("Play"),
                        TextFont::from_font_size(40.0),
                        TextColor(Color::WHITE),
                    ));
                });
        });
    println!("menu initted");
}

fn menu_action(
    interaction_query: Query<&Interaction, (With<Button>, Changed<Interaction>)>,
    mut next_state: ResMut<NextState<AppState>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    menu_entry_time: Res<MenuEntryTime>,
    time: Res<Time>,
) {
    // Only allow input after a short delay to prevent immediate transitions
    if time.elapsed_secs_f64() - menu_entry_time.0 < 0.1 {
        println!("Menu input delayed");
        return;
    }

    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::InGame);
            println!("RETURNING FROM INTERACTION");
            return;
        }
    }

    if keyboard_input.just_pressed(KeyCode::Enter) {
        println!("RETURN key press");
        next_state.set(AppState::InGame);
    }
}

// Generic system that despawns all entities with a given component whenever
// a state is exited
fn despawn_screen<T: Component>(to_despawn: Query<Entity, With<T>>, mut commands: Commands) {
    println!("Despawning screen");
    for entity in &to_despawn {
        commands.entity(entity).despawn();
    }
}
