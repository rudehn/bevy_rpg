use bevy::prelude::*;
use bracket_lib::prelude::Point;

use crate::character::Attributes;
use crate::components::{GameEntityMarker, Monster, Name, Position, Species, Viewshed};
use crate::constants::TILE_SIZE_X;
use crate::game::abilities::{
    BurningStrike, ExplodeOnDeath, ExplodeOnHit, Knockback, LifeDrain, PackTactics, Rally,
    RoughBody, SlowStrike, StunningBlow, SummonOnDeath, Terrify, WarCry,
};
use crate::game::actions::SpeedStats;
use crate::game::ai::{MonsterAI, MonsterAIMode};
use crate::game::camera::{MainCamera, UiCamera};
use crate::game::combat::{Damage, Health, HealthRegen};
use crate::game::items::{Equipment, ItemProperties};
use crate::game::magic::StatusEffects;
use crate::game::skills::Skills;
use crate::game::staves::MonsterAbilities;
use crate::game::stats::{Armor, DamageBonus};
use crate::game::stealth::MonsterPerception;
use crate::game::AppState;
use crate::map::Map;
use crate::map::light::LightMap;
use crate::player::Player;
use crate::ui::nearby::NearbyState;
use crate::ui::stealth_display::{
    light_intensity_at, player_armor_stealth_penalty, render_stealth_section, stealth_display_for,
};
use roguelike_engine::stealth::{Awareness, NoiseMap};

// --- Marker Components ---

#[derive(Component)]
pub struct MonsterInfoPanel;

#[derive(Component)]
struct MonsterInfoContent;

/// Bundled inputs needed to render the Stealth section. Pulled into a
/// single [`SystemParam`] to keep `update_monster_info_panel`'s
/// argument count under Bevy's 16-parameter ceiling.
#[derive(bevy::ecs::system::SystemParam)]
pub struct StealthDisplayParams<'w, 's> {
    pub player: Query<
        'w,
        's,
        (
            Entity,
            &'static Position,
            Option<&'static Skills>,
            Option<&'static Attributes>,
            Option<&'static Equipment>,
            Option<&'static Viewshed>,
        ),
        With<Player>,
    >,
    pub monsters: Query<
        'w,
        's,
        (
            Option<&'static MonsterPerception>,
            Option<&'static MonsterAI>,
            Option<&'static Awareness>,
            Option<&'static Viewshed>,
        ),
    >,
    pub item_props: Query<'w, 's, &'static ItemProperties>,
    pub light_map: Res<'w, LightMap>,
    pub map: Res<'w, Map>,
    pub noise_map: Res<'w, NoiseMap>,
}

/// Tracks which entity the panel is currently showing and a small set
/// of values that should force a rebuild when they change. HP drives
/// the bar refresh; the stealth headline keeps the "Notice this turn"
/// line in step with the monster's awareness state and the player's
/// stealth modifiers.
#[derive(Component)]
struct PanelTarget {
    entity: Entity,
    last_hp: i32,
    last_stealth_headline: String,
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, crate::ui::modal::MODAL_OVERLAY_OPACITY)),
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
    // Query 2: monster abilities (looked up by entity after focus is determined)
    q_abilities: Query<Option<&MonsterAbilities>>,
    // Query 2b: species tag (looked up by entity after focus is determined)
    q_species: Query<Option<&Species>>,
    // Query 3: active status effects (looked up by entity after focus is determined)
    q_statuses: Query<Option<&StatusEffects>>,
    // Query 4: ability traits
    q_traits: Query<(
        Has<BurningStrike>, Has<StunningBlow>, Has<LifeDrain>, Has<Knockback>,
        Has<SlowStrike>, Has<RoughBody>, Has<ExplodeOnDeath>, Has<ExplodeOnHit>,
        Has<SummonOnDeath>, Has<PackTactics>, Has<Rally>, Has<Terrify>, Has<WarCry>,
    )>,
    // Query 5: player stats for battle sim
    q_player_combat: Query<(&Health, &Damage, &Armor, &DamageBonus), With<Player>>,
    stealth_params: StealthDisplayParams,
    nearby_state: Res<NearbyState>,
    pos_query: Query<(Entity, &Position), Or<(With<Monster>, With<Player>, With<crate::components::Item>, With<crate::components::Prop>)>>,
    name_query: Query<&Name>,
    asset_server: Res<AssetServer>,
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
    if let Some(screen_pos) = window.cursor_position()
        && let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, screen_pos) {
            let grid_x = (world_pos.x / TILE_SIZE_X as f32 + 0.5).floor() as i32;
            let grid_y = (world_pos.y / TILE_SIZE_X as f32 + 0.5).floor() as i32;

            for (entity, pos) in pos_query.iter() {
                if pos.x == grid_x && pos.y == grid_y {
                    // Check visibility — try q_base first (monsters/player), fall back to
                    // checking InheritedVisibility directly (items/props)
                    let is_visible = if let Ok((_, _, _, _, _, _, _, visibility)) = q_base.get(entity) {
                        visibility.get()
                    } else {
                        // Items/props: check inherited visibility
                        true // They're visible if they exist at this position
                    };
                    if is_visible {
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

    // Fallback: nearby list selection (works for monsters, items, and props)
    if focused_entity.is_none()
        && let Some(idx) = nearby_state.selected_idx
            && let Some(&entity) = nearby_state.entity_list.get(idx) {
                {
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

    let Some(entity) = focused_entity else {
        *panel_visibility = Visibility::Hidden;
        panel_node.display = Display::None;
        commands.entity(panel_entity).remove::<PanelTarget>();
        return;
    };

    let Ok((_, name, health, regen, damage, speed_stats, armor, _)) = q_base.get(entity)
    else {
        // Not a monster/player — show a simple name-only tooltip for items/props
        if let Ok(item_name) = name_query.get(entity) {
            *panel_visibility = Visibility::Visible;
            panel_node.display = Display::Flex;

            if let Some(sp) = screen_position {
                panel_node.left = Val::Px(sp.x + 18.0);
                panel_node.top = Val::Px(sp.y - 18.0);
            }

            // Only rebuild if target changed
            let should_rebuild = current_target
                .map(|t| t.entity != entity)
                .unwrap_or(true);

            if should_rebuild {
                let font: Handle<Font> = asset_server.load("fonts/Macondo-Regular.ttf");
                commands.entity(panel_entity).insert(PanelTarget {
                    entity,
                    last_hp: 0,
                    last_stealth_headline: String::new(),
                });
                commands.entity(content_entity).despawn_related::<Children>();
                commands.entity(content_entity).with_children(|parent| {
                    parent.spawn((
                        Text::new(&item_name.0),
                        TextFont { font, font_size: 16.0, ..default() },
                        TextColor(Color::WHITE),
                    ));
                });
            }
        } else {
            *panel_visibility = Visibility::Hidden;
            panel_node.display = Display::None;
        }
        return;
    };

    // Show and position
    *panel_visibility = Visibility::Visible;
    panel_node.display = Display::Flex;

    if let Some(sp) = screen_position {
        panel_node.left = Val::Px(sp.x + 18.0);
        panel_node.top = Val::Px(sp.y - 18.0);
    }

    // Build the Stealth section lines before the rebuild guard so we
    // can fold the headline into the cache key. This keeps the
    // "Notice this turn" line live as the player moves, sneaks into
    // shadow, or the monster transitions Hidden → Searching → Aware.
    let is_player_focused = q_player_combat.get(entity).is_ok();
    let stealth_lines = if is_player_focused {
        None
    } else if let Ok((player_e, p_pos, p_skills, p_attrs, p_equip, p_viewshed)) =
        stealth_params.player.single()
    {
        let (mp, mai, m_aware, m_viewshed) = match stealth_params.monsters.get(entity) {
            Ok(v) => v,
            Err(_) => (None, None, None, None),
        };
        let monster_pos = match pos_query.get(entity) {
            Ok((_, pos)) => Point::new(pos.x, pos.y),
            Err(_) => Point::new(0, 0),
        };
        let player_point = Point::new(p_pos.x, p_pos.y);
        let monster_perception = mp.map(|p| p.0).unwrap_or(0);
        let is_asleep = mai
            .map(|a| a.mode == MonsterAIMode::Asleep)
            .unwrap_or(false);
        // The monster sees the player this turn iff the player's tile
        // is in the monster's viewshed. Fall back to the player's own
        // viewshed for symmetry if the monster has none (treat as
        // visible).
        let in_viewshed = m_viewshed
            .map(|vs| vs.visible_tiles.contains(&monster_pos))
            .or_else(|| p_viewshed.map(|vs| vs.visible_tiles.contains(&monster_pos)))
            .unwrap_or(true);
        let light_intensity = light_intensity_at(&stealth_params.light_map, &stealth_params.map, player_point);
        let armor_pen = player_armor_stealth_penalty(p_equip, &stealth_params.item_props);
        Some(stealth_display_for(
            monster_perception,
            monster_pos,
            is_asleep,
            in_viewshed,
            m_aware,
            player_e,
            player_point,
            p_skills,
            p_attrs,
            armor_pen,
            light_intensity,
            &stealth_params.noise_map,
        ))
    } else {
        None
    };
    let stealth_headline = stealth_lines
        .as_ref()
        .map(|l| l.headline.clone())
        .unwrap_or_default();

    // Skip full rebuild if entity, HP, and stealth headline are unchanged.
    if let Some(target) = current_target
        && target.entity == entity
        && target.last_hp == health.current
        && target.last_stealth_headline == stealth_headline
    {
        return;
    }

    // Clear existing content children and update tracking
    commands.entity(content_entity).despawn_related::<Children>();
    commands.entity(panel_entity).insert(PanelTarget {
        entity,
        last_hp: health.current,
        last_stealth_headline: stealth_headline,
    });

    let font: Handle<Font> = asset_server.load("fonts/Macondo-Regular.ttf");

    // Collect all data we need before the closure (to avoid borrow issues)
    let name_str = name.0.clone();
    let health_current = health.current;
    let health_max = health.max;
    let regen_rate = regen.map(|r| r.regen_rate);
    let damage_str = damage.0.clone();
    let speed_movement_delay = speed_stats.map(|s| s.movement_delay);
    let speed_attack_delay = speed_stats.map(|s| s.attack_delay);
    let armor_val = armor.map(|a| a.0).unwrap_or(0);

    // Species tag (biological category — Beast, Humanoid, Insect, ...)
    let species = q_species
        .get(entity)
        .ok()
        .and_then(|s| s.copied())
        .filter(|s| !matches!(s, Species::Unknown));

    // Collect monster abilities
    let mut ability_entries: Vec<String> = Vec::new();

    if let Ok(monster_abilities) = q_abilities.get(entity)
        && let Some(abilities) = monster_abilities {
            for ability in &abilities.0 {
                ability_entries.push(ability.name.clone());
            }
        }

    // Collect active status effects
    let status_effects = if let Ok(status) = q_statuses.get(entity) {
        crate::ui::collect_status_effects(status)
    } else {
        Vec::new()
    };

    // Collect ability traits
    let mut traits: Vec<&str> = Vec::new();
    if let Ok((burn, stun, drain, kb, slow, rough, explode_death, explode_hit, summon, pack, rally, terrify, warcry)) = q_traits.get(entity) {
        if burn { traits.push("Burning Strike"); }
        if stun { traits.push("Stunning Blow"); }
        if drain { traits.push("Life Drain"); }
        if kb { traits.push("Knockback"); }
        if slow { traits.push("Slowing Strike"); }
        if rough { traits.push("Rough Body"); }
        if explode_death { traits.push("Explodes on Death"); }
        if explode_hit { traits.push("Explodes on Hit"); }
        if summon { traits.push("Summons on Death"); }
        if pack { traits.push("Pack Tactics"); }
        if rally { traits.push("Rally Aura"); }
        if terrify { traits.push("Terrify Aura"); }
        if warcry { traits.push("War Cry"); }
    }

    // Battle sim: estimate turns to kill each other
    let is_player_entity = q_player_combat.get(entity).is_ok();
    let battle_estimate = if !is_player_entity {
        estimate_battle(
            health.current, &damage.0, armor_val,
            q_player_combat.single().ok(),
        )
    } else {
        None
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

        // Species (Beast / Humanoid / Insect / ... — hidden for Unknown/Player)
        if let Some(species) = species {
            parent.spawn((
                Text::new(format!("Species: {}", species)),
                TextFont {
                    font: font.clone(),
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.65, 0.65, 0.55)),
            ));
        }

        // Health
        let mut health_str = format!("HP: {}/{}", health_current, health_max);
        if let Some(rate) = regen_rate
            && rate > 0 {
                if rate >= 100 {
                    health_str.push_str(&format!(" (+{}/t)", rate / 100));
                } else {
                    health_str.push_str(&format!(" (+1/{}t)", 100 / rate));
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

        // Speed traits (movement + attack)
        if let Some(delay) = speed_movement_delay
            && let Some((label, color)) = super::get_speed_trait(delay, "Move") {
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
        if let Some(delay) = speed_attack_delay
            && let Some((label, color)) = super::get_speed_trait(delay, "Attack") {
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

        // Monster Abilities
        if !ability_entries.is_empty() {
            parent.spawn((
                Text::new("Abilities:"),
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
            for ability_name in &ability_entries {
                parent.spawn((
                    Text::new(format!("- {}", ability_name)),
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

        // Ability traits
        if !traits.is_empty() {
            parent.spawn((
                Text::new("Traits:"),
                TextFont {
                    font: font.clone(),
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.75, 0.5)),
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    ..default()
                },
            ));
            for t in &traits {
                parent.spawn((
                    Text::new(format!("- {}", t)),
                    TextFont {
                        font: font.clone(),
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.75, 0.55)),
                    Node {
                        padding: UiRect::left(Val::Px(8.0)),
                        ..default()
                    },
                ));
            }
        }

        // Battle estimate
        if let Some((player_ttk, monster_ttk)) = battle_estimate {
            let color = if player_ttk < monster_ttk {
                Color::srgb(0.3, 0.9, 0.3) // You win — green
            } else if player_ttk > monster_ttk {
                Color::srgb(0.9, 0.3, 0.3) // You lose — red
            } else {
                Color::srgb(0.9, 0.9, 0.3) // Close fight — yellow
            };
            parent.spawn((
                Text::new(format!("You kill in ~{} hits, it kills you in ~{}", player_ttk, monster_ttk)),
                TextFont {
                    font: font.clone(),
                    font_size: 11.0,
                    ..default()
                },
                TextColor(color),
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    ..default()
                },
            ));
        }

        // Stealth section — "Notice this turn: XX%" + per-modifier breakdown.
        // Suppressed when the focused entity is the player itself.
        if let Some(lines) = &stealth_lines {
            render_stealth_section(parent, font.clone(), lines);
        }
    });
}

/// Estimate a simple battle: returns (player_hits_to_kill_monster, monster_hits_to_kill_player).
fn estimate_battle(
    monster_hp: i32,
    monster_damage_dice: &str,
    monster_armor: i32,
    player_stats: Option<(&Health, &Damage, &Armor, &DamageBonus)>,
) -> Option<(i32, i32)> {
    let (player_hp, player_dmg, player_armor, player_dmg_bonus) = player_stats?;

    // Estimate average player damage per hit
    let player_avg = avg_dice(player_dmg.0.as_str()) + player_dmg_bonus.0 as f32;
    let player_effective = (player_avg - monster_armor as f32).max(1.0);
    let player_ttk = (monster_hp as f32 / player_effective).ceil() as i32;

    // Estimate average monster damage per hit
    let monster_avg = avg_dice(monster_damage_dice);
    let monster_effective = (monster_avg - player_armor.0 as f32).max(1.0);
    let monster_ttk = (player_hp.current as f32 / monster_effective).ceil() as i32;

    Some((player_ttk, monster_ttk))
}

fn avg_dice(dice_str: &str) -> f32 {
    let dice_str = dice_str.trim();
    let (dice_part, bonus) = if let Some(plus_idx) = dice_str.find('+') {
        let bonus: f32 = dice_str[plus_idx + 1..].trim().parse().unwrap_or(0.0);
        (&dice_str[..plus_idx], bonus)
    } else {
        (dice_str, 0.0)
    };

    if let Some(d_idx) = dice_part.find('d') {
        let n: f32 = dice_part[..d_idx].trim().parse().unwrap_or(1.0);
        let m: f32 = dice_part[d_idx + 1..].trim().parse().unwrap_or(4.0);
        n * (m + 1.0) / 2.0 + bonus
    } else {
        dice_str.parse::<f32>().unwrap_or(2.0)
    }
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
