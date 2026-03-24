Ready for review
Select text to add comments on the plan
Dynamic Terrain & Environmental Systems — Incremental Plan
First step: Copy this plan to docs/design/INCREMENTAL_PLAN.md before beginning implementation.

Context
Brogue's most distinctive quality is that its dungeons feel alive — grass blocks sight and burns, fire spreads, pressure plates flood corridors, and lava ignites nearby foliage. Our current implementation has none of these dynamic systems. The Decoration layer is purely visual, monsters spawn without terrain awareness, and no runtime tile mutation exists beyond door opening.

This plan adds 5 features in dependency order, each independently useful.

Phase 1: Gameplay-Affecting Foliage
Goal: TallGrass blocks line of sight, hides monsters, and gets trampled when walked on.

Changes
src/map/tile.rs

is_opaque() → also returns true for Decoration::TallGrass
Add is_concealing(&self) -> bool → true for TallGrass (hides entities standing on it)
src/game/systems.rs

update_monster_visibility → monsters on concealing tiles are hidden unless player is adjacent (within 1 tile). Same FOV check otherwise.
src/game/actions.rs

handle_movement → after step completes, trample decoration:
TallGrass → Grass
Grass → DeadGrass
Update Map.tiles[idx].decoration, mark all viewsheds dirty
No new files. FOV engine (bracket-lib) automatically respects the updated is_opaque().

Save/load: No impact — Decoration already serializes as part of Tile.

Out of scope: Fire interaction (Phase 3), decoration rendering as sprite overlays.

Phase 2: Terrain-Aware Monster Spawning
Goal: Monsters with terrain affinity spawn near matching terrain (aquatic near water, spiders near cobwebs, fire creatures near lava).

Changes
src/assets/mod.rs

Add terrain_affinity: String to MonsterAsset (serde default "")
Values: "" (any), "water", "lava", "grass", "cobweb"
assets/monsters.ron

Annotate existing monsters where appropriate (e.g., spider → "cobweb")
src/map/builders/monster_spawner.rs

get_walkable_room_point() gains optional affinity: &str param
When set, first try N times to find a point adjacent to matching terrain/liquid/decoration
Affinity matching: "water" → adjacent Water/ShallowWater, "lava" → adjacent Lava, "grass" → on Grass/TallGrass decoration, "cobweb" → on Cobweb decoration
Fallback: any walkable point (current behavior)
No new files. Save/load unaffected (spawning is build-time only).

Out of scope: Monsters that can walk ON water/lava (swimming/flying movement modes).

Phase 3: Dynamic Terrain / Feature Chains
Goal: Tiles transform at runtime — grass burns when hit by fire, fire spreads to adjacent flammable tiles, lava ignites nearby grass. Turn-based fire lifecycle.

New File: src/game/terrain_effects.rs
TileIgniteMessage { pos: Point }
handle_tile_ignite     — sets decoration to Burning, sets burn timer (3-5 turns)
terrain_tick_system    — on TurnEndEvent: decrement burn timers, spread fire (30% per adjacent flammable tile), expired → ScorchedEarth
lava_adjacency_system  — on TurnEndEvent: tiles adjacent to lava with flammable decoration have 10% chance to ignite
is_flammable(dec)      — Grass, TallGrass, Cobweb, Fungus
mutate_tile_decoration — centralized helper: update Map + mark viewsheds dirty
Changes
src/map/tile.rs

Add Decoration::Burning variant
is_opaque() → true for Burning (smoke)
src/map/map.rs

Add burn_timers: Vec<u8> parallel to tiles (0 = not burning)
src/game/combat.rs

After damage applied, if DamageType::Fire and target tile has flammable decoration → emit TileIgniteMessage
src/game/turns.rs

Register TileIgniteMessage
Add handle_tile_ignite to processing chain
Add terrain_tick_system + lava_adjacency_system to TurnEndEvent handler chain (after regen)
Fire spread cap: Max 12 tiles from original ignition point (prevent whole-map burns).

Save/load:

Add burn_timers: Vec<u8> to MapSaveData and CachedFloorSave with #[serde(default)]
Decoration::Burning is a new enum variant — old saves won't contain it, so no compat issue
Out of scope: Water mechanics (flooding, steam), player-initiated fire (fire spells igniting terrain — could be added to handle_cast_spell later), terrain damage to entities standing on burning tiles (follow-up).

Phase 4: Triggers (Pressure Plates & Levers)
Goal: Step-on and bump-activated terrain features that cause tile mutations — flooding, collapsing floors, fire traps, opening/closing remote doors.

New File: src/game/triggers.rs
TriggerKind: PressurePlate, Lever
TriggerEffect: Flood(LiquidType), Collapse, Ignite, ToggleDoor
Trigger component: kind, effect, target_tiles (relative offsets), activated, repeatable
TriggerActivateMessage { trigger_entity, activator }
handle_trigger_activate — reads effect, applies terrain mutations using Phase 3 helpers
spawn_trigger helper
Changes
src/components.rs

Re-export Trigger component
src/game/actions.rs

handle_movement → after step, query for Trigger entity at destination with TriggerKind::PressurePlate. If found and not yet activated → emit TriggerActivateMessage
Add bump-to-lever: when moving into tile with Lever entity → emit TriggerActivateMessage instead of moving (parallel to bump-to-door logic)
src/game/turns.rs

Register TriggerActivateMessage, add handle_trigger_activate to processing chain after movement
src/map/builders/mod.rs

Add trigger_spawn_list: Vec<TriggerSpawnEntry> to BuilderMap
assets/prefabs.ron

Add trigger definitions to prefab templates:
triggers: [(x: 3, y: 0, kind: PressurePlate, effect: Flood(Water), targets: [(4,1),(4,2),(5,1)])]
Trigger target offsets rotate with prefab orientation
Save/load:

TriggerSaveEntry in GameSaveData with position, kind, effect, targets, activated state
Add to CachedFloorSave
Out of scope: Delayed triggers, chain reactions between triggers, player-placeable triggers, complex wiring.

Phase 5: Machine Vestibules
Goal: Entrance areas before gated encounters — barricades, guard posts, trapped approaches placed at chokepoints leading to valuable rooms.

New File: src/map/builders/vestibule_placer.rs
VestibulePlacer (MetaMapBuilder)
VestibuleSite { chokepoint_idx, direction, corridor_length }
Changes
src/map/builders/choke_map.rs

Add best_vestibule_sites(map, count) -> Vec<VestibuleSite> — finds chokepoints with enough corridor length for vestibule placement (min 4 tiles of corridor)
src/map/builders/prefab_placer.rs

Support placement: "vestibule" mode — aligns prefab along corridor axis at chokepoint
src/map/builders/mod.rs

Insert VestibulePlacer after PrefabPlacer, before MonsterSpawner
assets/prefabs.ron

Add vestibule prefab templates with:
Barricade props blocking the corridor
Guard spawn points (terrain-aware per Phase 2)
Pressure plate traps (per Phase 4)
Thematic decoration (dead grass, scorched earth, cobwebs)
placement: "vestibule", depth range, budget cost
Vestibule types (initial set):

Barricade — props blocking corridor, 1-2 guards behind
Fire Trap — pressure plate triggers ignite on tiles ahead
Flood Trap — pressure plate floods a section with water
Guard Post — widened corridor alcove with 2-3 guards and a watchfire
Save/load: No new structures — vestibules compose existing saveable elements (tiles, props, triggers, monsters).

Out of scope: Multi-room machines, machine reward scaling, fully procedural interior generation (that's Phase 5 in the existing design docs).

Dependency Graph
Phase 1 (Foliage) ──→ Phase 3 (Feature Chains) ──→ Phase 4 (Triggers) ──→ Phase 5 (Vestibules)
      │                        ↑
      └──→ Phase 2 (Spawning) ─┘
Phases 1 and 2 can be worked in parallel. Phase 3 requires Phase 1. Phase 4 requires Phase 3. Phase 5 requires all four.

Verification Plan
Phase 1
Place TallGrass in a corridor, confirm FOV is blocked (player can't see through)
Walk through TallGrass, confirm it tramples to Grass then DeadGrass
Place monster in TallGrass, confirm it's hidden when player is >1 tile away
Save/load with trampled decorations, confirm state persists
Phase 2
Add terrain_affinity: "water" to a test monster, confirm it spawns adjacent to water tiles
Confirm monsters without affinity still spawn normally
Confirm fallback works when no matching terrain exists in a room
Phase 3
Hit a grass tile with fire damage (e.g., lava splash or fire spell), confirm it ignites
Confirm fire spreads to adjacent grass over turns
Confirm fire extinguishes after burn timer (3-5 turns) → ScorchedEarth
Confirm lava adjacency ignition works (grass near lava randomly catches fire)
Confirm fire spread cap prevents whole-map burns
Save/load mid-fire, confirm burn timers persist
Phase 4
Place a prefab with a pressure plate, step on it, confirm effect fires (e.g., water floods target tiles)
Test lever bump activation
Confirm single-use plates don't re-trigger
Confirm trigger targets rotate correctly with prefab orientation
Save/load with activated triggers, confirm state persists
Phase 5
Generate a floor with vestibule placer enabled, confirm vestibules appear at chokepoints
Confirm vestibule guards spawn with terrain-aware placement
Confirm vestibule traps (pressure plates) function
Confirm vestibule budget limits prevent over-placement
Critical Files Summary
File	Phases
src/map/tile.rs	1, 3
src/game/actions.rs	1, 3, 4
src/game/systems.rs	1
src/game/turns.rs	3, 4
src/game/combat.rs	3
src/map/map.rs	3
src/assets/mod.rs	2, 4
src/map/builders/monster_spawner.rs	2
src/map/builders/choke_map.rs	5
src/map/builders/prefab_placer.rs	5
src/map/builders/mod.rs	2, 4, 5
assets/monsters.ron	2
assets/prefabs.ron	4, 5
src/save/mod.rs	3, 4
NEW src/game/terrain_effects.rs	3
NEW src/game/triggers.rs	4
NEW src/map/builders/vestibule_placer.rs	5