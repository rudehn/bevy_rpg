//! Help screen — keybinding reference displayed with `?` key.

use bevy::prelude::*;

use crate::game::InGameState;
use crate::ui::modal::{despawn_screen, spawn_modal, ModalConfig};
use crate::ui::registry::{close_on_toggle_or_escape, HelpEntry, Modifiers, UiScreen};

/// Registry entry for the help screen.
pub struct HelpScreen;

impl UiScreen for HelpScreen {
    const STATE: InGameState = InGameState::Help;
    const OPEN_KEY: Option<KeyCode> = Some(KeyCode::Slash);
    const OPEN_MODIFIERS: Modifiers = Modifiers::SHIFT;
    // No PlayerInput gate — Help is reachable from any in-game moment
    // (the legacy `help_input_system` had no gate either).
    const HELP: Option<HelpEntry> = Some(HelpEntry {
        display: "?",
        label: "Help",
    });

    fn build(app: &mut App) {
        app.add_systems(OnEnter(Self::STATE), spawn_help_ui)
            .add_systems(OnExit(Self::STATE), despawn_screen::<OnHelpScreen>)
            .add_systems(
                Update,
                close_on_toggle_or_escape::<Self>.run_if(in_state(Self::STATE)),
            );
    }
}

#[derive(Component)]
struct OnHelpScreen;

fn spawn_help_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    registry: Res<crate::ui::registry::ScreenRegistry>,
) {
    let font = asset_server.load("fonts/SourceCodePro.ttf");

    // The "Screens" section is derived from `ScreenRegistry` so adding
    // a new modal screen automatically appears here. Other sections
    // (Movement, Targeting, in-screen bindings) stay hand-written.
    let screen_rows: Vec<(String, String)> = registry
        .help_entries
        .iter()
        .map(|e| (e.display.to_string(), e.label.to_string()))
        .chain(std::iter::once((
            "Tab".to_string(),
            "Cycle nearby entities".to_string(),
        )))
        .collect();

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
        move |panel, font| {
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

            let screen_rows_refs: Vec<(&str, &str)> = screen_rows
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            section(panel, "Screens", &screen_rows_refs);

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
                ("=", "Set target level on focused skill (auto-disables on reach)"),
                ("/", "Toggle Auto / Manual training mode"),
                ("M / Esc", "Close"),
            ]);
        },
    );
}
