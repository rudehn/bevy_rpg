use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::components::{Chest, GameEntityMarker, InInventory, Item, Monster, Name, Position, Prop, Viewshed};
use crate::game::items::ItemProperties;
use crate::game::shrines::{ShrineData, ShrineMarker};
use crate::game::{AppState, InGameState};
use crate::map::map::GRID_SIZE;
use crate::player::Player;

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

// --- Systems ---

fn update_nearby_panel(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    ascii_font_res: Option<Res<crate::game::ascii_mode::AsciiFont>>,
    player_query: Query<(&Viewshed, &Position), (With<Player>, Changed<Viewshed>)>,
    monster_query: Query<(Entity, &Position, &Name, &Sprite, Option<&Children>), With<Monster>>,
    item_query: Query<
        (Entity, &Position, &Name, &ItemProperties, &Sprite, Option<&Children>),
        (With<Item>, Without<InInventory>),
    >,
    chest_query: Query<
        (Entity, &Position, &Name, &Sprite, Option<&Children>),
        (With<Chest>, With<Prop>, Without<ShrineMarker>),
    >,
    shrine_query: Query<
        (Entity, &Position, &Name, &ShrineData, Option<&Sprite>, Option<&Children>),
        With<ShrineMarker>,
    >,
    glyph_query: Query<(&Text2d, &TextColor), With<crate::game::ascii_mode::AsciiGlyph>>,
    root_query: Query<Entity, With<NearbyListRoot>>,
    row_query: Query<Entity, With<NearbyRow>>,
    mut nearby_state: ResMut<NearbyState>,
) {
    let Ok((viewshed, player_pos)) = player_query.single() else {
        return;
    };

    let visible: HashSet<(i32, i32)> = viewshed
        .visible_tiles
        .iter()
        .map(|p| (p.x, p.y))
        .collect();

    let is_ascii = *mode == crate::game::ascii_mode::GraphicsMode::Ascii;

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

    // (entity, dist, name, image, atlas, ascii_char, ascii_color)
    type NearbyEntry = (Entity, i32, String, Handle<Image>, Option<TextureAtlas>, Option<String>, Option<Color>);

    // Collect visible monsters
    let mut monsters: Vec<NearbyEntry> =
        monster_query
            .iter()
            .filter(|(_, pos, ..)| visible.contains(&(pos.x, pos.y)))
            .map(|(entity, pos, name, sprite, children)| {
                let dist = tile_distance(player_pos, pos);
                let (ac, acol) = get_ascii_info(children).unzip();
                (entity, dist, name.0.clone(), sprite.image.clone(), sprite.texture_atlas.clone(), ac, acol)
            })
            .collect();
    monsters.sort_by_key(|(_, d, ..)| *d);

    // Collect visible items
    let mut items: Vec<NearbyEntry> = item_query
        .iter()
        .filter(|(_, pos, ..)| visible.contains(&(pos.x, pos.y)))
        .map(|(entity, pos, name, _props, sprite, children)| {
            let dist = tile_distance(player_pos, pos);
            let (ac, acol) = get_ascii_info(children).unzip();
            (entity, dist, name.0.clone(), sprite.image.clone(), sprite.texture_atlas.clone(), ac, acol)
        })
        .collect();
    items.sort_by_key(|(_, d, ..)| *d);

    // Collect visible chests (props)
    let mut chests: Vec<NearbyEntry> = chest_query
        .iter()
        .filter(|(_, pos, ..)| visible.contains(&(pos.x, pos.y)))
        .map(|(entity, pos, name, sprite, children)| {
            let dist = tile_distance(player_pos, pos);
            let (ac, acol) = get_ascii_info(children).unzip();
            (entity, dist, name.0.clone(), sprite.image.clone(), sprite.texture_atlas.clone(), ac, acol)
        })
        .collect();
    chests.sort_by_key(|(_, d, ..)| *d);

    // Collect visible shrines
    let mut shrines: Vec<NearbyEntry> = shrine_query
        .iter()
        .filter(|(_, pos, ..)| visible.contains(&(pos.x, pos.y)))
        .filter_map(|(entity, pos, name, _shrine_data, sprite, children)| {
            let dist = tile_distance(player_pos, pos);
            let (ac, acol) = get_ascii_info(children).unzip();
            let (img, atlas) = if let Some(s) = sprite {
                (s.image.clone(), s.texture_atlas.clone())
            } else {
                return None;
            };
            Some((entity, dist, name.0.clone(), img, atlas, ac, acol))
        })
        .collect();
    shrines.sort_by_key(|(_, d, ..)| *d);

    // Update entity list
    nearby_state.entity_list = monsters
        .iter()
        .map(|(e, ..)| *e)
        .chain(items.iter().map(|(e, ..)| *e))
        .chain(chests.iter().map(|(e, ..)| *e))
        .chain(shrines.iter().map(|(e, ..)| *e))
        .collect();

    // Clamp selection
    if let Some(idx) = nearby_state.selected_idx {
        if idx >= nearby_state.entity_list.len() {
            nearby_state.selected_idx = None;
        }
    }

    // Despawn old rows
    for entity in &row_query {
        commands.entity(entity).despawn();
    }

    let Ok(root) = root_query.single() else {
        return;
    };

    if monsters.is_empty() && items.is_empty() && chests.is_empty() && shrines.is_empty() {
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
            for (i, (entity, dist, name, image, atlas, ascii_char, ascii_color)) in monsters.iter().enumerate() {
                let is_selected = sel == Some(i);
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
                        if is_ascii {
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
                        } else {
                            let mut img = ImageNode::new(image.clone());
                            img.texture_atlas = atlas.clone();
                            row.spawn((
                                Node {
                                    width: Val::Px(14.0),
                                    height: Val::Px(14.0),
                                    margin: UiRect::right(Val::Px(4.0)),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                img,
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

        // --- ITEMS ---
        if !items.is_empty() {
            parent.spawn((
                Text::new("ITEMS"),
                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.9, 0.7, 0.3)),
                Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                NearbyRow { entity: Entity::PLACEHOLDER },
            ));
            for (i, (entity, dist, name, image, atlas, ascii_char, ascii_color)) in items.iter().enumerate() {
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
                        if is_ascii {
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
                        } else {
                            let mut img = ImageNode::new(image.clone());
                            img.texture_atlas = atlas.clone();
                            row.spawn((
                                Node {
                                    width: Val::Px(14.0),
                                    height: Val::Px(14.0),
                                    margin: UiRect::right(Val::Px(4.0)),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                img,
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
            for (i, (entity, dist, name, image, atlas, ascii_char, ascii_color)) in chests.iter().enumerate() {
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
                        if is_ascii {
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
                        } else {
                            let mut img = ImageNode::new(image.clone());
                            img.texture_atlas = atlas.clone();
                            row.spawn((
                                Node {
                                    width: Val::Px(14.0),
                                    height: Val::Px(14.0),
                                    margin: UiRect::right(Val::Px(4.0)),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                img,
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

        // --- SHRINES ---
        if !shrines.is_empty() {
            parent.spawn((
                Text::new("SHRINES"),
                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.6, 0.9, 0.6)),
                Node { margin: UiRect::top(Val::Px(4.0)), ..default() },
                NearbyRow { entity: Entity::PLACEHOLDER },
            ));
            for (i, (entity, dist, name, image, atlas, ascii_char, ascii_color)) in shrines.iter().enumerate() {
                let global_idx = monsters.len() + items.len() + chests.len() + i;
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
                        if is_ascii {
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
                        } else {
                            let mut img = ImageNode::new(image.clone());
                            img.texture_atlas = atlas.clone();
                            row.spawn((
                                Node {
                                    width: Val::Px(14.0),
                                    height: Val::Px(14.0),
                                    margin: UiRect::right(Val::Px(4.0)),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                img,
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
    entity_positions: Query<&Position, Or<(With<Monster>, With<Item>, With<Chest>, With<ShrineMarker>)>>,
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
