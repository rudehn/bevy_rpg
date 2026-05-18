//! Town NPC placement builder.
//!
//! Reads [`assets/town_npcs.ron`] and queues `SpawnEntry`s for each
//! NPC count + placement strategy. Spawn entries carry a
//! `PatrolRoute` (`AreaRoam` for drunks, `Sentry` for vendors) so the
//! materializer attaches the right roaming behaviour to the
//! resulting entity.
//!
//! **Separation of concerns**: the NPC asset itself (in
//! `assets/monsters.ron`) declares *what* the NPC is — name, glyph,
//! AI tuning, faction. This builder owns *where* and *how broadly* it
//! roams. Adding a new NPC type to the town = one entry in
//! `town_npcs.ron` plus its row in `monsters.ron`. No code change.

use bracket_lib::prelude::{Point, Rect};
use serde::Deserialize;

use bevy::prelude::*;

use roguelike_engine::components::PatrolState;

use crate::components::Position;
use crate::map::builders::{BuilderMap, BuilderPhase, MetaMapBuilder, SpawnEntry};
use crate::map::tile::{LiquidType, TerrainType};

// =====================================================================
// Asset schema — keyed off `assets/town_npcs.ron`
// =====================================================================

/// Top-level RON manifest. Each entry says "spawn N of `<npc>` via
/// `<placement>`". The `npc` string keys into `monsters.ron` (NPCs
/// reuse the monster pipeline; their peaceful behaviour is enforced
/// by their `faction:` field, not by a separate spawn path).
#[derive(Asset, TypePath, Deserialize, Debug, Clone, Default)]
pub struct TownNpcManifest {
    #[serde(default)]
    pub spawns: Vec<TownNpcSpawn>,
}

/// One spawn directive in `town_npcs.ron`.
#[derive(Deserialize, Debug, Clone)]
pub struct TownNpcSpawn {
    /// Lookup key into `monsters.ron`.
    pub npc: String,
    /// How many of this NPC to place.
    pub count: u32,
    /// Where on the map to place them + what their roaming bounds are.
    pub placement: TownNpcPlacement,
}

/// Placement strategy. The town builder owns the geometry; this enum
/// is a stable contract between authored RON and the placement code
/// in [`TownNpcBuilder::place_one`].
///
/// Phase 1 ships `AnywhereInTown`. Phase 2+ adds `Pier`, then
/// `BuildingInterior(role)` for vendors.
#[derive(Deserialize, Debug, Clone, Copy)]
pub enum TownNpcPlacement {
    /// Random walkable Floor tile east of the water strip, outside
    /// every building interior. Roam bounds = the entire land
    /// portion of the town.
    AnywhereInTown,
    // Future: Pier { index: Option<u8> }, BuildingInterior(BuildingRole), ...
}

/// Asset handle resource. Populated by [`load_town_npc_manifest`] on
/// `OnEnter(AppState::Loading)`.
#[derive(Resource, Default)]
pub struct TownNpcManifestHandle(pub Handle<TownNpcManifest>);

// =====================================================================
// Builder
// =====================================================================

/// Queues NPC `SpawnEntry`s onto `BuilderMap.spawn_list`. Each entry
/// carries a `PatrolRoute` matching the chosen placement so the
/// materializer wires the right roaming behaviour onto the spawned
/// entity. NPCs themselves are stored in `monsters.ron`; this builder
/// just decides *where* they appear and *how broadly* they wander.
pub struct TownNpcBuilder {
    spawns: Vec<TownNpcSpawn>,
}

impl TownNpcBuilder {
    pub fn new(spawns: Vec<TownNpcSpawn>) -> Box<Self> {
        Box::new(Self { spawns })
    }
}

impl MetaMapBuilder for TownNpcBuilder {
    fn phase(&self) -> Option<BuilderPhase> {
        // After Layout + Portal + Stairs + Paths so the placement
        // pass can read every existing tile + building rect and
        // avoid stomping on them.
        Some(BuilderPhase::Finalization)
    }

    fn build_map(&mut self, build: &mut BuilderMap) {
        // Snapshot the spawns up front; iterating self.spawns while
        // mutating build would be a double-mut hazard.
        let spawns = self.spawns.clone();
        for spawn in &spawns {
            for _ in 0..spawn.count {
                self.place_one(build, spawn);
            }
        }
    }
}

impl TownNpcBuilder {
    /// Pick a position + patrol bounds for one NPC and push the
    /// resulting `SpawnEntry` onto `build.spawn_list`. Silently
    /// gives up after a placement attempts budget — better one
    /// missing drunk than a hung builder on a degenerate town.
    fn place_one(&self, build: &mut BuilderMap, spawn: &TownNpcSpawn) {
        match spawn.placement {
            TownNpcPlacement::AnywhereInTown => {
                let Some(pos) = pick_anywhere_in_town(build) else {
                    bevy::log::warn!(
                        "TownNpcBuilder: could not find an open tile for '{}' (AnywhereInTown)",
                        spawn.npc,
                    );
                    return;
                };
                let bounds = town_interior_bounds(build);
                let patrol = roguelike_engine::components::PatrolRoute {
                    state: PatrolState::AreaRoam {
                        min: (bounds.x1, bounds.y1),
                        max: (bounds.x2, bounds.y2),
                    },
                };
                build.spawn_list.push(SpawnEntry {
                    pos: Point::new(pos.x, pos.y),
                    name: spawn.npc.clone(),
                    squad_id: None,
                    squad_config: None,
                    is_leader: false,
                    patrol_route: Some(patrol),
                });
            }
        }
    }
}

// =====================================================================
// Placement helpers
// =====================================================================

/// The walkable land box of the town — everywhere east of the water
/// strip, minus the outer wall. Drunks roam within this rectangle.
fn town_interior_bounds(build: &BuilderMap) -> Rect {
    use crate::map::builders::town;
    Rect::with_exact(
        town::WATER_EAST_EDGE + 1,
        1,
        build.width - 2,
        build.height - 2,
    )
}

/// Pick a random walkable Floor tile suitable for an NPC. Avoids
/// water, building interiors, stairs, the portal, and any tile
/// already claimed by a prior spawn — we don't want two drunks
/// starting in the same square.
fn pick_anywhere_in_town(build: &mut BuilderMap) -> Option<Position> {
    const MAX_TRIES: u32 = 128;
    let bounds = town_interior_bounds(build);
    let occupied = collect_occupied_positions(build);

    for _ in 0..MAX_TRIES {
        let x = build.rng.range(bounds.x1, bounds.x2);
        let y = build.rng.range(bounds.y1, bounds.y2);
        if occupied.contains(&(x, y)) { continue; }
        if !is_open_town_tile(build, x, y) { continue; }
        if inside_any_building(build, x, y) { continue; }
        return Some(Position { x, y });
    }
    None
}

fn collect_occupied_positions(build: &BuilderMap) -> std::collections::HashSet<(i32, i32)> {
    let mut occupied: std::collections::HashSet<(i32, i32)> =
        std::collections::HashSet::new();
    for entry in &build.spawn_list {
        occupied.insert((entry.pos.x, entry.pos.y));
    }
    occupied
}

/// A "town tile" the NPC may start on: plain walkable Floor, with no
/// liquid (so we don't drop a drunk into the harbour). Stairs and
/// the portal are explicitly excluded — players need a clear
/// approach.
fn is_open_town_tile(build: &BuilderMap, x: i32, y: i32) -> bool {
    if x <= 0 || y <= 0 || x >= build.width - 1 || y >= build.height - 1 {
        return false;
    }
    let idx = build.map.xy_idx(x, y);
    let tile = build.map.tiles[idx];
    if tile.liquid != LiquidType::None { return false; }
    matches!(tile.terrain, TerrainType::Floor)
}

fn inside_any_building(build: &BuilderMap, x: i32, y: i32) -> bool {
    let Some(rooms) = &build.rooms else { return false; };
    rooms.iter().any(|r| {
        // Building interior = strictly inside the wall border.
        x > r.x1 && x < r.x2 - 1 && y > r.y1 && y < r.y2 - 1
    })
}

// =====================================================================
// Asset loader (wired by `LoadingPlugin` in `src/assets/mod.rs`)
// =====================================================================

pub fn load_town_npc_manifest(
    asset_server: Res<AssetServer>,
    mut handle: ResMut<TownNpcManifestHandle>,
) {
    handle.0 = asset_server.load("town_npcs.ron");
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::Decoration;

    #[test]
    fn manifest_parses_with_one_entry() {
        let ron = r#"(spawns: [
            ( npc: "Drunken Sailor", count: 3, placement: AnywhereInTown ),
        ])"#;
        let manifest: TownNpcManifest =
            ron::from_str(ron).expect("town_npcs.ron must parse");
        assert_eq!(manifest.spawns.len(), 1);
        assert_eq!(manifest.spawns[0].npc, "Drunken Sailor");
        assert_eq!(manifest.spawns[0].count, 3);
        assert!(matches!(
            manifest.spawns[0].placement,
            TownNpcPlacement::AnywhereInTown
        ));
    }

    #[test]
    fn empty_manifest_parses() {
        let manifest: TownNpcManifest =
            ron::from_str("(spawns: [])").expect("empty must parse");
        assert!(manifest.spawns.is_empty());
    }

    /// `TownNpcBuilder` with N drunks must queue N entries onto
    /// `spawn_list`. Each entry references the NPC name, sits on
    /// open Floor, and carries an `AreaRoam` patrol route.
    #[test]
    fn builder_queues_n_entries_with_area_roam_route() {
        let mut bm = open_town_for_test(80, 60);
        let mut builder = TownNpcBuilder {
            spawns: vec![TownNpcSpawn {
                npc: "Drunken Sailor".to_string(),
                count: 3,
                placement: TownNpcPlacement::AnywhereInTown,
            }],
        };
        builder.build_map(&mut bm);
        assert_eq!(bm.spawn_list.len(), 3);
        for entry in &bm.spawn_list {
            assert_eq!(entry.name, "Drunken Sailor");
            assert!(matches!(
                &entry.patrol_route,
                Some(roguelike_engine::components::PatrolRoute {
                    state: PatrolState::AreaRoam { .. }
                })
            ));
            // Position must be on Floor.
            let idx = bm.map.xy_idx(entry.pos.x, entry.pos.y);
            assert_eq!(bm.map.tiles[idx].terrain, TerrainType::Floor);
            assert_eq!(bm.map.tiles[idx].liquid, LiquidType::None);
        }
    }

    /// Two NPCs in the same builder run shouldn't share a tile.
    #[test]
    fn npc_positions_are_unique_within_one_run() {
        let mut bm = open_town_for_test(80, 60);
        let mut builder = TownNpcBuilder {
            spawns: vec![TownNpcSpawn {
                npc: "Drunken Sailor".to_string(),
                count: 5,
                placement: TownNpcPlacement::AnywhereInTown,
            }],
        };
        builder.build_map(&mut bm);
        let mut seen = std::collections::HashSet::new();
        for entry in &bm.spawn_list {
            assert!(
                seen.insert((entry.pos.x, entry.pos.y)),
                "duplicate NPC position ({}, {})", entry.pos.x, entry.pos.y,
            );
        }
    }

    // Builds a town-ish BuilderMap for tests: open Floor everywhere
    // east of WATER_EAST_EDGE, water everywhere west. No buildings
    // (so placement has a clear field), no stairs (so the test
    // doesn't depend on those builders running).
    fn open_town_for_test(w: i32, h: i32) -> BuilderMap {
        use crate::map::builders::town;
        let mut bm = BuilderMap::new_for_test(w, h);
        for y in 0..h {
            for x in 0..w {
                let idx = bm.map.xy_idx(x, y);
                let liquid = if x < town::WATER_EAST_EDGE {
                    LiquidType::Water
                } else {
                    LiquidType::None
                };
                bm.map.tiles[idx] = crate::map::tile::Tile {
                    terrain: TerrainType::Floor,
                    liquid,
                    decoration: Decoration::None,
                };
            }
        }
        bm.rooms = Some(vec![]);
        bm
    }
}
