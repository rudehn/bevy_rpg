//! Help screen — keybinding reference displayed with `?` key.

use bevy::prelude::*;

use crate::game::{AppState, InGameState};
use crate::ui::modal::{ModalConfig, despawn_screen, spawn_modal, toggle_screen};

pub struct HelpPlugin;

impl Plugin for HelpPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            help_input_system.run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnEnter(InGameState::Help), spawn_help_ui)
        .add_systems(OnExit(InGameState::Help), despawn_screen::<OnHelpScreen>);
    }
}

#[derive(Component)]
struct OnHelpScreen;

fn help_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    // ? is Shift+/ on most keyboards
    if keys.just_pressed(KeyCode::Slash) && keys.pressed(KeyCode::ShiftLeft)
        || keys.just_pressed(KeyCode::Slash) && keys.pressed(KeyCode::ShiftRight)
    {
        if *state.get() == InGameState::Running {
            next_state.set(InGameState::Help);
        } else if *state.get() == InGameState::Help {
            next_state.set(InGameState::Running);
        }
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && *state.get() == InGameState::Help {
        next_state.set(InGameState::Running);
    }
}

fn spawn_help_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/SourceCodePro.ttf");

    spawn_modal(
        &mut commands,
        OnHelpScreen,
        &font,
        &ModalConfig {
            title: "Help — Keybindings",
            width: 500.0,
            height: 520.0,
            footer: "Press ? or Esc to close",
            ..default()
        },
        |panel, font| {
            let section = |panel: &mut ChildSpawnerCommands, heading: &str, bindings: &[(&str, &str)]| {
                // Heading
                panel.spawn((
                    Text::new(heading),
                    TextFont { font: font.clone(), font_size: 18.0, ..default() },
                    TextColor(Color::srgb(1.0, 0.84, 0.0)),
                ));
                panel.spawn(Node { height: Val::Px(4.0), ..default() });

                // Bindings
                let lines: String = bindings
                    .iter()
                    .map(|(key, desc)| format!("  {:<14} {}", key, desc))
                    .collect::<Vec<_>>()
                    .join("\n");
                panel.spawn((
                    Text::new(lines),
                    TextFont { font: font.clone(), font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.85, 0.85, 0.85)),
                ));
                panel.spawn(Node { height: Val::Px(10.0), ..default() });
            };

            section(panel, "Movement", &[
                ("W/A/S/D", "Move (or Arrow keys)"),
                ("Space", "Wait one turn"),
                ("G", "Pick up item"),
                (".", ">  Descend stairs"),
            ]);

            section(panel, "Screens", &[
                ("I", "Inventory & Equipment"),
                ("C", "Character info (race / class / level / attributes)"),
                ("M", "Skills (training screen)"),
                ("L", "Log history"),
                ("Tab", "Cycle nearby entities"),
                ("?", "This help screen"),
            ]);

            section(panel, "Level-Up (when prompted)", &[
                ("S", "Spend a point into Strength"),
                ("D", "Spend a point into Dexterity"),
                ("I", "Spend a point into Intelligence"),
            ]);

            section(panel, "Inventory Actions", &[
                ("E", "Equip / unequip"),
                ("U", "Use item (potions, staves)"),
                ("D", "Drop item"),
                ("J/K", "Navigate up/down"),
            ]);

            section(panel, "Targeting", &[
                ("W/A/S/D", "Move cursor"),
                ("Enter/Space", "Confirm target"),
                ("Esc", "Cancel"),
            ]);

            section(panel, "Camera", &[
                ("-", "Zoom out"),
                ("+", "Zoom in"),
            ]);

            section(panel, "Character Creation", &[
                ("\u{2191}/\u{2193}", "Cycle field (Race / Class / STR / DEX / CON / INT / Begin)"),
                ("\u{2190}/\u{2192}", "Change race / class selection, or adjust focused attribute"),
                ("Enter", "Begin Descent (when on Begin button)"),
                ("Esc", "Return to main menu"),
            ]);

            section(panel, "Skill Screen", &[
                ("\u{2191}/\u{2193}", "Navigate skills"),
                ("Enter", "Cycle state: Normal \u{2192} Focused \u{2192} Disabled"),
                ("/", "Toggle Auto / Manual training mode"),
                ("M / Esc", "Close"),
            ]);
        },
    );
}
