//! Character info screen (Phase 2). Bound to `C`. Shows the player's
//! race / class / level / XP, attribute scores and modifiers, derived
//! combat stats, resistances, and progression hints (next racial gain
//! level, next player-choice ASI level).

use bevy::prelude::*;

use crate::character::{ability_mod, Attribute, Attributes, Class, Race, RaceManifest, RaceManifestHandle};
use crate::game::combat::{Health, Resistances};
use crate::game::stats::{Armor, DamageBonus, Dodge, HitBonus};
use crate::game::turns::TurnState;
use crate::game::xp::{Experience, Level, PLAYER_CHOICE_LEVELS, LEVEL_CAP, xp_to_next_level};
use crate::game::{AppState, InGameState};
use crate::player::Player;
use crate::ui::modal::{spawn_modal, despawn_screen, ModalConfig, GOLD};

#[derive(Component)]
struct OnCharacterInfoScreen;

#[derive(Component)]
struct CharInfoBodyText;

pub struct CharacterInfoPlugin;

impl Plugin for CharacterInfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            character_info_input
                .run_if(in_state(AppState::InGame))
                .run_if(in_state(TurnState::PlayerInput).or(in_state(InGameState::CharacterInfo))),
        )
        .add_systems(OnEnter(InGameState::CharacterInfo), spawn_character_info_ui)
        .add_systems(
            Update,
            update_character_info_ui.run_if(in_state(InGameState::CharacterInfo)),
        )
        .add_systems(
            OnExit(InGameState::CharacterInfo),
            despawn_screen::<OnCharacterInfoScreen>,
        );
    }
}

fn character_info_input(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    crate::ui::modal::toggle_screen(
        &keys,
        &state,
        &mut next_state,
        KeyCode::KeyC,
        InGameState::CharacterInfo,
    );
}

fn spawn_character_info_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/Macondo-Regular.ttf");
    spawn_modal(
        &mut commands,
        OnCharacterInfoScreen,
        &font,
        &ModalConfig {
            title: "Character",
            title_color: GOLD,
            footer: "[C] Close   |   [Esc] Close",
            width: 540.0,
            height: 480.0,
            ..default()
        },
        |panel, font| {
            panel.spawn((
                Text::new(""),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(Color::srgb(0.92, 0.92, 0.92)),
                CharInfoBodyText,
            ));
        },
    );
}

fn next_racial_gain_level(current: u32, interval: u32) -> Option<u32> {
    if interval == 0 || current >= LEVEL_CAP {
        return None;
    }
    let next = current - (current % interval) + interval;
    if next <= LEVEL_CAP { Some(next) } else { None }
}

fn next_player_choice_level(current: u32) -> Option<u32> {
    PLAYER_CHOICE_LEVELS.iter().copied().find(|&l| l > current)
}

fn update_character_info_ui(
    player_q: Query<
        (
            &Race,
            &Class,
            &Attributes,
            &Level,
            &Experience,
            &Health,
            &HitBonus,
            &DamageBonus,
            &Dodge,
            &Armor,
            Option<&Resistances>,
        ),
        With<Player>,
    >,
    race_manifest_handle: Res<RaceManifestHandle>,
    race_manifests: Res<Assets<RaceManifest>>,
    mut body_q: Query<&mut Text, With<CharInfoBodyText>>,
) {
    let Ok((race, class, attrs, level, xp, health, hit, dmg, dodge, armor, resists)) =
        player_q.single()
    else {
        return;
    };
    let Some(race_manifest) = race_manifests.get(&race_manifest_handle.0) else {
        return;
    };
    let race_asset = race_manifest.races.get(&race.name().to_lowercase());

    let xp_line = if level.0 >= LEVEL_CAP {
        format!("Level {} — MAX", level.0)
    } else {
        format!("Level {}   XP {} / {}", level.0, xp.0, xp_to_next_level(level.0))
    };

    let next_racial_str = race_asset
        .and_then(|ra| next_racial_gain_level(level.0, ra.gain_schedule.interval))
        .map(|l| format!("L{} ({})", l, race_asset.unwrap().gain_schedule.notation()))
        .unwrap_or_else(|| "—".to_string());
    let next_choice_str = next_player_choice_level(level.0)
        .map(|l| format!("L{}", l))
        .unwrap_or_else(|| "—".to_string());

    let mut body = String::new();
    body.push_str(&format!("{} {}\n", race, class));
    body.push_str(&format!("{}\n", xp_line));
    body.push_str("\n");
    body.push_str("Attributes:\n");
    for attr in [Attribute::Str, Attribute::Dex, Attribute::Int] {
        let score = attrs.get(attr);
        let m = ability_mod(score);
        body.push_str(&format!("  {} {:>2}   ({:+})\n", attr.name(), score, m));
    }
    body.push_str("\n");
    body.push_str(&format!(
        "HP {}/{}    Armor {}    Dodge {}\n",
        health.current, health.max, armor.0, dodge.0
    ));
    body.push_str(&format!(
        "HitBonus {:+}    DamageBonus {:+}\n",
        hit.0, dmg.0
    ));
    body.push_str("\n");
    body.push_str("Resistances:\n");
    if let Some(r) = resists
        && !r.0.is_empty()
    {
        for (dt, pct) in &r.0 {
            body.push_str(&format!("  {:?}: {}%\n", dt, pct));
        }
    } else {
        body.push_str("  (none)\n");
    }
    body.push_str("\n");
    body.push_str(&format!("Next racial gain: {}\n", next_racial_str));
    body.push_str(&format!("Next free point:  {}\n", next_choice_str));

    if let Ok(mut t) = body_q.single_mut() {
        *t = Text::new(body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_racial_gain_picks_next_multiple_of_interval() {
        // interval 4: at L1 next is L4; at L4 next is L8; at L24 next is L28
        // (clamped: L28 > LEVEL_CAP so None)
        assert_eq!(next_racial_gain_level(1, 4), Some(4));
        assert_eq!(next_racial_gain_level(3, 4), Some(4));
        assert_eq!(next_racial_gain_level(4, 4), Some(8));
        assert_eq!(next_racial_gain_level(24, 4), None); // 28 > 27
        assert_eq!(next_racial_gain_level(27, 4), None);
    }

    #[test]
    fn next_player_choice_returns_correct_milestone() {
        assert_eq!(next_player_choice_level(1), Some(3));
        assert_eq!(next_player_choice_level(3), Some(9));
        assert_eq!(next_player_choice_level(15), Some(21));
        assert_eq!(next_player_choice_level(27), None);
    }
}
