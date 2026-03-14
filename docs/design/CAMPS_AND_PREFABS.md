# Camps, Props & Prefabs

## Context

The [squad system](SQUAD_SYSTEM.md) gives monster groups coordinated behavior, but
squads have no spatial anchor — they wander the map with no territory. Camps give
factions a physical presence: a prefabricated structure with a watchfire, guards,
and loot that the player must decide whether to invade.

This required three new systems: **props** (non-item world entities), **prefabs**
(data-driven map templates), and **guard AI** (monsters that patrol a home position).

---

## Props

Props are non-item, non-monster entities that exist in the world. They can emit
light, block movement, and provide visual flavor.

### Data Model

- **Component**: `Prop` marker in `src/components.rs`
- **Manifest**: `assets/props.ron` — defines name, sprite, blocking, opacity, light radius/color
- **Asset types**: `PropAsset`, `PropManifest`, `PropManifestHandle`, `PropSpriteAssets` in `src/assets/mod.rs`

### Spawning

`spawn_prop()` in `src/game/spawner.rs` creates a prop entity with:
- `Prop`, `Name`, `Position`, `Sprite`, `Transform`
- `Visibility::Hidden`, `RenderLayers::layer(1)`, `FloorEntityMarker`, `GameEntityMarker`
- Light-emitting props (`light_radius.is_some()`) also get `Candle` + `AnimationTimer`, reusing the existing candle lighting infrastructure
- Blocking props get `Collider`

### Visibility

`update_prop_visibility` in `src/game/systems.rs` follows the same pattern as
item visibility: visible in FOV, dimmed in explored-but-not-visible tiles, hidden
in unexplored tiles.

### Map Builder Integration

`BuilderMap` has a `prop_spawn_list: Vec<(Point, String)>` field. Prefabs and
other builders populate this list; `spawn_dungeon_entities` iterates it and calls
`spawn_prop`.

### Persistence

- `CachedFloor` / `CachedFloorSave`: `prop_list` field preserves props across floor transitions
- `GameSaveData`: `props: Vec<PropEntry>` with `#[serde(default)]` for backward compatibility

### Current Props

| Name | Light | Blocking | Notes |
|------|-------|----------|-------|
| candle | Yes (radius 30) | No | Reuses existing candle light system |
| watchfire | Yes (radius 40) | No | Camp centerpiece, warm orange glow |
| totem_pole | No | Yes | Decorative blocker |
| barricade | No | Yes | Tactical obstacle |

---

## Guard AI

Guards are monsters with a `home_position` that patrol near their post and return
after chasing the player.

### MonsterAI Changes

**New mode**: `MonsterAIMode::Guarding`

**New field**: `home_position: Option<Point>` on `MonsterAI`

**Constructor**: `MonsterAI::guard(home: Point)` — starts in `Guarding` mode with
the given home position.

### State Transitions

```
Guarding ──(player visible)──> Hunting
Hunting ──(player lost + reached last known pos)──> Guarding  (if home_position.is_some())
Hunting ──(player lost + reached last known pos)──> Wandering (if no home_position)
```

Guards that lose the player pathfind back to their home position instead of
wandering randomly. This is the only behavioral difference from normal monsters.

### Movement in Guarding Mode

- **Within patrol radius** (3 tiles from home): random walk constrained to tiles within radius
- **Outside patrol radius**: A* pathfind back toward home position

### Squad Integration

`alert_to_position` wakes guards from `Guarding → Hunting` just like it wakes
sleepers from `Asleep → Hunting`. Squad leashing still applies to guard followers.

### Persistence

`home_position` is saved in `MonsterEntry` / `CachedMonster` / `CachedMonsterSave`
with `#[serde(default)]` for backward compatibility.

---

## Prefab System

Prefabs are data-driven map templates that stamp predefined structures onto dungeon
rooms during map generation.

### RON Format

Defined in `assets/prefabs.ron` as a `PrefabManifest` containing a list of
`PrefabTemplate` entries. Each template specifies:

- **Dimensions**: `width`, `height`
- **Floor range**: `min_floor`, `max_floor`
- **Tile layout**: row-major strings (`#`=wall, `.`=floor, `+`=door, ` `=unchanged)
- **Spawns**: `props`, `monster_spawns` (with `guard`/`squad` flags), `item_spawns`
- **Squad config**: `on_leader_death`, `flee_threshold`

### Placement Algorithm — PrefabPlacer

`PrefabPlacer` is a `MetaMapBuilder` in the pipeline after `StartPointBuilder`,
before `CandleSpawner`:

```
BrogueLikeBuilder → DiagonalCuller → StartPointBuilder → PrefabPlacer
  → CandleSpawner → MonsterSpawner → ItemSpawner → UnseenCuller → DistantExit
```

**Steps**:
1. Roll placement chance (~40% per floor, at most 1 prefab per floor)
2. Filter prefabs eligible for the current floor depth
3. Find a room large enough to contain the prefab, centered within the room
4. Snapshot target area tiles
5. Stamp prefab tiles onto the map
6. **Connectivity check**: flood-fill from `starting_position` — verify all walkable
   tiles are still reachable. If broken, restore snapshot and try the next room.
7. Add entries to `spawn_list`, `prop_spawn_list`, `item_spawn_list`
8. Assign a shared `SquadId` to all `squad: true` monster spawns; first squad member
   becomes the leader

### First Prefab: Goblin Camp

A 7x7 walled enclosure with a single entrance, containing:
- A watchfire in the center (light source)
- 2 Goblin guards and 1 Goblin Archer (all squad-linked with guard AI)
- One item spawn point
- Appears on floors 2-8

---

## Files Changed

| File | Change |
|------|--------|
| `src/components.rs` | `Prop` component |
| `src/assets/mod.rs` | `PropAsset`, `PropManifest`, `PrefabTemplate`, `PrefabManifest`, loading |
| `assets/props.ron` | Prop definitions |
| `assets/prefabs.ron` | Goblin camp prefab |
| `src/game/spawner.rs` | `spawn_prop()` |
| `src/game/systems.rs` | `update_prop_visibility` |
| `src/game/ai.rs` | `Guarding` mode, `home_position`, guard behavior |
| `src/map/builders/mod.rs` | `prop_spawn_list`, `home_position` on `SpawnEntry`, pipeline update |
| `src/map/builders/prefab_placer.rs` | `PrefabPlacer` MetaMapBuilder |
| `src/map/dungeon.rs` | Prop/prefab spawning, floor cache, `EntityAssets` |
| `src/save/mod.rs` | `PropEntry`, prop/guard persistence |

---

## Future Layers

- **Layer 2 — Retreat to camp**: surviving squad members pathfind to camp after leader death
- **Layer 3 — Information relay**: retreating goblin alerts camp garrison, camp dispatches pursuit squad
- **Layer 4 — Fortress**: multi-room structures, boss encounters, dynamic squad dispatch
