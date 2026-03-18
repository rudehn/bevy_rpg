use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, BaseMap, DistanceAlg, Point, SmallVec};

use crate::{
    components::{Collider, Position, Viewshed},
    game::AppState,
    map::{
        light::LightMap,
        tile::{Decoration, TileExplored, TileMarker, Tile, TerrainType, LiquidType, TileVisibility, is_opaque},
    },
    player::Player,
    ui::game_log::GameLogMessage,
};

/*
There are two map types.

1. The Map struct defined here. This grid based map handles all game logic, from map generation
   to collision and fog of war.

2. The ECS Entity tiles. This handles the rendering of all entities on the level.
   This handles sprites, visibility, pixel location, etc.

*/
pub const GRID_SIZE: Vec2 = Vec2 { x: 16.0, y: 16.0 };
pub const MAP_SIZE: UVec2 = UVec2 { x: 80, y: 60 };

/// Minimum brightness for tiles currently in the player's FOV but not near a candle.
/// High enough that lighting enhances atmosphere rather than gating visibility.
const AMBIENT: f32 = 0.55;

#[derive(Message, Debug, Clone, Copy)]
pub struct RevealMapMessage;

/// Set to true by spawn_dungeon when restoring a saved or cached floor so that
/// previously-explored tiles are rendered as dim/explored instead of hidden.
#[derive(Resource, Default)]
pub struct NeedsExploredInit(pub bool);

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Map::default())
            .init_resource::<NeedsExploredInit>()
            .add_message::<RevealMapMessage>()
            .add_systems(
                Update,
                (
                    init_explored_tiles_system,
                    update_tile_visibility
                        .after(crate::map::light::rebuild_light_map_system)
                        // Skip during floor transitions: stale old-floor tile entities
                        // would be processed with new-floor FOV, marking wrong tiles explored.
                        // init_explored_tiles_system clears this flag once new tiles are ready.
                        .run_if(|init: Res<NeedsExploredInit>| !init.0)
                        .after(init_explored_tiles_system),
                    handle_reveal_map_system.run_if(on_message::<RevealMapMessage>),
                ).run_if(in_state(AppState::InGame)),
            );
    }
}

// Tag for the entity that holds the map storage
#[derive(Component)]
pub struct DungeonECSMap; // Tag for entity holding the active ECS map marker

// --------------------------------------------------------------------------------
// SYSTEMS
// --------------------------------------------------------------------------------

pub fn handle_reveal_map_system(
    mut messages: MessageReader<RevealMapMessage>,
    mut tile_render_query: Query<(&mut TileExplored, &mut Sprite, &mut Visibility)>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for _ in messages.read() {
        for (mut tile_explored, mut sprite, mut visibility) in tile_render_query.iter_mut() {
            *tile_explored = TileExplored::Explored;
            *visibility = Visibility::Visible;
            sprite.color = Color::srgb(0.5, 0.5, 0.5);
        }
        log_writer.write(GameLogMessage("The map has been revealed!".to_string()));
    }
}

/// Initializes tile explored state from the Map resource after a save load or
/// floor restore. Runs once (flag is cleared immediately after).
pub fn init_explored_tiles_system(
    mut needs_init: ResMut<NeedsExploredInit>,
    map: Res<Map>,
    mut tile_query: Query<(&Position, &mut TileExplored, &mut Sprite, &mut Visibility)>,
) {
    if !needs_init.0 {
        return;
    }
    // New tile entities are spawned via deferred commands — if none exist yet,
    // wait until next frame when the command queue has been flushed.
    if tile_query.is_empty() {
        return;
    }
    needs_init.0 = false;

    for (pos, mut tile_explored, mut sprite, mut visibility) in tile_query.iter_mut() {
        let idx = map.xy_idx(pos.x, pos.y);
        if map.explored_tiles.get(idx).copied().unwrap_or(false) {
            *tile_explored = TileExplored::Explored;
            *visibility = Visibility::Visible;
            sprite.color = Color::srgb(0.5, 0.5, 0.5);
        }
    }
}

/// Add warm light to a base color. `light_amount` is 0.0 (no light) to 1.0 (max light).
/// Uses additive warm tint: candles add (warm_r, warm_g, warm_b) scaled by light_amount.
fn apply_light_to_color(base: Color, light_amount: f32) -> Color {
    let srgba = base.to_srgba();
    // Warm candle tint: add up to (0.15, 0.10, 0.03) at full light
    let r = (srgba.red + light_amount * 0.15).min(1.0);
    let g = (srgba.green + light_amount * 0.10).min(1.0);
    let b = (srgba.blue + light_amount * 0.03).min(1.0);
    Color::srgba(r, g, b, srgba.alpha)
}

/// Dim a color for explored-but-not-visible tiles.
fn dim_color(base: Color, factor: f32) -> Color {
    let srgba = base.to_srgba();
    Color::srgba(srgba.red * factor, srgba.green * factor, srgba.blue * factor, srgba.alpha)
}

pub fn update_tile_visibility(
    player_query: Query<&Viewshed, With<Player>>,
    viewshed_changed: Query<(), (With<Player>, Changed<Viewshed>)>,
    mut map: ResMut<Map>,
    light_map: Res<LightMap>,
    mode: Res<crate::game::ascii_mode::GraphicsMode>,
    mut sprite_set: ParamSet<(
        Query<(
            &Position,
            &mut TileVisibility,
            &mut TileExplored,
            &mut Sprite,
            &mut Visibility,
            Option<&Children>,
        )>,
        Query<(&mut Sprite, &crate::game::ascii_mode::AsciiBackground)>,
    )>,
    mut ascii_glyph_query: Query<(&mut TextColor, &crate::game::ascii_mode::AsciiGlyphColor), With<crate::game::ascii_mode::AsciiGlyph>>,
) {
    // Run when viewshed changes OR graphics mode changes
    let viewshed_dirty = !viewshed_changed.is_empty();
    let mode_dirty = mode.is_changed();
    if !viewshed_dirty && !mode_dirty {
        return;
    }

    let Ok(player_viewshed) = player_query.single() else {
        return;
    };

    let fov_tiles = &player_viewshed.visible_tiles;
    let is_ascii = *mode == crate::game::ascii_mode::GraphicsMode::Ascii;

    let mut newly_explored = Vec::new();
    // Deferred ASCII child updates: (entity, light_amount, is_explored_dim).
    // Applied after the tile query loop via ParamSet to avoid Sprite borrow conflict.
    let mut ascii_child_updates: Vec<(Entity, f32, bool)> = Vec::new();

    for (tile_pos, mut tile_visibility, mut tile_explored, mut sprite, mut visibility, children) in
        sprite_set.p0().iter_mut()
    {
        let current_point = Point::new(tile_pos.x, tile_pos.y);

        if fov_tiles.contains(&current_point) {
            *tile_visibility = TileVisibility::Visible;
            *tile_explored = TileExplored::Explored;
            *visibility = Visibility::Visible;

            let light = if map.in_bounds(current_point) {
                let idx = map.xy_idx(tile_pos.x, tile_pos.y);
                if !map.explored_tiles[idx] {
                    newly_explored.push(idx);
                }
                light_map.values.get(idx).copied().unwrap_or(0.0).max(AMBIENT)
            } else {
                AMBIENT
            };

            if is_ascii {
                sprite.color = Color::NONE;
                // Additive light: 0.0 at ambient, 1.0 at max candle brightness
                let light_amount = ((light - AMBIENT) / (1.0 - AMBIENT)).clamp(0.0, 1.0);
                if let Some(children) = children {
                    for child in children.iter() {
                        ascii_child_updates.push((child, light_amount, false));
                    }
                }
            } else {
                sprite.color = Color::srgb(light, light * 0.95, light * 0.8);
            }
        } else {
            *tile_visibility = TileVisibility::Hidden;
            if *tile_explored == TileExplored::Explored {
                *visibility = Visibility::Visible;
                if is_ascii {
                    sprite.color = Color::NONE;
                    if let Some(children) = children {
                        for child in children.iter() {
                            ascii_child_updates.push((child, 0.0, true));
                        }
                    }
                } else {
                    sprite.color = Color::srgb(0.5, 0.5, 0.5);
                }
            } else {
                *visibility = Visibility::Hidden;
                sprite.color = Color::BLACK;
            }
        }
    }

    // Apply deferred ASCII child color updates via ParamSet's second query.
    {
        let mut bg_q = sprite_set.p1();
        for &(entity, light_amount, is_dim) in &ascii_child_updates {
            if let Ok((mut bg_sprite, bg_data)) = bg_q.get_mut(entity) {
                bg_sprite.color = if is_dim {
                    dim_color(bg_data.base_color, 0.35)
                } else {
                    apply_light_to_color(bg_data.base_color, light_amount)
                };
            }
        }
    }
    for &(entity, light_amount, is_dim) in &ascii_child_updates {
        if let Ok((mut text_color, glyph_data)) = ascii_glyph_query.get_mut(entity) {
            *text_color = TextColor(if is_dim {
                dim_color(glyph_data.0, 0.45)
            } else {
                apply_light_to_color(glyph_data.0, light_amount)
            });
        }
    }

    if !newly_explored.is_empty() {
        for idx in newly_explored {
            map.explored_tiles[idx] = true;
        }
    }
}

/// Marks tiles occupied by `Collider` entities so that A* pathfinding treats
/// them as high-cost, causing monsters to route around each other.
pub fn populate_blocked_tiles(
    mut map: ResMut<Map>,
    collider_query: Query<&Position, With<Collider>>,
) {
    // Clear all blocked flags.
    for b in map.blocked.iter_mut() {
        *b = false;
    }

    // Mark tiles occupied by collider entities.
    for pos in collider_query.iter() {
        let pt = Point::new(pos.x, pos.y);
        if map.in_bounds(pt) {
            let idx = map.xy_idx(pos.x, pos.y);
            map.blocked[idx] = true;
        }
    }
}

#[derive(Default, Clone, Resource)]
pub struct Map {
    pub name: String,
    pub tiles: Vec<Tile>,
    /// Mirrors `tiles` index-for-index: true once the player has seen that tile.
    pub explored_tiles: Vec<bool>,
    /// Mirrors `tiles` index-for-index: true when an entity with `Collider` occupies
    /// this tile. Populated each frame by `populate_blocked_tiles` before AI runs.
    /// Pathfinding treats blocked tiles as high-cost rather than impassable so
    /// monsters route around each other instead of lining up.
    pub blocked: Vec<bool>,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
}

impl Map {
    /// Creates a new map of the given size, with all tiles set to `Wall`.
    pub fn new<S: ToString>(depth: i32, width: i32, height: i32, name: S) -> Self {
        let map_tile_count = (width * height) as usize;
        Self {
            name: name.to_string(),
            tiles: vec![Tile { terrain: TerrainType::Wall, liquid: LiquidType::None, decoration: Decoration::None }; map_tile_count],
            explored_tiles: vec![false; map_tile_count],
            blocked: vec![false; map_tile_count],
            width,
            height,
            depth,
        }
    }

    pub fn xy_idx(&self, x: i32, y: i32) -> usize {
        (y as usize * self.width as usize) + x as usize
    }

    pub fn idx_xy(&self, idx: usize) -> (i32, i32) {
        (idx as i32 % self.width, idx as i32 / self.width)
    }
    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn depth(&self) -> i32 {
        self.depth
    }

    pub fn get_tile(&self, pt: Point) -> Option<Tile> {
        if self.in_bounds(pt) {
            let idx = self.xy_idx(pt.x, pt.y);
            Some(self.tiles[idx])
        } else {
            None
        }
    }

    pub fn set_tile(&mut self, pt: Point, terrain: TerrainType) {
        if self.in_bounds(pt) {
            let idx = self.xy_idx(pt.x, pt.y);
            self.tiles[idx].terrain = terrain;
        }
    }

    #[allow(dead_code)]
    pub fn set_liquid(&mut self, pt: Point, liquid: LiquidType) {
        if self.in_bounds(pt) {
            let idx = self.xy_idx(pt.x, pt.y);
            self.tiles[idx].liquid = liquid;
        }
    }

    /// Determine the cost to move to this cell
    /// None: Can't move to this cell
    /// Some(f32): The cost to move to this cell (usually 1.0 for normal terrain)
    pub fn get_pathing_cost(&self, x: i32, y: i32) -> Option<f32> {
        if !self.in_bounds(Point::new(x, y)) {
            return None;
        }

        let idx = self.xy_idx(x, y);
        let tile = self.tiles[idx];

        // Topologically passable: anywhere an entity *could* go, or doors.
        if !crate::map::tile::is_passable(tile) {
            return None;
        }

        // High cost for tiles occupied by another entity — A* will prefer
        // routing around rather than queuing behind.
        if self.blocked.get(idx).copied().unwrap_or(false) {
            return Some(10.0);
        }

        Some(1.0)
    }
}

impl BaseMap for Map {
    fn is_opaque(&self, idx: usize) -> bool {
        is_opaque(self.tiles[idx])
    }

    fn get_available_exits(
        &self,
        idx: usize,
    ) -> bracket_lib::prelude::SmallVec<[(usize, f32); 10]> {
        let mut exits = SmallVec::new();
        let (x, y) = self.idx_xy(idx);

        // Check all 8 directions
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue; // Skip current position
                }

                let nx = x + dx;
                let ny = y + dy;

                if let Some(base_cost) = self.get_pathing_cost(nx, ny) {
                    let next_idx = self.xy_idx(nx, ny);
                    // Diagonal moves cost slightly more
                    let cost = if dx != 0 && dy != 0 { base_cost * 1.45 } else { base_cost };
                    exits.push((next_idx, cost));
                }
            }
        }
        exits
    }

    fn get_pathing_distance(&self, idx1: usize, idx2: usize) -> f32 {
        let (x1, y1) = self.idx_xy(idx1);
        let (x2, y2) = self.idx_xy(idx2);
        DistanceAlg::Pythagoras.distance2d(Point::new(x1, y1), Point::new(x2, y2))
    }
}

impl Algorithm2D for Map {
    fn dimensions(&self) -> Point {
        Point::new(self.width, self.height)
    }

    fn point2d_to_index(&self, pt: Point) -> usize {
        self.xy_idx(pt.x, pt.y)
    }

    fn index_to_point2d(&self, idx: usize) -> Point {
        Point::new(idx as i32 % self.width, idx as i32 / self.width)
    }
}
