use bevy::prelude::*;

use crate::assets::SpellRegistryHandle;
use crate::game::magic::{ActiveSpells, KnownSpells};
use crate::game::spells::SpellRegistry;
use crate::game::stats::Mana;
use crate::game::{AppState, InGameState};
use crate::game::turns::TurnState;
use crate::player::Player;

pub struct SpellsPlugin;

impl Plugin for SpellsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedSpellSlot>()
            .add_systems(
                Update,
                spells_input_system.run_if(
                    in_state(AppState::InGame)
                        .and(in_state(TurnState::PlayerInput).or(in_state(InGameState::Spells))),
                ),
            )
            .add_systems(OnEnter(InGameState::Spells), spawn_spells_ui)
            .add_systems(
                Update,
                update_spells_ui.run_if(in_state(InGameState::Spells)),
            )
            .add_systems(OnExit(InGameState::Spells), despawn_spells_ui);
    }
}

// --- Resources ---

#[derive(Resource, Default)]
pub struct SelectedSpellSlot(pub usize);

// --- Marker components ---

#[derive(Component)]
struct OnSpellsScreen;

#[derive(Component)]
struct SpellRowText(usize); // row index into KnownSpells.spells

#[derive(Component)]
struct SpellDetailText;

// --- Systems ---

fn spells_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    if keys.just_pressed(KeyCode::KeyS) {
        match state.get() {
            InGameState::Running => next_state.set(InGameState::Spells),
            InGameState::Spells => next_state.set(InGameState::Running),
            _ => {}
        }
    }
    if keys.just_pressed(KeyCode::Escape) && *state.get() == InGameState::Spells {
        next_state.set(InGameState::Running);
    }
}

fn spawn_spells_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
            ZIndex(200),
            OnSpellsScreen,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(700.0),
                    height: Val::Px(520.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(20.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::BLACK),
                BorderColor::all(Color::WHITE),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("SPELLBOOK"),
                    TextFont { font: font.clone(), font_size: 28.0, ..default() },
                    TextColor(Color::srgb(0.4, 0.6, 1.0)),
                ));

                panel.spawn(Node { height: Val::Px(10.0), ..default() });

                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_grow: 1.0,
                        column_gap: Val::Px(20.0),
                        ..default()
                    })
                    .with_children(|cols| {
                        cols.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            width: Val::Px(320.0),
                            ..default()
                        })
                        .with_children(|list| {
                            for i in 0..10 {
                                list.spawn((
                                    Text::new(format!("{:2}. ---", i + 1)),
                                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                                    SpellRowText(i),
                                ));
                            }
                        });

                        cols.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            border: UiRect::all(Val::Px(1.0)),
                            padding: UiRect::all(Val::Px(10.0)),
                            ..default()
                        })
                        .with_children(|detail| {
                            detail.spawn((
                                Text::new("Select a spell to see its details.\n\nPress 1–6 to assign\nthe highlighted spell to a slot."),
                                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                                TextColor(Color::srgb(0.6, 0.6, 0.6)),
                                SpellDetailText,
                            ));
                        });
                    });

                panel.spawn(Node { height: Val::Px(10.0), ..default() });

                panel.spawn((
                    Text::new("↑/↓ Navigate  |  1–6 Assign to slot  |  S/Esc Close"),
                    TextFont { font: font.clone(), font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                ));
            });
        });
}

fn update_spells_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut selected: ResMut<SelectedSpellSlot>,
    mut player_query: Query<(&KnownSpells, &mut ActiveSpells, &Mana), With<Player>>,
    mut row_texts: Query<(&mut Text, &mut TextColor, &SpellRowText)>,
    mut detail_text: Query<
        (&mut Text, &mut TextColor),
        (With<SpellDetailText>, Without<SpellRowText>),
    >,
    spell_registry_handle: Res<SpellRegistryHandle>,
    spell_registries: Res<Assets<SpellRegistry>>,
) {
    let Ok((known, mut active, mana)) = player_query.single_mut() else {
        return;
    };

    let spell_count = known.spells.len();

    // Navigate
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK) {
        if spell_count > 0 && selected.0 > 0 {
            selected.0 -= 1;
        }
    }
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ) {
        if spell_count > 0 && selected.0 + 1 < spell_count {
            selected.0 += 1;
        }
    }

    // Assign highlighted spell to slot 1–6
    let slot_keys = [
        KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3,
        KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6,
    ];
    if let Some(spell_id) = known.spells.get(selected.0) {
        for (i, &key) in slot_keys.iter().enumerate() {
            if keys.just_pressed(key) {
                for slot in active.slots.iter_mut() {
                    if slot.as_deref() == Some(spell_id.as_str()) {
                        *slot = None;
                    }
                }
                active.slots[i] = Some(spell_id.clone());
                break;
            }
        }
    }

    let registry = spell_registries.get(&spell_registry_handle.0);

    // Update spell rows
    for (mut text, mut color, row) in &mut row_texts {
        let i = row.0;
        if let Some(spell_id) = known.spells.get(i) {
            let spell_name = registry
                .and_then(|r| r.spells.get(spell_id))
                .map(|s| s.name.as_str())
                .unwrap_or(spell_id.as_str());

            let slot_label: String = active
                .slots
                .iter()
                .enumerate()
                .find(|(_, s)| s.as_deref() == Some(spell_id.as_str()))
                .map(|(idx, _)| format!(" [{}]", idx + 1))
                .unwrap_or_default();

            text.0 = format!("{:2}. {}{}", i + 1, spell_name, slot_label);
            color.0 = Color::srgb(0.4, 0.6, 1.0);
        } else {
            text.0 = format!("{:2}. ---", i + 1);
            color.0 = Color::srgb(0.3, 0.3, 0.3);
        }

        if i == selected.0 && i < spell_count {
            if !text.0.starts_with('>') {
                text.0 = format!("> {}", text.0.trim_start_matches("> "));
            }
        } else if text.0.starts_with("> ") {
            text.0 = text.0[2..].to_string();
        }
    }

    // Detail panel
    if let Ok((mut text, mut color)) = detail_text.single_mut() {
        if let Some(spell_id) = known.spells.get(selected.0) {
            let slot_label = active
                .slots
                .iter()
                .enumerate()
                .find(|(_, s)| s.as_deref() == Some(spell_id.as_str()))
                .map(|(idx, _)| format!("Active slot: {}", idx + 1))
                .unwrap_or_else(|| "Not assigned".to_string());

            if let Some(spell) = registry.and_then(|r| r.spells.get(spell_id)) {
                text.0 = format!(
                    "{}\n\nMana cost: {}/{} MP\n{}\n\n{}\n\nPress 1–6 to assign to a slot.",
                    spell.name, spell.mana_cost, mana.max, slot_label, spell.description,
                );
                color.0 = Color::srgb(0.4, 0.6, 1.0);
            } else {
                text.0 = format!("{}\n\n{}", spell_id, slot_label);
                color.0 = Color::srgb(0.4, 0.6, 1.0);
            }
        } else {
            text.0 = "Select a spell to see its details.\n\nPress 1–6 to assign\nthe highlighted spell to a slot.".to_string();
            color.0 = Color::srgb(0.6, 0.6, 0.6);
        }
    }
}

fn despawn_spells_ui(mut commands: Commands, query: Query<Entity, With<OnSpellsScreen>>) {
    for entity in query.iter() {
        commands.entity(entity).despawn();
    }
}
