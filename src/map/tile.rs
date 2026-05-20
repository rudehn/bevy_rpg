use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::component::Component;
use bevy::prelude::{
    Commands, Entity, Has, InheritedVisibility, Message, MessageReader, MessageWriter,
    Query, Res, ResMut, Resource, Sprite, Text2d, TextColor, TextFont, Transform, Vec3,
    ViewVisibility, Visibility, With, Without, default, warn,
};
use bracket_lib::prelude::Point;

use crate::assets::{TileManifest, TileManifestHandle};
use crate::components::{Collider, Name, Viewshed};
use crate::map::Map;
use crate::map::map::GRID_SIZE;

// Tile data types and helpers now live in the engine crate. Re-exported
// here so the 100+ `use crate::map::tile::*` sites compile unchanged.
pub use roguelike_engine::map::tile::{
    Decoration, LiquidType, PromotionRule, PromotionTarget, TerrainType, Tile,
    can_entity_enter_tile, is_opaque, is_passable, is_pathing_blocker, is_walkable,
};

// Mutation messages, apply systems, and the TileEntityIndex spatial
// resource live in the engine. Re-exported so existing
// `crate::map::tile::*` import sites compile unchanged. See
// `roguelike_engine::map::mutation` for the apply pipeline. Game-side
// reactions to LiquidMutationMessage (chasm fall, lava kill) live in
// `crate::game::chasm_fall`.
pub use roguelike_engine::map::mutation::{
    apply_decoration_mutations, apply_liquid_mutations, apply_tile_mutations,
    DecorationMutationMessage, LiquidMutationMessage, MapMutationPlugin, MapMutationSet,
    TileMutationMessage,
};
pub use roguelike_engine::map::tile_entity_index::TileEntityIndex;

#[derive(Component)]
pub struct TileMarker;

// `TerrainType` and `LiquidType` are re-exported from the engine above.

// `Decoration`, `Tile`, `PromotionTarget`, `PromotionRule` and all
// enum impls are re-exported from the engine above.

// Engine re-exports cover: PromotionTarget, PromotionRule, and all
// inherent methods on TerrainType / LiquidType / Decoration (name,
// flammability, timed_promotion, on_step_promotion, movement_cost,
// blocks_fov, entangles).

#[derive(Component, Default, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TileVisibility {
    #[default]
    Hidden,
    Visible,
}

#[derive(Component, Default, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TileExplored {
    #[default]
    Unexplored,
    Explored,
}

// is_walkable, can_entity_enter_tile, is_passable, is_pathing_blocker,
// is_opaque — all re-exported from the engine above.

pub fn spawn_tile_entity(
    commands: &mut Commands,
    _map_entity: Entity,
    tile: Tile,
    pt: Point,
    tile_manifest: &TileManifest,
    ascii_font: Option<&crate::game::ascii_mode::AsciiFont>,
) -> Entity {
    let terrain_asset = tile_manifest
        .tiles
        .get(tile.terrain.name())
        .expect("Terrain type not in manifest");

    // Determine scale to fit one game map tile (GRID_SIZE).
    // Still needed for ASCII children's inverse scale calculation.
    let tile_size = terrain_asset
        .tile_size
        .unwrap_or(bevy::prelude::UVec2::new(16, 16));
    let scale_x = GRID_SIZE.x / tile_size.x as f32;
    let scale_y = GRID_SIZE.y / tile_size.y as f32;

    let mut command = commands.spawn((
        TileMarker,
        tile.terrain,
        tile.liquid,
        TileVisibility::Hidden,
        TileExplored::Unexplored,
        Transform {
            translation: Vec3::new(
                pt.x as f32 * GRID_SIZE.x,
                pt.y as f32 * GRID_SIZE.y,
                0.0,
            ),
            scale: Vec3::new(scale_x, scale_y, 1.0),
            ..Default::default()
        },
        Visibility::Hidden,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        RenderLayers::layer(1),
    ));

    if !is_walkable(tile) {
        command.insert(Collider);
    }

    let tile_entity = command.id();

    // NOTE: Decoration sprite overlays are not spawned yet — all decoration entries
    // in tiles.ron use floor_stone.png as placeholder. Once proper decoration sprites
    // exist, spawn a child entity here at z=0.05 with DecorationOverlay marker
    // (same pattern as the liquid overlay above).

    // --- ASCII mode children ---
    if let Some(font) = ascii_font {
        // Children inherit parent's scale transform. Compensate so they render
        // at the correct pixel size regardless of the atlas tile's native size.
        let inv_scale = Vec3::new(1.0 / scale_x, 1.0 / scale_y, 1.0);

        // Determine background color: liquid > terrain
        let bg_color = if tile.liquid != LiquidType::None {
            let liquid_asset = tile_manifest.tiles.get(tile.liquid.name());
            liquid_asset
                .map(|a| a.ascii_bg)
                .unwrap_or(terrain_asset.ascii_bg)
        } else {
            terrain_asset.ascii_bg
        };

        // Determine character: important terrain (stairs, doors) > decoration (dry only) > liquid > terrain
        // Fire rendering is handled by the fire animation system, not here.
        let terrain_has_priority = matches!(
            tile.terrain,
            TerrainType::DownStairs
                | TerrainType::UpStairs
                | TerrainType::Door
                | TerrainType::OpenDoor
                | TerrainType::LockedDoor
        );
        let (ascii_char, fg_color) = if terrain_has_priority {
            (terrain_asset.ascii_char.clone(), terrain_asset.ascii_fg)
        } else if tile.decoration != Decoration::None && tile.liquid == LiquidType::None {
            let dec_asset = tile_manifest.tiles.get(tile.decoration.name());
            match dec_asset {
                Some(da) if !da.ascii_char.is_empty() => (da.ascii_char.clone(), da.ascii_fg),
                _ => (terrain_asset.ascii_char.clone(), terrain_asset.ascii_fg),
            }
        } else if tile.liquid != LiquidType::None {
            let liquid_asset = tile_manifest.tiles.get(tile.liquid.name());
            match liquid_asset {
                Some(la) if !la.ascii_char.is_empty() => (la.ascii_char.clone(), la.ascii_fg),
                _ => (terrain_asset.ascii_char.clone(), terrain_asset.ascii_fg),
            }
        } else {
            (terrain_asset.ascii_char.clone(), terrain_asset.ascii_fg)
        };

        let display_char = if ascii_char.is_empty() {
            "?".to_string()
        } else {
            ascii_char
        };

        // Background quad — sized to fill one grid cell after parent scale is applied
        let bg_child = commands
            .spawn((
                Sprite {
                    color: bg_color,
                    custom_size: Some(GRID_SIZE),
                    ..default()
                },
                Transform {
                    scale: inv_scale,
                    ..default()
                },
                Visibility::Hidden,
                crate::game::ascii_mode::AsciiBackground {
                    base_color: bg_color,
                },
                RenderLayers::layer(1),
            ))
            .id();
        commands.entity(tile_entity).add_child(bg_child);

        // Character glyph — also counter-scaled so font renders at correct size
        let glyph_child = commands
            .spawn((
                Text2d::new(display_char),
                TextFont {
                    font: font.0.clone(),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(fg_color),
                Transform {
                    translation: Vec3::new(0.0, 0.0, 0.05),
                    scale: inv_scale,
                    ..default()
                },
                Visibility::Hidden,
                crate::game::ascii_mode::AsciiGlyph,
                crate::game::ascii_mode::AsciiGlyphColor(fg_color),
                RenderLayers::layer(1),
            ))
            .id();
        commands.entity(tile_entity).add_child(glyph_child);
    }

    tile_entity
}

// ---------------------------------------------------------------------------
// Runtime tile mutation
// ---------------------------------------------------------------------------

/// Bundles the three tile mutation message writers into a single SystemParam.
/// Use this instead of three separate `MessageWriter<...>` params to stay under
/// Bevy's 16-parameter limit.
#[derive(bevy::ecs::system::SystemParam)]
pub struct TileMutationWriters<'w> {
    pub terrain: MessageWriter<'w, TileMutationMessage>,
    pub decoration: MessageWriter<'w, DecorationMutationMessage>,
    pub liquid: MessageWriter<'w, LiquidMutationMessage>,
    pub fire_tiles: ResMut<'w, crate::game::fire::FireTiles>,
    pub gas_tiles: ResMut<'w, crate::game::gas::GasTiles>,
    pub light_sources: ResMut<'w, crate::map::light::LightSources>,
}

/// Resolves what a tile should display in ASCII mode. Returns (char, fg_color, bg_name)
/// based on priority: fire > important terrain > decoration (dry) > liquid > terrain.
pub fn resolve_tile_display(tile: Tile, manifest: &TileManifest) -> (String, bevy::prelude::Color, &str) {
    let terrain_asset = manifest.tiles.get(tile.terrain.name());
    let default_fg = terrain_asset.map(|a| a.ascii_fg).unwrap_or(bevy::prelude::Color::WHITE);
    let default_char = terrain_asset
        .map(|a| if a.ascii_char.is_empty() { tile.terrain.name().to_string() } else { a.ascii_char.clone() })
        .unwrap_or_else(|| "?".to_string());

    let terrain_has_priority = matches!(
        tile.terrain,
        TerrainType::DownStairs | TerrainType::UpStairs | TerrainType::Door
        | TerrainType::OpenDoor | TerrainType::LockedDoor | TerrainType::Portal
    );

    if terrain_has_priority {
        return (default_char, default_fg, tile.terrain.name());
    }

    if tile.decoration != Decoration::None && tile.liquid == LiquidType::None {
        if let Some(da) = manifest.tiles.get(tile.decoration.name()) {
            if !da.ascii_char.is_empty() {
                // Return the *decoration* name (matching the liquid branch
                // below). themed_tile_display uses `name != terrain.name()`
                // as the gate for "this tile isn't bare terrain — don't
                // theme-override its glyph." Returning the terrain name
                // here caused every forest decoration (Moss, TallGrass,
                // PhosphorescentMoss, …) to be silently overwritten by
                // the FloorKind's `,` comma glyph.
                return (da.ascii_char.clone(), da.ascii_fg, tile.decoration.name());
            }
        }
    }

    if tile.liquid != LiquidType::None {
        if let Some(la) = manifest.tiles.get(tile.liquid.name()) {
            if !la.ascii_char.is_empty() {
                return (la.ascii_char.clone(), la.ascii_fg, tile.liquid.name());
            }
        }
    }

    (default_char, default_fg, tile.terrain.name())
}

/// Resolve the background color for a tile: fire > liquid > terrain.
pub fn resolve_tile_bg(tile: Tile, manifest: &TileManifest) -> bevy::prelude::Color {
    let terrain_bg = manifest.tiles.get(tile.terrain.name()).map(|a| a.ascii_bg)
        .unwrap_or(bevy::prelude::Color::BLACK);

    if tile.liquid != LiquidType::None {
        return manifest.tiles.get(tile.liquid.name()).map(|a| a.ascii_bg).unwrap_or(terrain_bg);
    }
    terrain_bg
}

// `TileMutationMessage`, `DecorationMutationMessage`,
// `LiquidMutationMessage`, and the `apply_*_mutations` systems now live
// in `roguelike_engine::map::mutation`. They are re-exported above.

/// Game-side reaction to [`LiquidMutationMessage`]: handles chasm fall
/// (player/monsters/items dropping to the next floor with damage),
/// lava-kill (entities consumed by impassable non-chasm liquids), and
/// the forced floor transition when the player falls.
///
/// Runs `.after(MapMutationSet)` so it sees the post-mutation Map state.
pub fn chasm_fall_reaction_system(
    mut commands: Commands,
    mut messages: MessageReader<LiquidMutationMessage>,
    map: Res<Map>,
    // Monster snapshot query (mirrors snapshot_floor)
    monster_query: Query<
        (Entity, &crate::components::Position, &Name, &crate::game::combat::Health,
         Option<&crate::game::squad::SquadId>, Option<&crate::game::squad::SquadConfig>,
         Has<crate::game::squad::SquadLeader>, Option<&crate::game::ai::PatrolRoute>,
         Has<crate::components::Submerged>),
        With<crate::components::Monster>,
    >,
    // Floor item snapshot query (mirrors snapshot_floor)
    item_query: Query<
        (Entity, &crate::components::Position, &Name,
         Option<&crate::game::items::ItemStack>,
         Option<&crate::game::enchantment::Enchantment>,
         Option<&crate::game::enchantment::ItemWeaponRunic>,
         Option<&crate::game::enchantment::ItemArmorRunic>,
         Option<&crate::game::enchantment::RunicIdentified>,
         Option<&crate::game::staves::StaffData>,
         Option<&crate::game::staves::Rechargeable>,
         Has<crate::components::Drifting>),
        (With<crate::components::Item>, Without<crate::components::InInventory>),
    >,
    // Player detection
    player_query: Query<(Entity, &crate::components::Position), With<crate::player::Player>>,
    floor: Res<crate::map::dungeon::Floor>,
    mut fallen: ResMut<crate::map::dungeon::FallenEntities>,
    mut turn_manager: ResMut<crate::game::TurnManager>,
    mut transition_writer: MessageWriter<crate::map::dungeon::MapTransitionMessage>,
    mut damage_writer: MessageWriter<crate::game::combat::DamageEvent>,
    mut log_writer: MessageWriter<crate::ui::game_log::GameLogMessage>,
) {
    let mut player_falls = false;

    for msg in messages.read() {
        let idx = map.xy_idx(msg.position.x, msg.position.y);
        let full_tile = map.tiles[idx];

        // Handle entities on chasm tiles — they fall to the floor below.
        if msg.new_liquid == LiquidType::Chasm {
            let dest_floor = floor.0 + 1;
            let can_fall = dest_floor <= crate::constants::MAX_FLOOR;

            if can_fall {
                // Check for player
                if let Ok((player_entity, player_pos)) = player_query.single() {
                    if player_pos.x == msg.position.x && player_pos.y == msg.position.y {
                        player_falls = true;
                        // 2d6 fall damage (same as voluntary chasm fall)
                        let mut rng = bracket_lib::random::RandomNumberGenerator::new();
                        let fall_damage = rng.range(1, 7) + rng.range(1, 7);
                        log_writer.write(crate::ui::game_log::GameLogMessage(
                            format!("The floor collapses beneath you! You take {} damage!", fall_damage),
                        ));
                        damage_writer.write(crate::game::combat::DamageEvent {
                            attacker: None,
                            target: player_entity,
                            amount: fall_damage,
                            damage_type: crate::game::combat::DamageType::Physical,
                            source: crate::game::combat::DamageSource::Environment,
                            armor: 0,
                        });
                    }
                }

                // Snapshot and despawn monsters — they appear on the floor below.
                for (entity, pos, name, health, squad_id, squad_config, is_leader, patrol_route, is_submerged) in monster_query.iter() {
                    if pos.x == msg.position.x && pos.y == msg.position.y {
                        log_writer.write(crate::ui::game_log::GameLogMessage(
                            format!("{} falls through the chasm!", name.0),
                        ));

                        // 2d6 fall damage applied to saved HP.
                        let mut rng = bracket_lib::random::RandomNumberGenerator::new();
                        let fall_damage = rng.range(1, 7) + rng.range(1, 7);
                        let hp_after_fall = health.current - fall_damage;
                        if hp_after_fall <= 0 {
                            log_writer.write(crate::ui::game_log::GameLogMessage(
                                format!("{} doesn't survive the fall!", name.0),
                            ));
                            turn_manager.remove_entity(entity);
                            commands.entity(entity).despawn();
                            continue;
                        }

                        let saved = crate::save::SavedMonster {
                            x: pos.x,
                            y: pos.y,
                            name: name.0.clone(),
                            hp_current: hp_after_fall,
                            squad_id: squad_id.map(|s| s.0),
                            is_leader,
                            squad_config: squad_config.cloned(),
                            patrol_route: patrol_route.cloned(),
                            submerged: is_submerged,
                            // Chasm fallers reset to Hidden — they re-encounter
                            // the player on a fresh floor; degraded awareness
                            // would be a stale carryover.
                            awareness: crate::save::MonsterAwarenessSave::default(),
                            // Same reasoning: drop sticky Fleeing on a fresh
                            // floor. The new environment is its own fight.
                            fleeing: None,
                        };
                        fallen.monsters.entry(dest_floor).or_default().push(saved);

                        turn_manager.remove_entity(entity);
                        commands.entity(entity).despawn();
                    }
                }

                // Snapshot and despawn floor items — they land on the floor below.
                // Items don't "take damage" so there's no HP check; they simply
                // reappear at the same grid coordinates on the destination floor.
                for (entity, pos, name, stack, enchant, weapon_runic, armor_runic, runic_id, staff_data, rechargeable, is_drifting) in item_query.iter() {
                    if pos.x == msg.position.x && pos.y == msg.position.y {
                        log_writer.write(crate::ui::game_log::GameLogMessage(
                            format!("The {} tumbles into the chasm!", name.0),
                        ));
                        let saved = crate::save::SavedItem {
                            x: pos.x,
                            y: pos.y,
                            name: name.0.clone(),
                            count: stack.map(|s| s.count).unwrap_or(1),
                            state: crate::save::build_item_state(
                                enchant, weapon_runic, armor_runic, runic_id, staff_data, rechargeable,
                            ),
                            drifting: is_drifting,
                        };
                        fallen.items.entry(dest_floor).or_default().push(saved);
                        commands.entity(entity).despawn();
                    }
                }
            } else {
                // Deepest floor — nowhere to fall, entities are lost to the void.
                for (entity, pos, name, ..) in monster_query.iter() {
                    if pos.x == msg.position.x && pos.y == msg.position.y {
                        log_writer.write(crate::ui::game_log::GameLogMessage(
                            format!("{} falls into the endless void!", name.0),
                        ));
                        turn_manager.remove_entity(entity);
                        commands.entity(entity).despawn();
                    }
                }
                for (entity, pos, name, ..) in item_query.iter() {
                    if pos.x == msg.position.x && pos.y == msg.position.y {
                        log_writer.write(crate::ui::game_log::GameLogMessage(
                            format!("The {} is lost to the void.", name.0),
                        ));
                        commands.entity(entity).despawn();
                    }
                }
            }
        } else if !is_walkable(full_tile) {
            // Non-chasm impassable (e.g. lava) — kill entities.
            for (entity, pos, name, ..) in monster_query.iter() {
                if pos.x == msg.position.x && pos.y == msg.position.y {
                    log_writer.write(crate::ui::game_log::GameLogMessage(
                        format!("{} is consumed!", name.0),
                    ));
                    turn_manager.remove_entity(entity);
                    commands.entity(entity).despawn();
                }
            }
        }
    }

    // If player fell through a collapsing tile, trigger forced floor transition.
    if player_falls {
        transition_writer.write(crate::map::dungeon::MapTransitionMessage {
            destination_floor: floor.0 + 1,
            destination_pos: None,
        });
    }
}

// Tile walkability/passability/opacity tests moved to
// `roguelike_engine::map::tile::tests`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{TileAsset, TileManifest};
    use bevy::prelude::Color;

    fn tile_asset(ascii_char: &str) -> TileAsset {
        TileAsset {
            sprite: String::new(),
            grid_size: None,
            tile_size: None,
            ascii_char: ascii_char.to_string(),
            ascii_fg: Color::WHITE,
            ascii_bg: Color::BLACK,
        }
    }

    fn manifest_with(entries: &[(&str, &str)]) -> TileManifest {
        let mut tiles = HashMap::new();
        for (name, glyph) in entries {
            tiles.insert((*name).to_string(), tile_asset(glyph));
        }
        TileManifest { tiles }
    }

    #[test]
    fn portal_glyph_beats_decoration() {
        // Regression guard: a Portal tile with a grass decoration must render
        // the portal glyph, not the grass. Before the fix, decoration won.
        let manifest = manifest_with(&[("Portal", "Ω"), ("Grass", "\"")]);
        let tile = Tile {
            terrain: TerrainType::Portal,
            liquid: LiquidType::None,
            decoration: Decoration::Grass,
        };
        let (glyph, _, _) = resolve_tile_display(tile, &manifest);
        assert_eq!(glyph, "Ω");
    }

    #[test]
    fn stairs_glyph_beats_decoration() {
        // Parity check: the same priority the portal now shares with stairs.
        let manifest = manifest_with(&[("DownStairs", ">"), ("Grass", "\"")]);
        let tile = Tile {
            terrain: TerrainType::DownStairs,
            liquid: LiquidType::None,
            decoration: Decoration::Grass,
        };
        let (glyph, _, _) = resolve_tile_display(tile, &manifest);
        assert_eq!(glyph, ">");
    }

    #[test]
    fn decoration_still_beats_plain_floor() {
        // Decorations should still override ordinary floor — we didn't
        // break the normal case by adding Portal to the priority list.
        let manifest = manifest_with(&[("Floor", "."), ("Grass", "\"")]);
        let tile = Tile {
            terrain: TerrainType::Floor,
            liquid: LiquidType::None,
            decoration: Decoration::Grass,
        };
        let (glyph, _, _) = resolve_tile_display(tile, &manifest);
        assert_eq!(glyph, "\"");
    }
}
