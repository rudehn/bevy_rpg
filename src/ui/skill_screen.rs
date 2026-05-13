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

pub struct SkillScreenPlugin;

impl Plugin for SkillScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkillScreenFocus>()
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
                despawn_screen::<OnSkillScreen>,
            );
    }
}

fn skill_screen_open_close(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
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
            footer: "[\u{2191}/\u{2193}] navigate  [Enter] cycle state  [/] Auto/Manual  [M/Esc] close",
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
    mut training_q: Query<&mut SkillTraining, With<Player>>,
) {
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
}

fn refresh_skill_screen(
    focus: Res<SkillScreenFocus>,
    mode: Res<TrainingMode>,
    pool: Res<SkillXpPool>,
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
    body.push_str("[+] training    [*] focused (2x XP)    [-] disabled\n\n");

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
        let cursor = if i == focus.0 { ">" } else { " " };
        body.push_str(&format!(
            "{} [{}] {:<18}{:>5.1}   {}\n",
            cursor,
            badge,
            skill.name(),
            level,
            apt_str,
        ));
    }

    if let Ok(mut t) = body_q.single_mut() {
        *t = Text::new(body);
    }
}
