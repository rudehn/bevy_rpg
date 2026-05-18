use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::components::{Chest, GameEntityMarker, InInventory, Item, Monster, Name, Position, Prop, Viewshed};
use crate::game::ai::{MonsterAI, MonsterAIMode};
use crate::game::combat::Health;
use crate::game::enchantment::{display_item_name, Enchantment, ItemArmorRunic, ItemWeaponRunic, RunicIdentified};
use crate::game::goap::GoapAI;
use crate::game::items::ItemProperties;
use crate::game::magic::StatusEffects;
use crate::game::{AppState, InGameState};
use crate::map::Map;
use crate::map::map::GRID_SIZE;
use crate::map::tile::TerrainType;
use crate::player::Player;
use crate::ui::{collect_status_effects_with_duration, spawn_status_badge};
use roguelike_engine::stealth::{Awareness, AwarenessState};

// --- Resources ---

#[derive(Resource, Default)]
pub struct NearbyState {
    pub selected_idx: Option<usize>,
    pub entity_list: Vec<Entity>,
}

/// A 1×1 white pixel image used to render the world-space tile highlight.
#[derive(Resource)]
pub struct WhitePixelHandle(pub Handle<Image>);

// --- Marker components ---

/// Marker for the container node in the stats panel that holds nearby rows.
#[derive(Component)]
pub struct NearbyListRoot;

/// Marker for each entity row spawned inside NearbyListRoot.
#[derive(Component)]
pub struct NearbyRow {
    pub entity: Entity,
}

/// World-space pulsing overlay drawn on the selected entity's tile.
#[derive(Component)]
pub struct NearbyHighlightOverlay;

// --- Plugin ---

pub struct NearbyPlugin;

impl Plugin for NearbyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyState>()
            .add_systems(Startup, setup_white_pixel)
            .add_systems(
                Update,
                (
                    update_nearby_panel,
                    update_nearby_selection_highlight,
                    update_nearby_highlight,
                    nearby_keyboard_input.run_if(in_state(InGameState::Running)),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// --- Setup ---

fn setup_white_pixel(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = Image::new_fill(
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[255, 255, 255, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let handle = images.add(image);
    commands.insert_resource(WhitePixelHandle(handle));
}

// --- Helper ---

fn tile_distance(a: &Position, b: &Position) -> i32 {
    let dx = (a.x - b.x) as f32;
    let dy = (a.y - b.y) as f32;
    (dx * dx + dy * dy).sqrt().round() as i32
}

/// Awareness pill text + colour for a monster row.
///
/// `mode == Asleep` short-circuits to "Sleeping" regardless of any
/// stale awareness record left over from before the monster was put
/// to sleep. Otherwise the pill reflects the monster's current
/// `AwarenessState` of the player — Aware → "Hunting", Searching or
/// Suspicious → "Searching" / "Suspicious", and an absent or Hidden
/// record means the monster is just "Wandering".
pub(super) fn awareness_pill(
    mode: MonsterAIMode,
    awareness: &Awareness,
    player_entity: Entity,
    fleeing: bool,
) -> (&'static str, Color) {
    // Sticky Fleeing wins over everything except Sleeping — a fleeing
    // monster is by definition awake and panicked.
    if mode == MonsterAIMode::Asleep {
        return ("Sleeping", Color::srgb(0.45, 0.45, 0.45));
    }
    if fleeing {
        return ("Fleeing", Color::srgb(0.95, 0.50, 0.20));
    }
    match awareness.get(player_entity).map(|r| r.state) {
        Some(AwarenessState::Aware) => ("Hunting", Color::srgb(0.85, 0.20, 0.20)),
        Some(AwarenessState::Searching { .. }) => ("Searching", Color::srgb(0.95, 0.78, 0.20)),
        Some(AwarenessState::Suspicious { .. }) => ("Suspicious", Color::srgb(0.95, 0.78, 0.20)),
        None | Some(AwarenessState::Hidden) => ("Wandering", Color::srgb(0.55, 0.55, 0.55)),
    }
}

// --- Systems ---

fn update_nearby_panel(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    ascii_font_res: Option<Res<crate::game::ascii_mode::AsciiFont>>,
    map: Res<Map>,
    player_query: Query<(Entity, &Viewshed, &Position), (With<Player>, Changed<Viewshed>)>,
    monster_query: Query<
        (
            Entity,
            &Position,
            &Name,
            Option<&Children>,
            &Health,
            Option<&MonsterAI>,
            Option<&GoapAI>,
            Option<&StatusEffects>,
            Option<&Awareness>,
            Option<&crate::game::fleeing::Fleeing>,
        ),
        With<Monster>,
    >,
    item_query: Query<
        (Entity, &Position, &Name, &ItemProperties, Option<&Children>,
         Option<&Enchantment>, Option<&ItemWeaponRunic>, Option<&ItemArmorRunic>, Option<&RunicIdentified>),
        (With<Item>, Without<InInventory>),
    >,
    chest_query: Query<
        (Entity, &Position, &Name, Option<&Children>),
        (With<Chest>, With<Prop>),
    >,
    glyph_query: Query<(&Text2d, &TextColor), With<crate::game::ascii_mode::AsciiGlyph>>,
    root_query: Query<Entity, With<NearbyListRoot>>,
    row_query: Query<Entity, With<NearbyRow>>,
    mut nearby_state: ResMut<NearbyState>,
) {
    let Ok((player_entity, viewshed, player_pos)) = player_query.single() else {
        return;
    };

    let visible: HashSet<(i32, i32)> = viewshed
        .visible_tiles
        .iter()
        .map(|p| (p.x, p.y))
        .collect();

    // Helper: find ASCII glyph info from children
    let get_ascii_info = |children: Option<&Children>| -> Option<(String, Color)> {
        children.and_then(|ch| {
            for child in ch.iter() {
                if let Ok((text, color)) = glyph_query.get(child) {
                    return Some((text.0.clone(), color.0));
                }
            }
            None
        })
    };

    // (entity, dist, name, ascii_char, ascii_color)
    type NearbyEntry = (Entity, i32, String, Option<String>, Option<Color>);

    // Monster entry with extra display data
    struct MonsterEntry {
        base: NearbyEntry,
        health_pct: f32,
        awareness_label: &'static str,
        awareness_color: Color,
        status_effects: Vec<(String, Color, u32, u32, String)>,
    }

    // Collect visible monsters
    let empty_awareness = Awareness::default();
    let mut monsters: Vec<MonsterEntry> =
        monster_query
            .iter()
            .filter(|(_, pos, ..)| visible.contains(&(pos.x, pos.y)))
            .map(|(entity, pos, name, children, health, monster_ai, _goap_ai, status, awareness, fleeing)| {
                let dist = tile_distance(player_pos, pos);
                let (ac, acol) = get_ascii_info(children).unzip();
                let health_pct = if health.max > 0 { health.current as f32 / health.max as f32 } else { 1.0 };
                let mode = monster_ai.map(|a| a.mode).unwrap_or(MonsterAIMode::Idle);
                let aw = awareness.unwrap_or(&empty_awareness);
                let (awareness_label, awareness_color) =
                    awareness_pill(mode, aw, player_entity, fleeing.is_some());
                let status_effects = collect_status_effects_with_duration(status);
                MonsterEntry {
                    base: (entity, dist, name.0.clone(), ac, acol),
                    health_pct,
                    awareness_label,
                    awareness_color,
                    status_effects,
                }
            })
            .collect();
    monsters.sort_by_key(|m| m.base.1);

    // Collect visible items
    let mut items: Vec<NearbyEntry> = item_query
        .iter()
        .filter(|(_, pos, ..)| visible.contains(&(pos.x, pos.y)))
        .map(|(entity, pos, name, _props, children, ench, w_runic, a_runic, runic_id)| {
            let dist = tile_distance(player_pos, pos);
            let (ac, acol) = get_ascii_info(children).unzip();
            let enriched = display_item_name(&name.0, ench, w_runic, a_runic, runic_id);
            (entity, dist, enriched, ac, acol)
        })
        .collect();
    items.sort_by_key(|(_, d, ..)| *d);

    // Collect visible chests (props)
    let mut chests: Vec<NearbyEntry> = chest_query
        .iter()
        .filter(|(_, pos, ..)| visible.contains(&(pos.x, pos.y)))
        .map(|(entity, pos, name, children)| {
            let dist = tile_distance(player_pos, pos);
            let (ac, acol) = get_ascii_info(children).unzip();
            (entity, dist, name.0.clone(), ac, acol)
        })
        .collect();
    chests.sort_by_key(|(_, d, ..)| *d);

    // Collect visible stairs from the map
    let mut stairs: Vec<(String, i32)> = Vec::new();
    for pt in &viewshed.visible_tiles {
        if pt.x < 0 || pt.y < 0 || pt.x >= map.width() || pt.y >= map.height() {
            continue;
        }
        let idx = map.xy_idx(pt.x, pt.y);
        let terrain = map.tiles[idx].terrain;
        let name = match terrain {
            TerrainType::DownStairs => "Down Stairs",
            TerrainType::UpStairs => "Up Stairs",
            _ => continue,
        };
        let stair_pos = Position { x: pt.x, y: pt.y };
        let dist = tile_distance(player_pos, &stair_pos);
        stairs.push((name.to_string(), dist));
    }
    stairs.sort_by_key(|(_, d)| *d);

    // Update entity list
    nearby_state.entity_list = monsters
        .iter()
        .map(|m| m.base.0)
        .chain(items.iter().map(|(e, ..)| *e))
        .chain(chests.iter().map(|(e, ..)| *e))
        .collect();

    // Clamp selection
    if let Some(idx) = nearby_state.selected_idx
        && idx >= nearby_state.entity_list.len() {
            nearby_state.selected_idx = None;
        }

    // Despawn old rows
    for entity in &row_query {
        commands.entity(entity).despawn();
    }

    let Ok(root) = root_query.single() else {
        return;
    };

    if monsters.is_empty() && items.is_empty() && chests.is_empty() && stairs.is_empty() {
        return;
    }

    let font: Handle<Font> = asset_server.load("fonts/Macondo-Regular.ttf");
    let sel = nearby_state.selected_idx;
    let entity_list = nearby_state.entity_list.clone();

    commands.entity(root).with_children(|parent| {
        // Section header (not selectable)
        parent.spawn((
            Text::new("— NEARBY —"),
            TextFont { font: font.clone(), font_size: 13.0, ..default() },
            TextColor(Color::srgb(0.4, 0.4, 0.55)),
            Node { margin: UiRect::top(Val::Px(8.0)), ..default() },
            NearbyRow { entity: Entity::PLACEHOLDER },
        ));

        // --- MONSTERS ---
        if !monsters.is_empty() {
            parent.spawn((
                Text::new("MONSTERS"),
                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.9, 0.3, 0.3)),
                Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                NearbyRow { entity: Entity::PLACEHOLDER },
            ));
            for (i, monster) in monsters.iter().enumerate() {
                let (entity, _dist, name, ascii_char, ascii_color) = &monster.base;
                let is_selected = sel == Some(i);
                let bg = if is_selected {
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15))
                } else {
                    BackgroundColor(Color::NONE)
                };
                let truncated = truncate_name(name);
                // Outer container: vertical column for all rows of this monster
                parent
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                            margin: UiRect::top(Val::Px(2.0)),
                            ..default()
                        },
                        bg,
                        NearbyRow { entity: *entity },
                    ))
                    .with_children(|col| {
                        // Row 1: icon + name
                        col.spawn(Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            ..default()
                        }).with_children(|row| {
                            if let (Some(ch), Some(col_color)) = (ascii_char, ascii_color) {
                                let afont = ascii_font_res.as_ref().map(|f| f.0.clone()).unwrap_or_else(|| font.clone());
                                row.spawn((
                                    Text::new(ch.clone()),
                                    TextFont { font: afont, font_size: 14.0, ..default() },
                                    TextColor(*col_color),
                                    Node {
                                        width: Val::Px(14.0),
                                        margin: UiRect::right(Val::Px(4.0)),
                                        ..default()
                                    },
                                ));
                            }
                            row.spawn((
                                Text::new(truncated),
                                TextFont { font: font.clone(), font_size: 12.0, ..default() },
                                TextColor(Color::srgb(0.85, 0.85, 0.85)),
                            ));
                        });

                        // Row 2: health bar + AI state
                        col.spawn(Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            margin: UiRect::top(Val::Px(1.0)),
                            padding: UiRect::left(Val::Px(18.0)),
                            ..default()
                        }).with_children(|row| {
                            // Health bar background (red)
                            row.spawn((
                                Node {
                                    width: Val::Px(80.0),
                                    height: Val::Px(8.0),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.5, 0.1, 0.1)),
                                BorderColor::all(Color::srgb(0.4, 0.4, 0.4)),
                            )).with_children(|bar| {
                                // Health bar fill (green)
                                bar.spawn((
                                    Node {
                                        width: Val::Percent(monster.health_pct * 100.0),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.0, 0.8, 0.0)),
                                ));
                            });
                            // Awareness pill (subsumes the legacy
                            // mode-only `(Sleeping)` label by adding
                            // Suspicious / Searching states the engine
                            // mode FSM can't distinguish).
                            row.spawn((
                                Text::new(monster.awareness_label),
                                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                                TextColor(monster.awareness_color),
                                Node { margin: UiRect::left(Val::Px(4.0)), ..default() },
                            ));
                        });

                        // Row 3: status effects (only if any)
                        if !monster.status_effects.is_empty() {
                            col.spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                column_gap: Val::Px(3.0),
                                row_gap: Val::Px(2.0),
                                margin: UiRect::top(Val::Px(1.0)),
                                padding: UiRect::left(Val::Px(18.0)),
                                ..default()
                            }).with_children(|row| {
                                for (label, color, turns_remaining, initial_duration, desc) in &monster.status_effects {
                                    spawn_status_badge(row, &font, label, *color, *turns_remaining, *initial_duration, desc);
                                }
                            });
                        }
                    });
            }
        }

        // --- ITEMS ---
        if !items.is_empty() {
            parent.spawn((
                Text::new("ITEMS"),
                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.9, 0.7, 0.3)),
                Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                NearbyRow { entity: Entity::PLACEHOLDER },
            ));
            for (i, (entity, dist, name, ascii_char, ascii_color)) in items.iter().enumerate() {
                let global_idx = monsters.len() + i;
                let is_selected = sel == Some(global_idx);
                let bg = if is_selected {
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15))
                } else {
                    BackgroundColor(Color::NONE)
                };
                let truncated = truncate_name(name);
                parent
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                            margin: UiRect::top(Val::Px(1.0)),
                            ..default()
                        },
                        bg,
                        NearbyRow { entity: *entity },
                    ))
                    .with_children(|row| {
                        if let (Some(ch), Some(col)) = (ascii_char, ascii_color) {
                            let afont = ascii_font_res.as_ref().map(|f| f.0.clone()).unwrap_or_else(|| font.clone());
                            row.spawn((
                                Text::new(ch.clone()),
                                TextFont { font: afont, font_size: 14.0, ..default() },
                                TextColor(*col),
                                Node {
                                    width: Val::Px(14.0),
                                    margin: UiRect::right(Val::Px(4.0)),
                                    ..default()
                                },
                            ));
                        }
                        row.spawn((
                            Text::new(format!("{} {}", truncated, dist)),
                            TextFont { font: font.clone(), font_size: 12.0, ..default() },
                            TextColor(Color::srgb(0.85, 0.85, 0.85)),
                            Node { flex_grow: 1.0, ..default() },
                        ));
                    });
            }
        }

        // --- CHESTS ---
        if !chests.is_empty() {
            parent.spawn((
                Text::new("CHESTS"),
                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.6, 0.4, 0.2)),
                Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                NearbyRow { entity: Entity::PLACEHOLDER },
            ));
            for (i, (entity, dist, name, ascii_char, ascii_color)) in chests.iter().enumerate() {
                let global_idx = monsters.len() + items.len() + i;
                let is_selected = sel == Some(global_idx);
                let bg = if is_selected {
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15))
                } else {
                    BackgroundColor(Color::NONE)
                };
                let truncated = truncate_name(name);
                parent
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                            margin: UiRect::top(Val::Px(1.0)),
                            ..default()
                        },
                        bg,
                        NearbyRow { entity: *entity },
                    ))
                    .with_children(|row| {
                        if let (Some(ch), Some(col)) = (ascii_char, ascii_color) {
                            let afont = ascii_font_res.as_ref().map(|f| f.0.clone()).unwrap_or_else(|| font.clone());
                            row.spawn((
                                Text::new(ch.clone()),
                                TextFont { font: afont, font_size: 14.0, ..default() },
                                TextColor(*col),
                                Node {
                                    width: Val::Px(14.0),
                                    margin: UiRect::right(Val::Px(4.0)),
                                    ..default()
                                },
                            ));
                        }
                        row.spawn((
                            Text::new(format!("{} {}", truncated, dist)),
                            TextFont { font: font.clone(), font_size: 12.0, ..default() },
                            TextColor(Color::srgb(0.85, 0.85, 0.85)),
                            Node { flex_grow: 1.0, ..default() },
                        ));
                    });
            }
        }

        // --- STAIRS ---
        if !stairs.is_empty() {
            parent.spawn((
                Text::new("STAIRS"),
                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.5, 0.8, 1.0)),
                Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                NearbyRow { entity: Entity::PLACEHOLDER },
            ));
            for (name, dist) in &stairs {
                parent
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                            margin: UiRect::top(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        NearbyRow { entity: Entity::PLACEHOLDER },
                    ))
                    .with_children(|row| {
                        // Stair icon: > for down, < for up
                        let icon = if name.contains("Down") { ">" } else { "<" };
                        row.spawn((
                            Text::new(icon),
                            TextFont { font: font.clone(), font_size: 14.0, ..default() },
                            TextColor(Color::srgb(0.5, 0.8, 1.0)),
                            Node {
                                width: Val::Px(14.0),
                                margin: UiRect::right(Val::Px(4.0)),
                                ..default()
                            },
                        ));
                        row.spawn((
                            Text::new(format!("{} {}", name, dist)),
                            TextFont { font: font.clone(), font_size: 12.0, ..default() },
                            TextColor(Color::srgb(0.85, 0.85, 0.85)),
                            Node { flex_grow: 1.0, ..default() },
                        ));
                    });
            }
        }

        let _ = entity_list; // suppress unused warning; we only need it for the borrow
    });
}

fn truncate_name(name: &str) -> String {
    if name.len() > 17 {
        format!("{:.16}…", name)
    } else {
        name.to_string()
    }
}

/// Updates row background colors when selection changes without a full rebuild.
fn update_nearby_selection_highlight(
    nearby_state: Res<NearbyState>,
    mut row_query: Query<(&NearbyRow, &mut BackgroundColor)>,
) {
    if !nearby_state.is_changed() {
        return;
    }
    let selected = nearby_state
        .selected_idx
        .and_then(|idx| nearby_state.entity_list.get(idx))
        .copied();

    for (row, mut bg) in &mut row_query {
        if row.entity == Entity::PLACEHOLDER {
            continue;
        }
        bg.0 = if selected == Some(row.entity) {
            Color::srgba(1.0, 1.0, 1.0, 0.15)
        } else {
            Color::NONE
        };
    }
}

fn nearby_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut nearby_state: ResMut<NearbyState>,
) {
    if nearby_state.entity_list.is_empty() {
        return;
    }

    let len = nearby_state.entity_list.len();
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    if keys.just_pressed(KeyCode::Tab) {
        nearby_state.selected_idx = Some(match nearby_state.selected_idx {
            None => {
                if shift { len - 1 } else { 0 }
            }
            Some(idx) => {
                if shift {
                    if idx == 0 { len - 1 } else { idx - 1 }
                } else {
                    (idx + 1) % len
                }
            }
        });
        return;
    }

    if keys.just_pressed(KeyCode::Escape) {
        nearby_state.selected_idx = None;
        return;
    }

    // Clear on any movement key
    let movement_keys = [
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::KeyW,
        KeyCode::KeyS,
        KeyCode::KeyA,
        KeyCode::KeyD,
        KeyCode::Numpad8,
        KeyCode::Numpad2,
        KeyCode::Numpad4,
        KeyCode::Numpad6,
        KeyCode::Numpad7,
        KeyCode::Numpad9,
        KeyCode::Numpad1,
        KeyCode::Numpad3,
        KeyCode::Numpad5,
    ];
    if keys.any_just_pressed(movement_keys) {
        nearby_state.selected_idx = None;
    }
}

/// Spawns or updates a pulsing world-space tile highlight on the selected entity's position.
fn update_nearby_highlight(
    time: Res<Time>,
    nearby_state: Res<NearbyState>,
    white_pixel: Option<Res<WhitePixelHandle>>,
    entity_positions: Query<&Position, Or<(With<Monster>, With<Item>, With<Chest>)>>,
    monster_check: Query<(), With<Monster>>,
    mut overlay_query: Query<(&mut Transform, &mut Sprite), With<NearbyHighlightOverlay>>,
    overlay_entities: Query<Entity, With<NearbyHighlightOverlay>>,
    mut commands: Commands,
) {
    let Some(white_pixel) = white_pixel else {
        return;
    };

    let selected_entity = nearby_state
        .selected_idx
        .and_then(|idx| nearby_state.entity_list.get(idx))
        .copied();

    if let Some(sel) = selected_entity {
        if let Ok(pos) = entity_positions.get(sel) {
            let is_monster = monster_check.get(sel).is_ok();
            let alpha = (time.elapsed_secs() * std::f32::consts::TAU).sin() * 0.25 + 0.55;
            let color = if is_monster {
                Color::srgba(1.0, 0.2, 0.2, alpha)
            } else {
                Color::srgba(0.9, 0.8, 0.1, alpha)
            };
            let world_x = pos.x as f32 * GRID_SIZE.x;
            let world_y = pos.y as f32 * GRID_SIZE.y;

            if let Ok((mut tf, mut sprite)) = overlay_query.single_mut() {
                tf.translation.x = world_x;
                tf.translation.y = world_y;
                sprite.color = color;
            } else {
                commands.spawn((
                    Sprite {
                        image: white_pixel.0.clone(),
                        color,
                        custom_size: Some(GRID_SIZE),
                        ..default()
                    },
                    Transform::from_xyz(world_x, world_y, 4.0),
                    NearbyHighlightOverlay,
                    GameEntityMarker,
                    RenderLayers::layer(1),
                ));
            }
        }
    } else {
        for entity in &overlay_entities {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bracket_lib::prelude::Point;

    fn pe() -> Entity {
        Entity::from_raw_u32(1).expect("valid test entity index")
    }

    #[test]
    fn asleep_overrides_awareness() {
        let mut a = Awareness::default();
        a.set(pe(), AwarenessState::Aware, 0);
        let (text, _) = awareness_pill(MonsterAIMode::Asleep, &a, pe(), false);
        assert_eq!(text, "Sleeping");
    }

    #[test]
    fn aware_yields_hunting() {
        let mut a = Awareness::default();
        a.set(pe(), AwarenessState::Aware, 0);
        let (text, _) = awareness_pill(MonsterAIMode::Hunting, &a, pe(), false);
        assert_eq!(text, "Hunting");
    }

    #[test]
    fn no_record_yields_wandering() {
        let a = Awareness::default();
        let (text, _) = awareness_pill(MonsterAIMode::Idle, &a, pe(), false);
        assert_eq!(text, "Wandering");
    }

    #[test]
    fn searching_yields_searching() {
        let mut a = Awareness::default();
        a.set(
            pe(),
            AwarenessState::Searching {
                last_known_pos: Point::new(0, 0),
                giveup_at_turn: 10,
            },
            0,
        );
        let (text, _) = awareness_pill(MonsterAIMode::Idle, &a, pe(), false);
        assert_eq!(text, "Searching");
    }

    #[test]
    fn suspicious_yields_suspicious() {
        let mut a = Awareness::default();
        a.set(
            pe(),
            AwarenessState::Suspicious {
                suspect_pos: Point::new(0, 0),
                decay_at_turn: 10,
            },
            0,
        );
        let (text, _) = awareness_pill(MonsterAIMode::Idle, &a, pe(), false);
        assert_eq!(text, "Suspicious");
    }

    #[test]
    fn hidden_record_yields_wandering() {
        let mut a = Awareness::default();
        a.set(pe(), AwarenessState::Hidden, 0);
        let (text, _) = awareness_pill(MonsterAIMode::Idle, &a, pe(), false);
        assert_eq!(text, "Wandering");
    }

    #[test]
    fn fleeing_marker_overrides_hunting_label() {
        let mut a = Awareness::default();
        a.set(pe(), AwarenessState::Aware, 0);
        let (text, _) = awareness_pill(MonsterAIMode::Hunting, &a, pe(), true);
        assert_eq!(text, "Fleeing");
    }

    #[test]
    fn fleeing_marker_does_not_override_sleeping() {
        // A sleeping monster cannot also be fleeing — guard the invariant.
        let a = Awareness::default();
        let (text, _) = awareness_pill(MonsterAIMode::Asleep, &a, pe(), true);
        assert_eq!(text, "Sleeping");
    }
}
