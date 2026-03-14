use bevy::prelude::*;

use crate::assets::SpellRegistryHandle;
use crate::components::{GameEntityMarker, Monster, Name, Position};
use crate::constants::TILE_SIZE_X;
use crate::game::abilities::{
    BaseArmor, Cowardly, DeathCurse, DeathCurseEffect, EnrageOnHit, ExplodeOnDeath, OnHitEffect,
    OnHitEffects, PoisonBody, Reanimate, SummonOnDeath, ThornAura,
};
use crate::game::actions::SpeedStats;
use crate::game::camera::{MainCamera, UiCamera};
use crate::game::combat::{Damage, Health, HealthRegen};
use crate::game::magic::{
    Burning, Disarmed, Enraged, Hasted, KnownSpells, Poisoned, Slowed, SpiritShielded, Stunned,
    TimedModifiers,
};
use crate::game::spells::SpellRegistry;
use crate::game::AppState;
use crate::player::Player;
use crate::ui::nearby::NearbyState;

// --- Marker Components ---

#[derive(Component)]
pub struct MonsterInfoPanel;

#[derive(Component)]
struct MonsterInfoContent;

/// Marker for the sub-tooltip that shows ability/spell descriptions on hover.
#[derive(Component)]
struct InfoSubTooltip;

/// Stores the description text shown when the user hovers this row.
#[derive(Component)]
struct InfoRowDescription(String);

/// Tracks which entity the panel is currently showing and its last known HP,
/// to avoid rebuilding every frame.
#[derive(Component)]
struct PanelTarget {
    entity: Entity,
    last_hp: i32,
}

// --- Spawn ---

fn spawn_monster_info_panel(mut commands: Commands, q_ui_camera: Query<Entity, With<UiCamera>>) {
    let Ok(ui_camera) = q_ui_camera.single() else {
        return;
    };

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                max_width: Val::Px(260.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.90)),
            BorderColor::all(Color::WHITE),
            ZIndex(100),
            Visibility::Hidden,
            UiTargetCamera(ui_camera),
            MonsterInfoPanel,
            GameEntityMarker,
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                MonsterInfoContent,
            ));
        });

    // Sub-tooltip for hover descriptions
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                max_width: Val::Px(220.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.95)),
            BorderColor::all(Color::srgb(0.6, 0.6, 0.7)),
            ZIndex(110),
            Visibility::Hidden,
            UiTargetCamera(ui_camera),
            InfoSubTooltip,
            GameEntityMarker,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(""),
                TextFont::default(),
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
        });
}

// --- Ability/Spell Description Helpers ---

fn on_hit_effect_name(effect: &OnHitEffect) -> &'static str {
    match effect {
        OnHitEffect::ApplyPoison { .. } => "Poison on Hit",
        OnHitEffect::ApplySlow { .. } => "Slow on Hit",
        OnHitEffect::ApplyStun { .. } => "Stun on Hit",
        OnHitEffect::AttributeDrain { .. } => "Attribute Drain",
        OnHitEffect::LifeDrain { .. } => "Life Drain",
        OnHitEffect::Knockback { .. } => "Knockback",
        OnHitEffect::ApplyBurning { .. } => "Burning on Hit",
        OnHitEffect::Disarm { .. } => "Disarm",
    }
}

fn on_hit_effect_description(effect: &OnHitEffect) -> String {
    match effect {
        OnHitEffect::ApplyPoison {
            damage_per_turn,
            duration,
            chance,
        } => format!(
            "{}% chance: Poison ({}/turn, {} turns)",
            chance, damage_per_turn, duration
        ),
        OnHitEffect::ApplySlow { duration, chance } => {
            format!("{}% chance: Slow for {} turns", chance, duration)
        }
        OnHitEffect::ApplyStun { duration, chance } => {
            format!("{}% chance: Stun for {} turns", chance, duration)
        }
        OnHitEffect::AttributeDrain {
            attribute,
            amount,
            duration,
            chance,
        } => format!(
            "{}% chance: Drain {} {} for {} turns",
            chance, attribute, amount, duration
        ),
        OnHitEffect::LifeDrain { amount, chance } => {
            format!("{}% chance: Drain {} HP", chance, amount)
        }
        OnHitEffect::Knockback { distance, chance } => {
            format!("{}% chance: Knockback {} tiles", chance, distance)
        }
        OnHitEffect::ApplyBurning {
            damage_per_turn,
            duration,
            chance,
        } => format!(
            "{}% chance: Burn ({} fire/turn, {} turns)",
            chance, damage_per_turn, duration
        ),
        OnHitEffect::Disarm { duration, chance } => {
            format!("{}% chance: Disarm for {} turns", chance, duration)
        }
    }
}

// --- Update System ---

/// Collected ability info for display.
struct AbilityEntry {
    name: String,
    description: String,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_monster_info_panel(
    mut commands: Commands,
    windows: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut q_panel: Query<
        (Entity, &mut Node, &mut Visibility, Option<&PanelTarget>),
        (With<MonsterInfoPanel>, Without<InfoSubTooltip>),
    >,
    q_content: Query<Entity, With<MonsterInfoContent>>,
    // Split into two queries to stay under Bevy's 15-element tuple limit:
    // Query 1: base stats + hover detection
    q_base: Query<
        (
            Entity,
            &Name,
            &Health,
            Option<&HealthRegen>,
            &Damage,
            Option<&SpeedStats>,
            Option<&BaseArmor>,
            &InheritedVisibility,
        ),
        Or<(With<Monster>, With<Player>)>,
    >,
    // Query 2: ability components (looked up by entity after focus is determined)
    q_abilities: Query<(
        Option<&Cowardly>,
        Option<&OnHitEffects>,
        Option<&ExplodeOnDeath>,
        Option<&Reanimate>,
        Option<&PoisonBody>,
        Option<&ThornAura>,
        Option<&EnrageOnHit>,
        Option<&DeathCurse>,
        Option<&SummonOnDeath>,
        Option<&KnownSpells>,
    )>,
    // Query 3: active status effects (looked up by entity after focus is determined)
    q_statuses: Query<(
        Option<&Poisoned>,
        Option<&Burning>,
        Option<&Slowed>,
        Option<&Hasted>,
        Option<&Stunned>,
        Option<&Enraged>,
        Option<&Disarmed>,
        Option<&SpiritShielded>,
        Option<&TimedModifiers>,
    )>,
    nearby_state: Res<NearbyState>,
    pos_query: Query<&Position>,
    asset_server: Res<AssetServer>,
    spell_registry_handle: Option<Res<SpellRegistryHandle>>,
    spell_registries: Res<Assets<SpellRegistry>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = q_camera.single() else {
        return;
    };
    let Ok((panel_entity, mut panel_node, mut panel_visibility, current_target)) =
        q_panel.single_mut()
    else {
        return;
    };
    let Ok(content_entity) = q_content.single() else {
        return;
    };

    // Determine focused entity: mouse hover takes priority, then nearby selection
    let mut focused_entity = None;
    let mut screen_position = None;

    // Use grid-based lookup instead of iterating all entities with AABB checks.
    // Convert mouse world position to grid coords and find matching entity.
    if let Some(screen_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            let grid_x = (world_pos.x / TILE_SIZE_X as f32 + 0.5).floor() as i32;
            let grid_y = (world_pos.y / TILE_SIZE_X as f32 + 0.5).floor() as i32;

            for (entity, _, _, _, _, _, _, visibility) in q_base.iter() {
                if !visibility.get() {
                    continue;
                }
                if let Ok(pos) = pos_query.get(entity) {
                    if pos.x == grid_x && pos.y == grid_y {
                        focused_entity = Some(entity);
                        let entity_world = Vec3::new(
                            pos.x as f32 * TILE_SIZE_X as f32,
                            pos.y as f32 * TILE_SIZE_X as f32,
                            0.0,
                        );
                        if let Ok(sp) = camera.world_to_viewport(camera_transform, entity_world) {
                            screen_position = Some(sp);
                        }
                        break;
                    }
                }
            }
        }
    }

    // Fallback: nearby list selection
    if focused_entity.is_none() {
        if let Some(idx) = nearby_state.selected_idx {
            if let Some(&entity) = nearby_state.entity_list.get(idx) {
                if q_base.get(entity).is_ok() {
                    focused_entity = Some(entity);
                    if let Ok(pos) = pos_query.get(entity) {
                        let entity_world = Vec3::new(
                            pos.x as f32 * TILE_SIZE_X as f32,
                            pos.y as f32 * TILE_SIZE_X as f32 + TILE_SIZE_X as f32 * 0.5,
                            0.0,
                        );
                        if let Ok(sp) = camera.world_to_viewport(camera_transform, entity_world) {
                            screen_position = Some(sp);
                        }
                    }
                }
            }
        }
    }

    let Some(entity) = focused_entity else {
        *panel_visibility = Visibility::Hidden;
        panel_node.display = Display::None;
        commands.entity(panel_entity).remove::<PanelTarget>();
        return;
    };

    let Ok((_, name, health, regen, damage, speed_stats, base_armor, _)) = q_base.get(entity)
    else {
        *panel_visibility = Visibility::Hidden;
        panel_node.display = Display::None;
        return;
    };

    // Show and position
    *panel_visibility = Visibility::Visible;
    panel_node.display = Display::Flex;

    if let Some(sp) = screen_position {
        panel_node.left = Val::Px(sp.x + 18.0);
        panel_node.top = Val::Px(sp.y - 18.0);
    }

    // Skip full rebuild if we're already showing this entity with the same HP
    if let Some(target) = current_target {
        if target.entity == entity && target.last_hp == health.current {
            return;
        }
    }

    // Clear existing content children and update tracking
    commands.entity(content_entity).despawn_related::<Children>();
    commands.entity(panel_entity).insert(PanelTarget {
        entity,
        last_hp: health.current,
    });

    let font: Handle<Font> = asset_server.load("fonts/Macondo-Regular.ttf");

    // Collect all data we need before the closure (to avoid borrow issues)
    let name_str = name.0.clone();
    let health_current = health.current;
    let health_max = health.max;
    let regen_rate = regen.map(|r| r.regen_rate);
    let damage_str = damage.0.clone();
    let speed_delay = speed_stats.map(|s| s.delay);
    let armor_val = base_armor.map(|a| a.0).unwrap_or(0);

    // Collect abilities
    let mut ability_entries: Vec<AbilityEntry> = Vec::new();
    let mut spell_entries: Vec<(String, String)> = Vec::new();

    if let Ok((
        cowardly,
        on_hit_effects,
        explode_on_death,
        reanimate,
        poison_body,
        thorn_aura,
        enrage_on_hit,
        death_curse,
        summon_on_death,
        known_spells,
    )) = q_abilities.get(entity)
    {
        if cowardly.is_some() {
            ability_entries.push(AbilityEntry {
                name: "Cowardly".into(),
                description: "Flees when below 50% HP".into(),
            });
        }
        if let Some(effects) = on_hit_effects {
            for effect in &effects.0 {
                ability_entries.push(AbilityEntry {
                    name: on_hit_effect_name(effect).to_string(),
                    description: on_hit_effect_description(effect),
                });
            }
        }
        if let Some(e) = explode_on_death {
            ability_entries.push(AbilityEntry {
                name: "Explode on Death".into(),
                description: format!(
                    "Deals {} fire damage in {}-tile radius on death",
                    e.damage, e.radius
                ),
            });
        }
        if let Some(r) = reanimate {
            ability_entries.push(AbilityEntry {
                name: "Reanimate".into(),
                description: format!("Revives once with {} HP after death", r.revive_hp),
            });
        }
        if let Some(p) = poison_body {
            ability_entries.push(AbilityEntry {
                name: "Poison Body".into(),
                description: format!("Poisons melee attackers ({} dmg/turn)", p.stacks),
            });
        }
        if let Some(t) = thorn_aura {
            ability_entries.push(AbilityEntry {
                name: "Thorn Aura".into(),
                description: format!("Reflects {} damage to melee attackers", t.damage),
            });
        }
        if let Some(e) = enrage_on_hit {
            ability_entries.push(AbilityEntry {
                name: "Berserk".into(),
                description: format!("Enrages at {}% HP (+50% damage)", e.threshold_percent),
            });
        }
        if let Some(dc) = death_curse {
            let desc = match &dc.effect {
                DeathCurseEffect::Slow { duration } => {
                    format!("Curses killer with Slow for {} turns on death", duration)
                }
                DeathCurseEffect::Poison {
                    damage_per_turn,
                    duration,
                } => format!(
                    "Curses killer with Poison ({}/turn, {} turns) on death",
                    damage_per_turn, duration
                ),
                DeathCurseEffect::WeakenStr { amount, duration } => format!(
                    "Weakens killer's strength by {} for {} turns on death",
                    amount, duration
                ),
            };
            ability_entries.push(AbilityEntry {
                name: "Death Curse".into(),
                description: desc,
            });
        }
        if let Some(s) = summon_on_death {
            ability_entries.push(AbilityEntry {
                name: "Summon on Death".into(),
                description: format!("Summons {} {} on death", s.count, s.monster_name),
            });
        }

        // Spells
        if let Some(spells) = known_spells {
            let registry = spell_registry_handle
                .as_ref()
                .and_then(|h| spell_registries.get(&h.0));

            for spell_id in &spells.spells {
                let (spell_name, spell_desc) = if let Some(reg) = registry {
                    if let Some(data) = reg.spells.get(spell_id) {
                        (data.name.clone(), data.description.clone())
                    } else {
                        (spell_id.clone(), String::new())
                    }
                } else {
                    (spell_id.clone(), String::new())
                };
                spell_entries.push((spell_name, spell_desc));
            }
        }
    }

    // Collect active status effects
    let status_effects = if let Ok((poisoned, burning, slowed, hasted, stunned, enraged, disarmed, spirit_shielded, timed_modifiers)) = q_statuses.get(entity) {
        crate::ui::collect_status_effects(
            poisoned, burning, slowed, hasted, stunned, enraged, disarmed, spirit_shielded, timed_modifiers,
        )
    } else {
        Vec::new()
    };

    // Build UI
    commands.entity(content_entity).with_children(|parent| {
        // Name
        parent.spawn((
            Text::new(name_str),
            TextFont {
                font: font.clone(),
                font_size: 16.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ));

        // Health
        let mut health_str = format!("HP: {}/{}", health_current, health_max);
        if let Some(rate) = regen_rate {
            if rate > 0 {
                if rate >= 100 {
                    health_str.push_str(&format!(" (+{}/t)", rate / 100));
                } else {
                    health_str.push_str(&format!(" (+1/{}t)", 100 / rate));
                }
            }
        }
        parent.spawn((
            Text::new(health_str),
            TextFont {
                font: font.clone(),
                font_size: 13.0,
                ..default()
            },
            TextColor(Color::srgb(0.8, 0.8, 0.8)),
        ));

        // Damage
        parent.spawn((
            Text::new(format!("Damage: {}", damage_str)),
            TextFont {
                font: font.clone(),
                font_size: 13.0,
                ..default()
            },
            TextColor(Color::srgb(0.8, 0.8, 0.8)),
        ));

        // Speed trait
        if let Some(delay) = speed_delay {
            if let Some((label, color)) = super::get_speed_trait(delay, "Action") {
                parent.spawn((
                    Text::new(label),
                    TextFont {
                        font: font.clone(),
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(color),
                ));
            }
        }

        // Armor
        if armor_val > 0 {
            parent.spawn((
                Text::new(format!("Armor: {}", armor_val)),
                TextFont {
                    font: font.clone(),
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.9)),
            ));
        }

        // Abilities
        if !ability_entries.is_empty() {
            parent.spawn((
                Text::new("Abilities:"),
                TextFont {
                    font: font.clone(),
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.85, 0.3)),
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    ..default()
                },
            ));
            for entry in &ability_entries {
                parent
                    .spawn((
                        Node {
                            padding: UiRect::left(Val::Px(8.0)),
                            ..default()
                        },
                        Interaction::None,
                        InfoRowDescription(entry.description.clone()),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Text::new(format!("- {}", entry.name)),
                            TextFont {
                                font: font.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.85, 0.85, 0.85)),
                        ));
                    });
            }
        }

        // Spells
        if !spell_entries.is_empty() {
            parent.spawn((
                Text::new("Spells:"),
                TextFont {
                    font: font.clone(),
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.4, 0.8, 1.0)),
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    ..default()
                },
            ));
            for (spell_name, spell_desc) in &spell_entries {
                parent
                    .spawn((
                        Node {
                            padding: UiRect::left(Val::Px(8.0)),
                            ..default()
                        },
                        Interaction::None,
                        InfoRowDescription(spell_desc.clone()),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Text::new(format!("- {}", spell_name)),
                            TextFont {
                                font: font.clone(),
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.85, 0.85, 0.85)),
                        ));
                    });
            }
        }

        // Active status effects
        if !status_effects.is_empty() {
            parent.spawn((
                Text::new("Status:"),
                TextFont {
                    font: font.clone(),
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.7, 0.9)),
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    ..default()
                },
            ));
            // Wrap badges in a row container
            parent.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(4.0),
                row_gap: Val::Px(2.0),
                ..default()
            }).with_children(|row| {
                for (label, color) in &status_effects {
                    row.spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(3.0), Val::Px(1.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::all(*color),
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                    )).with_children(|badge| {
                        badge.spawn((
                            Text::new(label.clone()),
                            TextFont {
                                font: font.clone(),
                                font_size: 10.0,
                                ..default()
                            },
                            TextColor(*color),
                        ));
                    });
                }
            });
        }
    });
}

// --- Sub-tooltip hover system ---

#[allow(clippy::type_complexity)]
fn update_info_sub_tooltip(
    q_rows: Query<(&Interaction, &InfoRowDescription, &GlobalTransform)>,
    mut q_tooltip: Query<
        (&mut Node, &mut Visibility),
        (With<InfoSubTooltip>, Without<InfoRowDescription>),
    >,
    mut q_tooltip_text: Query<&mut Text, With<InfoSubTooltip>>,
) {
    let Ok((mut tooltip_node, mut tooltip_vis)) = q_tooltip.single_mut() else {
        return;
    };

    let mut found_hover = false;

    for (interaction, desc, global_transform) in q_rows.iter() {
        if *interaction == Interaction::Hovered && !desc.0.is_empty() {
            found_hover = true;

            *tooltip_vis = Visibility::Visible;
            tooltip_node.display = Display::Flex;

            // Position near the hovered row
            let pos = global_transform.translation();
            tooltip_node.left = Val::Px(pos.x + 80.0);
            tooltip_node.top = Val::Px(pos.y.max(0.0));

            // Update text
            if let Ok(mut text) = q_tooltip_text.single_mut() {
                text.0 = desc.0.clone();
            }

            break;
        }
    }

    if !found_hover {
        *tooltip_vis = Visibility::Hidden;
        tooltip_node.display = Display::None;
    }
}

// --- Plugin ---

pub struct MonsterInfoPlugin;

impl Plugin for MonsterInfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_monster_info_panel)
            .add_systems(
                Update,
                (update_monster_info_panel, update_info_sub_tooltip)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
