use bevy::prelude::*;

use crate::assets::SpellRegistryHandle;
use crate::components::{GameEntityMarker, Monster, Name, Position};
use crate::constants::TILE_SIZE_X;
use crate::game::actions::SpeedStats;
use crate::game::camera::{MainCamera, UiCamera};
use crate::game::combat::{Damage, Health, HealthRegen};
use crate::game::magic::{
    Burning, Hasted, KnownSpells, Poisoned, Slowed, Stunned,
};
use crate::game::stats::Armor;
use crate::game::spells::SpellRegistry;
use crate::game::AppState;
use crate::player::Player;
use crate::ui::nearby::NearbyState;

// --- Marker Components ---

#[derive(Component)]
pub struct MonsterInfoPanel;

#[derive(Component)]
struct MonsterInfoContent;

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

}

// --- Update System ---

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_monster_info_panel(
    mut commands: Commands,
    windows: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut q_panel: Query<
        (Entity, &mut Node, &mut Visibility, Option<&PanelTarget>),
        With<MonsterInfoPanel>,
    >,
    q_content: Query<Entity, With<MonsterInfoContent>>,
    // Query 1: base stats + hover detection
    q_base: Query<
        (
            Entity,
            &Name,
            &Health,
            Option<&HealthRegen>,
            &Damage,
            Option<&SpeedStats>,
            Option<&Armor>,
            &InheritedVisibility,
        ),
        Or<(With<Monster>, With<Player>)>,
    >,
    // Query 2: spells (looked up by entity after focus is determined)
    q_spells: Query<Option<&KnownSpells>>,
    // Query 3: active status effects (looked up by entity after focus is determined)
    q_statuses: Query<(
        Option<&Poisoned>,
        Option<&Burning>,
        Option<&Slowed>,
        Option<&Hasted>,
        Option<&Stunned>,
    )>,
    nearby_state: Res<NearbyState>,
    pos_query: Query<(Entity, &Position), Or<(With<Monster>, With<Player>)>>,
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

    // Grid-based lookup: convert mouse to grid coords, then find a matching
    // Monster/Player entity at that position. Only iterates pos_query (lightweight)
    // instead of unpacking all q_base components for every entity.
    if let Some(screen_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            let grid_x = (world_pos.x / TILE_SIZE_X as f32 + 0.5).floor() as i32;
            let grid_y = (world_pos.y / TILE_SIZE_X as f32 + 0.5).floor() as i32;

            for (entity, pos) in pos_query.iter() {
                if pos.x == grid_x && pos.y == grid_y {
                    // Check that it's a visible Monster or Player with base stats
                    if let Ok((_, _, _, _, _, _, _, visibility)) = q_base.get(entity) {
                        if visibility.get() {
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
    }

    // Fallback: nearby list selection
    if focused_entity.is_none() {
        if let Some(idx) = nearby_state.selected_idx {
            if let Some(&entity) = nearby_state.entity_list.get(idx) {
                if q_base.get(entity).is_ok() {
                    focused_entity = Some(entity);
                    if let Ok((_, pos)) = pos_query.get(entity) {
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

    let Ok((_, name, health, regen, damage, speed_stats, armor, _)) = q_base.get(entity)
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
    let armor_val = armor.map(|a| a.0).unwrap_or(0);

    // Collect spells
    let mut spell_entries: Vec<(String, String)> = Vec::new();

    if let Ok(known_spells) = q_spells.get(entity) {
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
    let status_effects = if let Ok((poisoned, burning, slowed, hasted, stunned)) = q_statuses.get(entity) {
        crate::ui::collect_status_effects(
            poisoned, burning, slowed, hasted, stunned,
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
            for (spell_name, _spell_desc) in &spell_entries {
                parent.spawn((
                    Text::new(format!("- {}", spell_name)),
                    TextFont {
                        font: font.clone(),
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.85, 0.85)),
                    Node {
                        padding: UiRect::left(Val::Px(8.0)),
                        ..default()
                    },
                ));
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

// Sub-tooltip hover is disabled to avoid Bevy picking system overhead.
// Ability descriptions are shown inline in the panel instead.

// --- Plugin ---

pub struct MonsterInfoPlugin;

impl Plugin for MonsterInfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_monster_info_panel)
            .add_systems(
                Update,
                update_monster_info_panel
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
