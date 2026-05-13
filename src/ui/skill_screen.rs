//! Skill training screen (Phase 3). DCSS-inspired single-pane layout.
//!
//! Key M opens it. `↑/↓` navigates rows, `Enter` cycles the focused
//! skill's state (Normal → Focused → Disabled), `/` flips global
//! TrainingMode (Auto ↔ Manual), `M/Esc` closes.

use bevy::prelude::*;

use crate::character::{Race, RaceManifest, RaceManifestHandle};
use crate::game::skills::{
    Skill, Skills, SkillState, SkillTraining, SkillXp, SkillXpPool, TrainingMode,
};
use crate::game::turns::TurnState;
use crate::game::{AppState, InGameState};
use crate::player::Player;
use crate::ui::modal::{despawn_screen, spawn_modal, ModalConfig, GOLD};

#[derive(Component)]
struct OnSkillScreen;

#[derive(Component)]
struct SkillScreenBodyText;

#[derive(Resource, Default, Debug, Clone, Copy)]
struct SkillScreenFocus(usize);

/// While the user is typing a skill target, capture digits here. `None`
/// means we're not in input mode. The input is committed on Enter, or
/// discarded on Esc.
#[derive(Resource, Default, Debug, Clone)]
struct SkillTargetInput {
    /// Skill the target is being set for (the focused row when `=` was pressed).
    skill: Option<Skill>,
    /// Accumulated digit string, e.g. "12".
    buffer: String,
}

pub struct SkillScreenPlugin;

impl Plugin for SkillScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkillScreenFocus>()
            .init_resource::<SkillTargetInput>()
            .add_systems(
                Update,
                skill_screen_open_close
                    .run_if(in_state(AppState::InGame))
                    .run_if(in_state(TurnState::PlayerInput).or(in_state(InGameState::SkillScreen))),
            )
            .add_systems(OnEnter(InGameState::SkillScreen), spawn_skill_screen_ui)
            .add_systems(
                Update,
                (skill_screen_input, refresh_skill_screen)
                    .chain()
                    .run_if(in_state(InGameState::SkillScreen)),
            )
            .add_systems(
                OnExit(InGameState::SkillScreen),
                (despawn_screen::<OnSkillScreen>, clear_target_input_on_exit),
            );
    }
}

fn clear_target_input_on_exit(mut input: ResMut<SkillTargetInput>) {
    input.skill = None;
    input.buffer.clear();
}

fn skill_screen_open_close(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    target_input: Res<SkillTargetInput>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    // While the player is typing a target value, suppress the toggle
    // keys — the digit keys, Enter, Esc, and Backspace are owned by
    // the target-input handler in `skill_screen_input`.
    if target_input.skill.is_some() {
        return;
    }
    crate::ui::modal::toggle_screen(
        &keys,
        &state,
        &mut next_state,
        KeyCode::KeyM,
        InGameState::SkillScreen,
    );
}

fn spawn_skill_screen_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");
    spawn_modal(
        &mut commands,
        OnSkillScreen,
        &font,
        &ModalConfig {
            title: "Skills",
            title_color: GOLD,
            footer: "[\u{2191}/\u{2193}] navigate  [Enter] cycle state  [=] set target  [/] Auto/Manual  [M/Esc] close",
            width: 600.0,
            height: 500.0,
            ..default()
        },
        |panel, font| {
            panel.spawn((
                Text::new(""),
                TextFont {
                    font: font.clone(),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.92, 0.92)),
                SkillScreenBodyText,
            ));
        },
    );
}

fn skill_screen_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<SkillScreenFocus>,
    mut mode: ResMut<TrainingMode>,
    mut target_input: ResMut<SkillTargetInput>,
    mut training_q: Query<&mut SkillTraining, With<Player>>,
) {
    // ----- Target-input mode: digits / Enter / Esc / Backspace only -----
    if target_input.skill.is_some() {
        if keys.just_pressed(KeyCode::Escape) {
            target_input.skill = None;
            target_input.buffer.clear();
            return;
        }
        if keys.just_pressed(KeyCode::Backspace) {
            target_input.buffer.pop();
            return;
        }
        if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
            let skill = target_input.skill.expect("guarded above");
            let value: u32 = target_input.buffer.parse().unwrap_or(0);
            if let Ok(mut training) = training_q.single_mut() {
                training.set_target(skill, value);
            }
            target_input.skill = None;
            target_input.buffer.clear();
            return;
        }
        // Collect digits 0-9.
        for (key, digit) in [
            (KeyCode::Digit0, '0'),
            (KeyCode::Digit1, '1'),
            (KeyCode::Digit2, '2'),
            (KeyCode::Digit3, '3'),
            (KeyCode::Digit4, '4'),
            (KeyCode::Digit5, '5'),
            (KeyCode::Digit6, '6'),
            (KeyCode::Digit7, '7'),
            (KeyCode::Digit8, '8'),
            (KeyCode::Digit9, '9'),
        ] {
            if keys.just_pressed(key) && target_input.buffer.len() < 2 {
                target_input.buffer.push(digit);
            }
        }
        return; // While typing, suppress all other key behaviors
    }

    // ----- Normal navigation mode -----
    if keys.just_pressed(KeyCode::ArrowDown) {
        focus.0 = (focus.0 + 1) % Skill::ALL.len();
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        focus.0 = (focus.0 + Skill::ALL.len() - 1) % Skill::ALL.len();
    }
    if keys.just_pressed(KeyCode::Slash) {
        *mode = match *mode {
            TrainingMode::Auto => TrainingMode::Manual,
            TrainingMode::Manual => TrainingMode::Auto,
        };
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        if let Ok(mut training) = training_q.single_mut() {
            training.cycle(Skill::ALL[focus.0]);
        }
    }
    // `=` opens target-input mode for the focused skill.
    if keys.just_pressed(KeyCode::Equal) {
        target_input.skill = Some(Skill::ALL[focus.0]);
        target_input.buffer.clear();
    }
}

fn refresh_skill_screen(
    focus: Res<SkillScreenFocus>,
    mode: Res<TrainingMode>,
    pool: Res<SkillXpPool>,
    target_input: Res<SkillTargetInput>,
    player_q: Query<(&Race, &Skills, &SkillTraining, &SkillXp), With<Player>>,
    race_manifest_handle: Res<RaceManifestHandle>,
    race_manifests: Res<Assets<RaceManifest>>,
    mut body_q: Query<&mut Text, With<SkillScreenBodyText>>,
) {
    let Ok((race, skills, training, _skill_xp)) = player_q.single() else {
        return;
    };
    let Some(race_manifest) = race_manifests.get(&race_manifest_handle.0) else {
        return;
    };
    let Some(race_asset) = race_manifest.races.get(&race.name().to_lowercase()) else {
        return;
    };

    let mode_label = match *mode {
        TrainingMode::Auto => "Auto",
        TrainingMode::Manual => "Manual",
    };

    let mut body = String::new();
    body.push_str(&format!(
        "Mode: [{}]                    XP pooled: {}\n",
        mode_label, pool.raw
    ));
    body.push_str(
        "[+] training    [*] focused (2x weight vs Normal)    [-] disabled\n",
    );
    body.push_str(
        "(focused skills only outpace others when not everything is focused — \
         all-focused = all-normal)\n\n",
    );

    for (i, &skill) in Skill::ALL.iter().enumerate() {
        let state = training.get(skill);
        let badge = match state {
            SkillState::Normal => '+',
            SkillState::Focused => '*',
            SkillState::Disabled => '-',
        };
        let level = skills.get(skill);
        let apt = race_asset.aptitudes.for_skill(skill);
        let apt_str = if apt == 0 {
            String::new()
        } else {
            format!("apt {:+}", apt)
        };
        let target_str = match training.target(skill) {
            Some(t) => format!("\u{2192}{}", t), // "→N"
            None => "   ".to_string(),
        };
        let cursor = if i == focus.0 { ">" } else { " " };
        body.push_str(&format!(
            "{} [{}] {:<18}{:>5.1}  {:>4}   {}\n",
            cursor,
            badge,
            skill.name(),
            level,
            target_str,
            apt_str,
        ));
    }

    // If the player is currently typing a target, append a prompt line.
    if let Some(skill) = target_input.skill {
        body.push_str(&format!(
            "\nSet target for {}: {} _   (Enter to confirm, Esc to cancel, 0 = clear)",
            skill.name(),
            target_input.buffer,
        ));
    } else {
        body.push_str(
            "\n[=] set target on focused skill   (skill auto-disables on reach)\n",
        );
    }

    if let Ok(mut t) = body_q.single_mut() {
        *t = Text::new(body);
    }
}
