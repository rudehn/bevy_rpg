use bevy::prelude::*;

use crate::game::combat::Resistances;
use crate::game::essence::Essence;
use crate::game::items::Rarity;
use crate::game::magic::ActiveSpells;
use crate::game::shrines::{ActiveShrine, ShrineData, ShrinesPurchased, apply_shrine_effect};
use crate::game::{AppState, InGameState};
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

pub struct ShrineUiPlugin;

impl Plugin for ShrineUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(InGameState::Shrine), spawn_shrine_ui)
            .add_systems(
                Update,
                shrine_input_system
                    .run_if(in_state(AppState::InGame).and(in_state(InGameState::Shrine))),
            )
            .add_systems(OnExit(InGameState::Shrine), cleanup_shrine_ui);
    }
}

// --- Marker components ---

#[derive(Component)]
struct OnShrineScreen;

// --- Systems ---

fn spawn_shrine_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    active_shrine: Option<Res<ActiveShrine>>,
    shrine_query: Query<&ShrineData>,
    player_query: Query<&Essence, With<Player>>,
) {
    let Some(active) = active_shrine else {
        return;
    };
    let Ok(shrine_data) = shrine_query.get(active.0) else {
        return;
    };
    let essence = player_query.single().map(|e| e.current).unwrap_or(0);

    let font: Handle<Font> = asset_server.load("fonts/Macondo-Regular.ttf");

    // Determine title and color based on category
    let (title, title_color): (&'static str, Color) = match shrine_data.category_id.as_str() {
        "war" => ("WAR SHRINE", Color::srgb(1.0, 0.3, 0.3)),
        "arcane" => ("ARCANE SHRINE", Color::srgb(0.4, 0.6, 1.0)),
        "fortune" => ("FORTUNE SHRINE", Color::srgb(1.0, 0.84, 0.0)),
        _ => ("SHRINE", Color::srgb(0.8, 0.8, 0.8)),
    };

    use crate::ui::modal::{spawn_modal, ModalConfig};
    spawn_modal(
        &mut commands,
        OnShrineScreen,
        &font,
        &ModalConfig {
            title,
            title_color,
            footer: "[1/2/3] Purchase  |  [Esc] Leave",
            width: 620.0,
            height: 420.0,
            ..Default::default()
        },
        |panel, font| {
            // Effect cards
            for (i, effect) in shrine_data.effects.iter().enumerate() {
                let rarity_color = rarity_color(&effect.rarity);
                let rarity_label = format!("{:?}", effect.rarity);
                let affordable = essence >= effect.cost;
                let cost_color = if affordable {
                    Color::srgb(0.6, 0.9, 0.6)
                } else {
                    Color::srgb(0.7, 0.3, 0.3)
                };

                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(8.0)),
                        margin: UiRect::bottom(Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    })
                    .insert(BorderColor::all(rarity_color))
                    .insert(BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.8)))
                    .with_children(|card| {
                        // Name line: [N] Effect Name (Rarity)
                        card.spawn((
                            Text::new(format!(
                                "[{}] {}  ({})  — {} essence",
                                i + 1,
                                effect.name,
                                rarity_label,
                                effect.cost,
                            )),
                            TextFont {
                                font: font.clone(),
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(rarity_color),
                        ));
                        // Description
                        card.spawn((
                            Text::new(&effect.description),
                            TextFont {
                                font: font.clone(),
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.7, 0.7, 0.7)),
                            Node {
                                margin: UiRect::top(Val::Px(2.0)),
                                ..default()
                            },
                        ));
                        // Cost
                        card.spawn((
                            Text::new(if affordable {
                                format!("Cost: {} essence", effect.cost)
                            } else {
                                format!("Cost: {} essence (not enough!)", effect.cost)
                            }),
                            TextFont {
                                font: font.clone(),
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(cost_color),
                            Node {
                                margin: UiRect::top(Val::Px(2.0)),
                                ..default()
                            },
                        ));
                    });
            }

            // Essence balance
            panel.spawn((
                Text::new(format!("Your Essence: {}", essence)),
                TextFont {
                    font: font.clone(),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.9, 0.6)),
                Node {
                    margin: UiRect::top(Val::Px(8.0)),
                    ..default()
                },
            ));
        },
    );
}

fn shrine_input_system(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut next_ingame: ResMut<NextState<InGameState>>,
    active_shrine: Option<Res<ActiveShrine>>,
    shrine_query: Query<&ShrineData>,
    mut player_query: Query<(Entity, &mut Essence, Option<&mut ActiveSpells>, Option<&mut Resistances>), With<Player>>,
    mut shrines_purchased: ResMut<ShrinesPurchased>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    // Escape to close
    if keys.just_pressed(KeyCode::Escape) {
        next_ingame.set(InGameState::Running);
        return;
    }

    let Some(active) = active_shrine else {
        return;
    };
    let Ok(shrine_data) = shrine_query.get(active.0).cloned() else {
        next_ingame.set(InGameState::Running);
        return;
    };

    let choice_keys = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3];
    let mut chosen_index = None;
    for (i, &key) in choice_keys.iter().enumerate() {
        if keys.just_pressed(key) {
            chosen_index = Some(i);
            break;
        }
    }

    let Some(idx) = chosen_index else {
        return;
    };

    let Some(effect) = shrine_data.effects.get(idx) else {
        return;
    };

    let Ok((player_entity, mut essence, active_spells, resistances)) = player_query.single_mut() else {
        return;
    };

    if essence.current < effect.cost {
        log_writer.write(GameLogMessage("Not enough essence.".to_string()));
        return;
    }

    // Deduct essence
    essence.current -= effect.cost;

    // Apply the effect
    let mut active_spells_val = active_spells.map(|a| a.into_inner().clone());
    let mut resistances_val = resistances.map(|r| r.into_inner().clone());

    apply_shrine_effect(
        &mut commands,
        player_entity,
        &effect.kind,
        active_spells_val.as_mut(),
        resistances_val.as_mut(),
    );

    // Write back modified components if needed
    if let Some(new_active) = active_spells_val {
        commands.entity(player_entity).insert(new_active);
    }
    if let Some(new_res) = resistances_val {
        commands.entity(player_entity).insert(new_res);
    }

    // Track purchase
    if effect.unique {
        shrines_purchased.0.push(effect.id.clone());
    }

    // Log
    log_writer.write(GameLogMessage(format!(
        "You purchase {} from the {} Shrine for {} essence.",
        effect.name, shrine_data.category_name, effect.cost
    )));

    // Despawn the shrine entity
    let shrine_entity = active.0;
    commands.entity(shrine_entity).despawn();

    // Close UI
    next_ingame.set(InGameState::Running);
}

fn cleanup_shrine_ui(
    mut commands: Commands,
    query: Query<Entity, With<OnShrineScreen>>,
) {
    for entity in query.iter() {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<ActiveShrine>();
}

fn rarity_color(rarity: &Rarity) -> Color {
    match rarity {
        Rarity::Common => Color::WHITE,
        Rarity::Uncommon => Color::srgb(0.3, 0.9, 0.3),
        Rarity::Rare => Color::srgb(0.3, 0.5, 1.0),
        Rarity::Legendary => Color::srgb(1.0, 0.6, 0.1),
    }
}
