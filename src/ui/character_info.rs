use crate::game::AppState;
use crate::game::InGameState;
use crate::game::combat::{Damage, Health};
use crate::game::level::{AvailableStatPoints, Experience};
use crate::game::stats::{Attributes, CombatStats, Level, Mana, RolledHp};
use crate::player::Player;
use bevy::prelude::*;

pub struct CharacterInfoPlugin;

impl Plugin for CharacterInfoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StatDraft>()
            .add_systems(
                Update,
                character_info_input_system.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                OnEnter(InGameState::CharacterInfo),
                (spawn_character_info_ui, reset_stat_draft),
            )
            .add_systems(
                Update,
                (handle_allocation_buttons, update_character_info_ui)
                    .run_if(in_state(InGameState::CharacterInfo)),
            )
            .add_systems(
                OnExit(InGameState::CharacterInfo),
                despawn_character_info_ui,
            );
    }
}

#[derive(Component)]
pub struct OnCharacterInfoScreen;

#[allow(dead_code)]
#[derive(Component)]
pub struct HotkeyBar;

#[derive(Resource, Default, Clone, Copy)]
pub struct StatDraft {
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub agility: i32,
    pub intelligence: i32,
    pub perception: i32,
}

impl StatDraft {
    pub fn total_points(&self) -> u32 {
        (self.strength
            + self.dexterity
            + self.constitution
            + self.agility
            + self.intelligence
            + self.perception) as u32
    }
}

#[derive(Component)]
pub enum AllocationAction {
    PlusStrength,
    MinusStrength,
    PlusDexterity,
    MinusDexterity,
    PlusConstitution,
    MinusConstitution,
    PlusAgility,
    MinusAgility,
    PlusIntelligence,
    MinusIntelligence,
    PlusPerception,
    MinusPerception,
    Confirm,
}

// Marker components for updating text
#[derive(Component)]
pub struct XpText;
#[derive(Component)]
pub struct StatPointsText;
#[derive(Component)]
pub struct AttrText(pub crate::game::stats::Attributes); // Which attribute this text displays
#[derive(Component)]
pub struct CombatStatText;

// Marker components for showing/hiding buttons
#[derive(Component)]
pub struct StatPlusButton;
#[derive(Component)]
pub struct StatMinusButton(pub crate::game::stats::Attributes);
#[derive(Component)]
pub struct StatConfirmButton;

fn character_info_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    if keys.just_pressed(KeyCode::KeyC) {
        match state.get() {
            InGameState::Running => next_state.set(InGameState::CharacterInfo),
            InGameState::CharacterInfo => next_state.set(InGameState::Running),
            _ => {} // C does nothing while another screen is open
        }
    }

    if keys.just_pressed(KeyCode::Escape) && *state.get() == InGameState::CharacterInfo {
        next_state.set(InGameState::Running);
    }
}

fn reset_stat_draft(mut draft: ResMut<StatDraft>) {
    *draft = StatDraft::default();
}

fn spawn_character_info_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    player_query: Query<
        (
            &Health,
            &Attributes,
            &CombatStats,
            &Level,
            &Damage,
            &Experience,
            &AvailableStatPoints,
        ),
        With<Player>,
    >,
) {
    let Ok((health, _attrs, _stats, level, _damage, exp, points)) = player_query.single() else {
        return;
    };

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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
            ZIndex(200),
            OnCharacterInfoScreen,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(500.0),
                        padding: UiRect::all(Val::Px(20.0)),
                        flex_direction: FlexDirection::Column,
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::BLACK),
                    BorderColor::all(Color::WHITE),
                ))
                .with_children(|parent| {
                    // Header
                    parent.spawn((
                        Text::new("CHARACTER INFO"),
                        TextFont {
                            font: font.clone(),
                            font_size: 32.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.84, 0.0).into()), // Gold
                    ));

                    // Level, Health, XP
                    parent.spawn((
                        Text::new(format!(
                            "Level: {} | HP: {}/{}",
                            level.value, health.current, health.max
                        )),
                        TextFont {
                            font: font.clone(),
                            font_size: 24.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));

                    parent.spawn((
                        Text::new(format!("XP: {} / {}", exp.current, exp.next_level)),
                        TextFont {
                            font: font.clone(),
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.8, 0.8, 0.8).into()),
                        XpText,
                    ));

                    parent.spawn(Node {
                        height: Val::Px(10.0),
                        ..default()
                    });

                    // Stat Points
                    parent.spawn((
                        Text::new(format!("Available Stat Points: {}", points.0)),
                        TextFont {
                            font: font.clone(),
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.0, 1.0, 1.0).into()), // Cyan
                        StatPointsText,
                    ));

                    parent.spawn(Node {
                        height: Val::Px(10.0),
                        ..default()
                    });

                    // Attributes Section
                    parent.spawn((
                        Text::new("ATTRIBUTES"),
                        TextFont {
                            font: font.clone(),
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.7, 0.7, 1.0).into()),
                    ));

                    spawn_attribute_row(
                        parent,
                        "Strength",
                        font.clone(),
                        AllocationAction::PlusStrength,
                        AllocationAction::MinusStrength,
                    );
                    spawn_attribute_row(
                        parent,
                        "Dexterity",
                        font.clone(),
                        AllocationAction::PlusDexterity,
                        AllocationAction::MinusDexterity,
                    );
                    spawn_attribute_row(
                        parent,
                        "Constitution",
                        font.clone(),
                        AllocationAction::PlusConstitution,
                        AllocationAction::MinusConstitution,
                    );
                    spawn_attribute_row(
                        parent,
                        "Agility",
                        font.clone(),
                        AllocationAction::PlusAgility,
                        AllocationAction::MinusAgility,
                    );
                    spawn_attribute_row(
                        parent,
                        "Intelligence",
                        font.clone(),
                        AllocationAction::PlusIntelligence,
                        AllocationAction::MinusIntelligence,
                    );
                    spawn_attribute_row(
                        parent,
                        "Perception",
                        font.clone(),
                        AllocationAction::PlusPerception,
                        AllocationAction::MinusPerception,
                    );

                    parent.spawn(Node {
                        height: Val::Px(10.0),
                        ..default()
                    });

                    // Combat Stats Section
                    parent.spawn((
                        Text::new("COMBAT STATS (Preview)"),
                        TextFont {
                            font: font.clone(),
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.7, 0.7).into()),
                    ));

                    parent.spawn((
                        Text::new(""),
                        TextFont {
                            font: font.clone(),
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        CombatStatText,
                    ));

                    // Confirm Button
                    parent
                        .spawn((
                            Node {
                                height: Val::Px(40.0),
                                margin: UiRect::top(Val::Px(20.0)),
                                display: Display::None, // Hidden by default
                                ..default()
                            },
                            StatConfirmButton,
                        ))
                        .with_children(|p| {
                            p.spawn((
                                Button,
                                Node {
                                    width: Val::Px(120.0),
                                    height: Val::Px(40.0),
                                    border: UiRect::all(Val::Px(2.0)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.2, 0.5, 0.2)),
                                BorderColor::all(Color::WHITE),
                                AllocationAction::Confirm,
                            ))
                            .with_children(|b| {
                                b.spawn((
                                    Text::new("Confirm"),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 20.0,
                                        ..default()
                                    },
                                    TextColor(Color::WHITE),
                                ));
                            });
                        });

                    // Footer
                    parent.spawn(Node {
                        height: Val::Px(20.0),
                        ..default()
                    });
                    parent.spawn((
                        Text::new("Press (C) to Close"),
                        TextFont {
                            font: font.clone(),
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.5, 0.5, 0.5).into()), // Gray
                    ));
                });
        });
}

fn spawn_attribute_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    font: Handle<Font>,
    plus: AllocationAction,
    minus: AllocationAction,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(2.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(format!("{}: 10", label)),
                TextFont {
                    font: font.clone(),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                AttrText(match label {
                    "Strength" => Attributes {
                        strength: 1,
                        ..default()
                    },
                    "Dexterity" => Attributes {
                        dexterity: 1,
                        ..default()
                    },
                    "Constitution" => Attributes {
                        constitution: 1,
                        ..default()
                    },
                    "Agility" => Attributes {
                        agility: 1,
                        ..default()
                    },
                    "Intelligence" => Attributes {
                        intelligence: 1,
                        ..default()
                    },
                    "Perception" => Attributes {
                        perception: 1,
                        ..default()
                    },
                    _ => default(),
                }),
            ));

            row.spawn(Node {
                width: Val::Px(20.0),
                ..default()
            });

            // Minus Button
            row.spawn((
                Button,
                Node {
                    width: Val::Px(25.0),
                    height: Val::Px(25.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    display: Display::None, // Hidden by default
                    ..default()
                },
                BackgroundColor(Color::srgb(0.5, 0.2, 0.2)),
                minus,
                StatMinusButton(match label {
                    "Strength" => Attributes {
                        strength: 1,
                        ..default()
                    },
                    "Dexterity" => Attributes {
                        dexterity: 1,
                        ..default()
                    },
                    "Constitution" => Attributes {
                        constitution: 1,
                        ..default()
                    },
                    "Agility" => Attributes {
                        agility: 1,
                        ..default()
                    },
                    "Intelligence" => Attributes {
                        intelligence: 1,
                        ..default()
                    },
                    "Perception" => Attributes {
                        perception: 1,
                        ..default()
                    },
                    _ => default(),
                }),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("-"),
                    TextFont {
                        font: font.clone(),
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });

            row.spawn(Node {
                width: Val::Px(10.0),
                ..default()
            });

            // Plus Button
            row.spawn((
                Button,
                Node {
                    width: Val::Px(25.0),
                    height: Val::Px(25.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    display: Display::None, // Hidden by default
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.5, 0.2)),
                plus,
                StatPlusButton,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("+"),
                    TextFont {
                        font: font.clone(),
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

fn handle_allocation_buttons(
    mut interaction_query: Query<
        (&Interaction, &AllocationAction),
        (Changed<Interaction>, With<Button>),
    >,
    mut draft: ResMut<StatDraft>,
    mut player_query: Query<(&mut Attributes, &mut AvailableStatPoints), With<Player>>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    let Ok((mut player_attrs, mut points)) = player_query.single_mut() else {
        return;
    };

    for (interaction, action) in &mut interaction_query {
        if *interaction == Interaction::Pressed {
            match action {
                AllocationAction::PlusStrength => {
                    if draft.total_points() < points.0 {
                        draft.strength += 1;
                    }
                }
                AllocationAction::MinusStrength => {
                    if draft.strength > 0 {
                        draft.strength -= 1;
                    }
                }
                AllocationAction::PlusDexterity => {
                    if draft.total_points() < points.0 {
                        draft.dexterity += 1;
                    }
                }
                AllocationAction::MinusDexterity => {
                    if draft.dexterity > 0 {
                        draft.dexterity -= 1;
                    }
                }
                AllocationAction::PlusConstitution => {
                    if draft.total_points() < points.0 {
                        draft.constitution += 1;
                    }
                }
                AllocationAction::MinusConstitution => {
                    if draft.constitution > 0 {
                        draft.constitution -= 1;
                    }
                }
                AllocationAction::PlusAgility => {
                    if draft.total_points() < points.0 {
                        draft.agility += 1;
                    }
                }
                AllocationAction::MinusAgility => {
                    if draft.agility > 0 {
                        draft.agility -= 1;
                    }
                }
                AllocationAction::PlusIntelligence => {
                    if draft.total_points() < points.0 {
                        draft.intelligence += 1;
                    }
                }
                AllocationAction::MinusIntelligence => {
                    if draft.intelligence > 0 {
                        draft.intelligence -= 1;
                    }
                }
                AllocationAction::PlusPerception => {
                    if draft.total_points() < points.0 {
                        draft.perception += 1;
                    }
                }
                AllocationAction::MinusPerception => {
                    if draft.perception > 0 {
                        draft.perception -= 1;
                    }
                }
                AllocationAction::Confirm => {
                    player_attrs.strength += draft.strength;
                    player_attrs.dexterity += draft.dexterity;
                    player_attrs.constitution += draft.constitution;
                    player_attrs.agility += draft.agility;
                    player_attrs.intelligence += draft.intelligence;
                    player_attrs.perception += draft.perception;
                    points.0 -= draft.total_points();
                    *draft = StatDraft::default();
                    next_state.set(InGameState::Running);
                }
            }
        }
    }
}

fn update_character_info_ui(
    draft: Res<StatDraft>,
    player_query: Query<
        (
            &Attributes,
            &AvailableStatPoints,
            &Damage,
            &Level,
            &RolledHp,
            &Mana,
            &CombatStats,
        ),
        With<Player>,
    >,
    mut attr_texts: Query<(&mut Text, &AttrText), Without<StatPointsText>>,
    mut points_text: Query<&mut Text, (With<StatPointsText>, Without<AttrText>)>,
    mut combat_text: Query<
        &mut Text,
        (
            With<CombatStatText>,
            Without<AttrText>,
            Without<StatPointsText>,
        ),
    >,
    mut plus_buttons: Query<
        &mut Node,
        (
            With<StatPlusButton>,
            Without<StatMinusButton>,
            Without<StatConfirmButton>,
        ),
    >,
    mut minus_buttons: Query<
        (&mut Node, &StatMinusButton),
        (Without<StatPlusButton>, Without<StatConfirmButton>),
    >,
    mut confirm_button: Query<
        &mut Node,
        (
            With<StatConfirmButton>,
            Without<StatPlusButton>,
            Without<StatMinusButton>,
        ),
    >,
) {
    let Ok((player_attrs, points, damage, level, rolled_hp, mana, combat_stats)) =
        player_query.single()
    else {
        return;
    };

    let available_points = points.0 - draft.total_points();

    let Ok(mut points_t) = points_text.single_mut() else {
        return;
    };
    points_t.0 = format!("Available Stat Points: {}", available_points);

    // Update plus buttons visibility
    for mut node in &mut plus_buttons {
        node.display = if available_points > 0 {
            Display::Flex
        } else {
            Display::None
        };
    }

    // Update minus buttons visibility
    for (mut node, marker) in &mut minus_buttons {
        let draft_val = if marker.0.strength > 0 {
            draft.strength
        } else if marker.0.dexterity > 0 {
            draft.dexterity
        } else if marker.0.constitution > 0 {
            draft.constitution
        } else if marker.0.agility > 0 {
            draft.agility
        } else if marker.0.intelligence > 0 {
            draft.intelligence
        } else if marker.0.perception > 0 {
            draft.perception
        } else {
            0
        };
        node.display = if draft_val > 0 {
            Display::Flex
        } else {
            Display::None
        };
    }

    // Update confirm button visibility
    let Ok(mut confirm_node) = confirm_button.single_mut() else {
        return;
    };
    confirm_node.display = if draft.total_points() > 0 {
        Display::Flex
    } else {
        Display::None
    };

    for (mut text, attr_marker) in &mut attr_texts {
        if attr_marker.0.strength > 0 {
            text.0 = format!(
                "Strength:     {} (+{})",
                player_attrs.strength, draft.strength
            );
        } else if attr_marker.0.dexterity > 0 {
            text.0 = format!(
                "Dexterity:    {} (+{})",
                player_attrs.dexterity, draft.dexterity
            );
        } else if attr_marker.0.constitution > 0 {
            text.0 = format!(
                "Constitution: {} (+{})",
                player_attrs.constitution, draft.constitution
            );
        } else if attr_marker.0.agility > 0 {
            text.0 = format!(
                "Agility:      {} (+{})",
                player_attrs.agility, draft.agility
            );
        } else if attr_marker.0.intelligence > 0 {
            text.0 = format!(
                "Intelligence: {} (+{})",
                player_attrs.intelligence, draft.intelligence
            );
        } else if attr_marker.0.perception > 0 {
            text.0 = format!(
                "Perception:   {} (+{})",
                player_attrs.perception, draft.perception
            );
        }
    }

    if let Ok(mut text) = combat_text.single_mut() {
        let eff_str = player_attrs.strength + draft.strength;
        let eff_dex = player_attrs.dexterity + draft.dexterity;
        let eff_con = player_attrs.constitution + draft.constitution;
        let eff_agi = player_attrs.agility + draft.agility;
        let eff_int = player_attrs.intelligence + draft.intelligence;
        let eff_per = player_attrs.perception + draft.perception;

        let str_bonus = eff_str - 10;
        let dex_bonus = eff_dex - 10;
        let con_bonus = eff_con - 10;
        let agi_bonus = eff_agi - 10;
        let per_bonus = eff_per - 10;

        let max_hp = 10 + rolled_hp.0 + (con_bonus * level.value);
        let max_mana = eff_int * 5;
        let action_delay = (1.0f32 - (agi_bonus as f32 * 0.025)).clamp(0.5, 2.0);
        let vision_range = (8 + per_bonus).max(2);

        text.0 = format!(
            "Max HP:       {}\nMana:         {}/{}\nDamage:       {} + {}\nDefense:      {}\nHit Chance:   {}\nDodge Chance: {}\nAction Delay: {:.2}x\nVision Range: {} tiles",
            max_hp,
            mana.current,
            max_mana,
            damage.0,
            str_bonus,
            combat_stats.armor,
            10 + str_bonus,
            5 + dex_bonus,
            action_delay,
            vision_range
        );
    }
}

fn despawn_character_info_ui(
    mut commands: Commands,
    query: Query<Entity, With<OnCharacterInfoScreen>>,
) {
    for entity in query.iter() {
        commands.entity(entity).despawn();
    }
}
