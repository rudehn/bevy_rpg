use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::world::Ref;
use bevy::prelude::{Children, Color, Sprite, TextColor, Visibility};
use bevy::{
    ecs::{
        query::{Changed, With, Without},
        system::{Query, Res},
    },
    transform::components::Transform,
};
use bracket_lib::prelude::{Algorithm2D, Point, field_of_view};

use crate::game::magic::Stunned;
use crate::map::map::GRID_SIZE;
use crate::{
    components::{InInventory, Item, Monster, Position, Prop, Viewshed},
    map::Map,
    player::Player,
};

pub fn fov_update_system(mut query: Query<(&mut Viewshed, Ref<Position>)>, map: Res<Map>) {
    // We check if the map itself has changed (e.g. new level loaded) because
    // viewsheds calculated on the old map will be invalid even if the entity
    // hasn't moved yet.
    let map_changed = map.is_changed();
    for (mut viewshed, position) in query.iter_mut() {
        if viewshed.dirty || map_changed || viewshed.is_changed() || position.is_changed() {
            viewshed.visible_tiles.clear();
            viewshed.visible_tiles =
                field_of_view(Point::new(position.x, position.y), viewshed.range, &*map);
            viewshed.dirty = false;
        }
    }
}

pub fn update_monster_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    mut monster_query: Query<(&Position, &mut Visibility, &mut Sprite), With<Monster>>,
) {
    let Ok(player_viewshed) = player_query.single() else {
        return;
    };
    let is_ascii = *mode == crate::game::ascii_mode::GraphicsMode::Ascii;

    for (monster_pos, mut monster_vis, mut sprite) in monster_query.iter_mut() {
        let monster_point = Point::new(monster_pos.x, monster_pos.y);
        let is_visible = player_viewshed.visible_tiles.contains(&monster_point);

        if is_visible {
            *monster_vis = Visibility::Visible;
            sprite.color = if is_ascii { Color::NONE } else { Color::WHITE };
        } else {
            *monster_vis = Visibility::Hidden;
        }
    }
}

/// Updates floor item visibility to match the player's explored/visible state.
/// - Unexplored tile: Hidden
/// - Explored, not visible: Visible but dimmed (item "memory")
/// - Currently visible: Full brightness
pub fn update_item_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    map: Res<Map>,
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    mut item_query: Query<(&Position, &mut Visibility, &mut Sprite), (With<Item>, Without<InInventory>)>,
) {
    let Ok(viewshed) = player_query.single() else {
        return;
    };
    let is_ascii = *mode == crate::game::ascii_mode::GraphicsMode::Ascii;

    for (pos, mut vis, mut sprite) in item_query.iter_mut() {
        if !map.in_bounds(Point::new(pos.x, pos.y)) {
            continue;
        }
        let idx = map.xy_idx(pos.x, pos.y);
        let pt = Point::new(pos.x, pos.y);

        if viewshed.visible_tiles.contains(&pt) {
            *vis = Visibility::Visible;
            sprite.color = if is_ascii { Color::NONE } else { Color::WHITE };
        } else if map.explored_tiles[idx] {
            *vis = Visibility::Visible;
            sprite.color = if is_ascii { Color::NONE } else { Color::srgb(0.5, 0.5, 0.5) };
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

/// Tints monster sprites based on active status effects.
/// Priority: Stunned (yellow) > default (white).
pub fn update_status_visuals(
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    mut query: Query<(Option<&Stunned>, &mut Sprite, Option<&Children>), With<Monster>>,
    mut glyph_query: Query<&mut TextColor, With<crate::game::ascii_mode::AsciiGlyph>>,
) {
    let is_ascii = *mode == crate::game::ascii_mode::GraphicsMode::Ascii;

    for (stunned, mut sprite, children) in &mut query {
        let tint = if stunned.is_some() {
            Color::srgba(1.0, 1.0, 0.3, 1.0)
        } else {
            Color::WHITE
        };

        if is_ascii {
            sprite.color = Color::NONE;
            if let Some(children) = children {
                for &child in children.iter() {
                    if let Ok(mut text_color) = glyph_query.get_mut(child) {
                        *text_color = TextColor(tint);
                    }
                }
            }
        } else {
            sprite.color = tint;
        }
    }
}

/// Updates prop visibility to match the player's explored/visible state.
/// Same logic as items: explored-but-not-visible tiles show dimmed.
pub fn update_prop_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    map: Res<Map>,
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    mut prop_query: Query<(&Position, &mut Visibility, &mut Sprite), With<Prop>>,
) {
    let Ok(viewshed) = player_query.single() else {
        return;
    };
    let is_ascii = *mode == crate::game::ascii_mode::GraphicsMode::Ascii;

    for (pos, mut vis, mut sprite) in prop_query.iter_mut() {
        if !map.in_bounds(Point::new(pos.x, pos.y)) {
            continue;
        }
        let idx = map.xy_idx(pos.x, pos.y);
        let pt = Point::new(pos.x, pos.y);

        if viewshed.visible_tiles.contains(&pt) {
            *vis = Visibility::Visible;
            sprite.color = if is_ascii { Color::NONE } else { Color::WHITE };
        } else if map.explored_tiles[idx] {
            *vis = Visibility::Visible;
            sprite.color = if is_ascii { Color::NONE } else { Color::srgb(0.5, 0.5, 0.5) };
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

pub fn sync_entity_transforms(mut query: Query<(&mut Transform, &Position), Changed<Position>>) {
    let config = GRID_SIZE;
    for (mut transform, pos) in query.iter_mut() {
        // Calculate the center of the tile
        let x = pos.x as f32 * config.x;
        let y = pos.y as f32 * config.y;

        // Update the translation.
        // We keep the existing Z-axis to maintain layering (Player on top of Items)
        transform.translation.x = x;
        transform.translation.y = y;
    }
}
