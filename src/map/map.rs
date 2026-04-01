use bevy::prelude::*;
use bracket_lib::prelude::{Algorithm2D, BaseMap, DistanceAlg, Point, SmallVec};

use crate::{
    components::{Collider, MovementMode, Position, Viewshed},
    game::AppState,
    map::{
        light::LightMap,
        tile::{Decoration, TileExplored, Tile, TerrainType, LiquidType, TileVisibility, is_opaque, is_passable, is_pathing_blocker, can_entity_enter_tile},
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
            .init_resource::<crate::map::tile::TileEntityIndex>()
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
    mut map: ResMut<Map>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    for _ in messages.read() {
        // Mark all tiles as explored in the Map resource so the state
        // survives save/load and floor transitions.
        for flag in map.explored_tiles.iter_mut() {
            *flag = true;
        }
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
    omniscient: Res<crate::game::systems::Omniscient>,
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
        Query<&mut Sprite, (With<crate::game::ascii_mode::LiquidOverlay>, Without<crate::game::ascii_mode::AsciiBackground>)>,
    )>,
    mut ascii_glyph_query: Query<(&mut TextColor, &crate::game::ascii_mode::AsciiGlyphColor), With<crate::game::ascii_mode::AsciiGlyph>>,
) {
    // Run when viewshed changes, graphics mode changes, or omniscient toggles.
    let viewshed_dirty = !viewshed_changed.is_empty();
    let mode_dirty = mode.is_changed();
    let omni_dirty = omniscient.is_changed();
    if !viewshed_dirty && !mode_dirty && !omni_dirty {
        return;
    }

    let Ok(player_viewshed) = player_query.single() else {
        return;
    };

    let fov_tiles = &player_viewshed.visible_tiles;
    let is_ascii = *mode == crate::game::ascii_mode::GraphicsMode::Ascii;
    let omni = omniscient.0;

    let mut newly_explored = Vec::new();
    // Deferred ASCII child updates: (entity, light_amount, is_explored_dim).
    // Applied after the tile query loop via ParamSet to avoid Sprite borrow conflict.
    let mut ascii_child_updates: Vec<(Entity, f32, bool)> = Vec::new();
    // Deferred liquid overlay child updates: (entity, light_level, is_explored_dim, is_hidden).
    let mut liquid_child_updates: Vec<(Entity, f32, bool, bool)> = Vec::new();

    for (tile_pos, mut tile_visibility, mut tile_explored, mut sprite, mut visibility, children) in
        sprite_set.p0().iter_mut()
    {
        let current_point = Point::new(tile_pos.x, tile_pos.y);

        if omni || fov_tiles.contains(&current_point) {
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
                // Tint liquid overlay children to match the parent tile lighting
                if let Some(children) = children {
                    for child in children.iter() {
                        liquid_child_updates.push((child, light, false, false));
                    }
                }
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
                    // Dim liquid overlay children to match the explored-but-not-visible state
                    if let Some(children) = children {
                        for child in children.iter() {
                            liquid_child_updates.push((child, 0.0, true, false));
                        }
                    }
                }
            } else {
                *visibility = Visibility::Hidden;
                sprite.color = Color::BLACK;
                // Hide liquid overlay children when tile is unexplored
                if let Some(children) = children {
                    for child in children.iter() {
                        liquid_child_updates.push((child, 0.0, false, true));
                    }
                }
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

    // Apply deferred liquid overlay sprite tinting via ParamSet's third query.
    {
        let mut liquid_q = sprite_set.p2();
        for &(entity, light, is_dim, _is_hidden) in &liquid_child_updates {
            if let Ok(mut liquid_sprite) = liquid_q.get_mut(entity) {
                if is_dim {
                    liquid_sprite.color = Color::srgb(0.5, 0.5, 0.5);
                } else {
                    liquid_sprite.color = Color::srgb(light, light * 0.95, light * 0.8);
                }
            }
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

        // Pathing blockers (deep water, lava, chasm): AI avoids but CAN
        // path through as a last resort. Cost 5x normal — enough to prefer
        // dry routes but not so high that A* gives up or hits iteration limits.
        if crate::map::tile::is_pathing_blocker(tile) {
            return Some(5.0);
        }

        // High cost for tiles occupied by another entity — A* will prefer
        // routing around rather than queuing behind.
        if self.blocked.get(idx).copied().unwrap_or(false) {
            return Some(10.0);
        }

        // Decoration movement cost (cobwebs, tall grass, etc.)
        let dec_cost = tile.decoration.movement_cost();
        if dec_cost > 1.0 {
            return Some(dec_cost);
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

/// Wraps a `Map` reference with a `MovementMode` to provide mode-aware
/// pathfinding costs via bracket-lib's `BaseMap` trait.
pub struct MapWithMode<'a> {
    pub map: &'a Map,
    pub mode: MovementMode,
}

impl<'a> MapWithMode<'a> {
    /// Mode-aware pathing cost for a single cell.
    fn get_pathing_cost(&self, x: i32, y: i32) -> Option<f32> {
        if !self.map.in_bounds(Point::new(x, y)) {
            return None;
        }

        let idx = self.map.xy_idx(x, y);
        let tile = self.map.tiles[idx];

        // Topological passability gate (same for all modes).
        if !is_passable(tile) {
            return None;
        }

        match self.mode {
            MovementMode::Land => {
                // Land creatures cannot pathfind through deep water.
                // (Players bypass A* via direct bump movement, so this
                // only prevents monster AI from wading into water.)
                if tile.liquid == LiquidType::Water {
                    return None;
                }
                if is_pathing_blocker(tile) {
                    return Some(5.0);
                }
                if self.map.blocked.get(idx).copied().unwrap_or(false) {
                    return Some(10.0);
                }
                let dec_cost = tile.decoration.movement_cost();
                if dec_cost > 1.0 { return Some(dec_cost); }
                Some(1.0)
            }
            MovementMode::ImmuneToWater => {
                // Water is free; other blockers still penalized.
                if tile.liquid == LiquidType::Water {
                    return Some(tile.decoration.movement_cost().max(1.0));
                }
                if is_pathing_blocker(tile) {
                    return Some(5.0);
                }
                if self.map.blocked.get(idx).copied().unwrap_or(false) {
                    return Some(10.0);
                }
                let dec_cost = tile.decoration.movement_cost();
                if dec_cost > 1.0 { return Some(dec_cost); }
                Some(1.0)
            }
            MovementMode::RestrictedToLiquid => {
                // Can only enter tiles with liquid that are walkable.
                if !can_entity_enter_tile(tile, self.mode) {
                    return None;
                }
                if self.map.blocked.get(idx).copied().unwrap_or(false) {
                    return Some(10.0);
                }
                let dec_cost = tile.decoration.movement_cost();
                if dec_cost > 1.0 { return Some(dec_cost); }
                Some(1.0)
            }
        }
    }
}

impl<'a> BaseMap for MapWithMode<'a> {
    fn is_opaque(&self, idx: usize) -> bool {
        is_opaque(self.map.tiles[idx])
    }

    fn get_available_exits(&self, idx: usize) -> SmallVec<[(usize, f32); 10]> {
        let mut exits = SmallVec::new();
        let (x, y) = self.map.idx_xy(idx);

        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x + dx;
                let ny = y + dy;
                if let Some(base_cost) = self.get_pathing_cost(nx, ny) {
                    let next_idx = self.map.xy_idx(nx, ny);
                    let cost = if dx != 0 && dy != 0 { base_cost * 1.45 } else { base_cost };
                    exits.push((next_idx, cost));
                }
            }
        }
        exits
    }

    fn get_pathing_distance(&self, idx1: usize, idx2: usize) -> f32 {
        let (x1, y1) = self.map.idx_xy(idx1);
        let (x2, y2) = self.map.idx_xy(idx2);
        DistanceAlg::Pythagoras.distance2d(Point::new(x1, y1), Point::new(x2, y2))
    }
}

impl<'a> Algorithm2D for MapWithMode<'a> {
    fn dimensions(&self) -> Point {
        Point::new(self.map.width, self.map.height)
    }

    fn point2d_to_index(&self, pt: Point) -> usize {
        self.map.xy_idx(pt.x, pt.y)
    }

    fn index_to_point2d(&self, idx: usize) -> Point {
        Point::new(idx as i32 % self.map.width, idx as i32 / self.map.width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::{Tile, TerrainType, LiquidType, Decoration};

    /// Create a small test map with specified tiles.
    fn make_map(width: i32, height: i32, tiles: Vec<Tile>) -> Map {
        let count = (width * height) as usize;
        assert_eq!(tiles.len(), count);
        Map {
            name: "test".to_string(),
            tiles,
            explored_tiles: vec![false; count],
            blocked: vec![false; count],
            width,
            height,
            depth: 1,
        }
    }

    fn floor() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::None, decoration: Decoration::None }
    }

    fn wall() -> Tile {
        Tile { terrain: TerrainType::Wall, liquid: LiquidType::None, decoration: Decoration::None }
    }

    fn deep_water() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::Water, decoration: Decoration::None }
    }

    fn shallow_water() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::ShallowWater, decoration: Decoration::None }
    }

    fn lava() -> Tile {
        Tile { terrain: TerrainType::Floor, liquid: LiquidType::Lava, decoration: Decoration::None }
    }

    // ---- Map::get_pathing_cost ----

    #[test]
    fn pathing_cost_floor_is_one() {
        let map = make_map(3, 3, vec![floor(); 9]);
        assert_eq!(map.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn pathing_cost_wall_is_none() {
        let map = make_map(3, 3, vec![wall(); 9]);
        assert_eq!(map.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn pathing_cost_deep_water_is_five() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water(); // center
        let map = make_map(3, 3, tiles);
        assert_eq!(map.get_pathing_cost(1, 1), Some(5.0));
    }

    #[test]
    fn pathing_cost_out_of_bounds_is_none() {
        let map = make_map(3, 3, vec![floor(); 9]);
        assert_eq!(map.get_pathing_cost(-1, 0), None);
        assert_eq!(map.get_pathing_cost(3, 0), None);
    }

    #[test]
    fn pathing_cost_blocked_tile_is_ten() {
        let mut map = make_map(3, 3, vec![floor(); 9]);
        map.blocked[4] = true;
        assert_eq!(map.get_pathing_cost(1, 1), Some(10.0));
    }

    // ---- MapWithMode pathing costs ----

    #[test]
    fn land_mode_deep_water_impassable() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };
        assert_eq!(mwm.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn land_mode_floor_costs_one() {
        let map = make_map(3, 3, vec![floor(); 9]);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn land_mode_lava_costs_five() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = lava();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(5.0));
    }

    #[test]
    fn immune_to_water_deep_water_costs_one() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::ImmuneToWater };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn immune_to_water_floor_costs_one() {
        let map = make_map(3, 3, vec![floor(); 9]);
        let mwm = MapWithMode { map: &map, mode: MovementMode::ImmuneToWater };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn restricted_to_liquid_deep_water_costs_one() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn restricted_to_liquid_dry_floor_is_impassable() {
        let map = make_map(3, 3, vec![floor(); 9]);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        assert_eq!(mwm.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn restricted_to_liquid_shallow_water_costs_one() {
        let mut tiles = vec![floor(); 9];
        tiles[4] = shallow_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        assert_eq!(mwm.get_pathing_cost(1, 1), Some(1.0));
    }

    #[test]
    fn restricted_to_liquid_wall_is_impassable() {
        let map = make_map(3, 3, vec![wall(); 9]);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };
        assert_eq!(mwm.get_pathing_cost(1, 1), None);
    }

    #[test]
    fn all_modes_wall_is_impassable() {
        let map = make_map(3, 3, vec![wall(); 9]);
        for mode in [MovementMode::Land, MovementMode::ImmuneToWater, MovementMode::RestrictedToLiquid] {
            let mwm = MapWithMode { map: &map, mode };
            assert_eq!(mwm.get_pathing_cost(1, 1), None, "mode {:?} should block walls", mode);
        }
    }

    #[test]
    fn all_modes_out_of_bounds_is_none() {
        let map = make_map(3, 3, vec![floor(); 9]);
        for mode in [MovementMode::Land, MovementMode::ImmuneToWater, MovementMode::RestrictedToLiquid] {
            let mwm = MapWithMode { map: &map, mode };
            assert_eq!(mwm.get_pathing_cost(-1, 0), None);
        }
    }

    // ---- MapWithMode::get_available_exits ----

    #[test]
    fn land_mode_excludes_water_from_exits() {
        // 3x3 map: all floor except center is deep water
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };

        // Center tile (1,1) = index 4 with deep water. Check exits FROM a corner.
        let exits = mwm.get_available_exits(0); // (0,0)
        // Land mode should NOT include deep water as an exit at all.
        let center_exit = exits.iter().find(|(idx, _)| *idx == 4);
        assert!(center_exit.is_none(), "land mode should not pathfind into deep water");
    }

    #[test]
    fn restricted_to_liquid_no_exits_to_dry_floor() {
        // 3x3 map: center is deep water, everything else is dry floor
        let mut tiles = vec![floor(); 9];
        tiles[4] = deep_water();
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };

        // From center (deep water), all exits lead to dry floor → no exits
        let exits = mwm.get_available_exits(4);
        assert!(exits.is_empty(), "restricted-to-liquid should have no exits to dry floor");
    }

    #[test]
    fn restricted_to_liquid_exits_to_adjacent_water() {
        // 3x3 map: all deep water
        let tiles = vec![deep_water(); 9];
        let map = make_map(3, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };

        // From center, should have 8 exits (all water)
        let exits = mwm.get_available_exits(4);
        assert_eq!(exits.len(), 8);
        // All costs should be 1.0 (cardinal) or 1.45 (diagonal)
        for (_, cost) in &exits {
            assert!(*cost >= 1.0 && *cost <= 1.5, "unexpected cost: {}", cost);
        }
    }

    // ---- A* pathfinding integration ----

    #[test]
    fn land_mode_paths_around_water() {
        // 5x3 map: row 1 has deep water in the middle
        // Layout (y=0 bottom, y=2 top):
        //   F F F F F   (y=2)
        //   F F W F F   (y=1)
        //   F F F F F   (y=0)
        // Land mode should find a path from (0,1) to (4,1) that goes around the water
        let mut tiles = vec![floor(); 15];
        tiles[5 + 2] = deep_water(); // (2, 1)
        let map = make_map(5, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::Land };

        let path = bracket_lib::prelude::a_star_search(
            mwm.point2d_to_index(Point::new(0, 1)),
            mwm.point2d_to_index(Point::new(4, 1)),
            &mwm,
        );
        assert!(path.success, "should find a path");
        // The path should NOT go through (2,1) if a cheaper route exists
        let water_idx = map.xy_idx(2, 1);
        // A* may or may not go through water depending on cost — at 50.0 it should avoid it
        // since going diagonally around costs about 2.9 (1.45 * 2)
        assert!(!path.steps.contains(&water_idx), "should route around deep water");
    }

    #[test]
    fn restricted_to_liquid_paths_through_water_only() {
        // 5x3: water channel through the middle row
        //   W W W W W   (y=2) -- all wall
        //   D D D D D   (y=1) -- all deep water
        //   W W W W W   (y=0) -- all wall
        let mut tiles = vec![wall(); 15];
        for x in 0..5 {
            tiles[5 + x] = deep_water(); // row 1
        }
        let map = make_map(5, 3, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };

        let path = bracket_lib::prelude::a_star_search(
            mwm.point2d_to_index(Point::new(0, 1)),
            mwm.point2d_to_index(Point::new(4, 1)),
            &mwm,
        );
        assert!(path.success, "should find a path through water channel");
        assert_eq!(path.steps.len(), 5, "should be 5 steps (0,1)->(4,1)");
    }

    #[test]
    fn restricted_to_liquid_no_path_across_dry_gap() {
        // 5x1: water, water, floor, water, water
        let tiles = vec![deep_water(), deep_water(), floor(), deep_water(), deep_water()];
        let map = make_map(5, 1, tiles);
        let mwm = MapWithMode { map: &map, mode: MovementMode::RestrictedToLiquid };

        let path = bracket_lib::prelude::a_star_search(
            mwm.point2d_to_index(Point::new(0, 0)),
            mwm.point2d_to_index(Point::new(4, 0)),
            &mwm,
        );
        assert!(!path.success, "should not find path across dry gap");
    }
}
