# Entities in Vision Panel

A persistent HUD sidebar showing all monsters and items currently within the player's field of view, sorted by proximity. Updates every turn.

---

## Layout

A vertical panel anchored to the **right edge** of the screen — full height, fixed width (~180px). It sits beside the game viewport and never occludes the map. Always visible during gameplay; no toggle required.

```
+--------------------+
| NEARBY             |
+--------------------+
| MONSTERS           |
| [sprite] Goblin  3 |
| [sprite] Orc     5 |
| [sprite] Skeleton 8|
+--------------------+
| ITEMS              |
| [sprite] Sword   4 |
| [sprite] Potion  7 |
+--------------------+
```

**Each row contains:**
- A small sprite icon — the entity's actual in-game atlas sprite, scaled to ~16×16 px using the `TextureAtlas` handle/index already on the entity's `Sprite` component (no extra asset loading needed)
- Entity name, truncated if too long
- Tile distance from the player (integer, Euclidean)

**Grouping:** Monsters section first, Items section below. Each section sorted by distance ascending (closest first). Empty sections are hidden entirely — no header shown if no monsters or no items are visible.

**Boss entities** have their name rendered in orange/red to distinguish them at a glance.

---

## Map Highlight

When hovering over or selecting a row, the entity's tile on the map shows a **pulsing colored ring**:

- Monsters → red ring
- Items → gold/yellow ring
- Animation: alpha pulses via `sin(time)` between ~0.4 and 0.9 over roughly 1 second
- Highlight clears immediately when hover ends or selection changes

**Implementation:** A `HoveredEntity(Option<Entity>)` resource tracks the currently highlighted entity. A rendering system reads this resource each frame and draws a colored overlay border on the matching map tile.

---

## Tooltip

Hovering a row (or selecting it via keyboard) shows a tooltip floating to the **left** of the sidebar, z-indexed above all other UI.

### Monster tooltip

```
+------------------------+
| Goblin Warrior         |
| 5 tiles away           |
| HP: [████████░░] 32/40 |
| ATK: 8  DEF: 3  SPD: 1 |
+------------------------+
```

Fields:
- Name
- Distance ("5 tiles away")
- HP bar with current/max values — filled blocks proportional to HP percentage
- ATK, DEF, SPD pulled from `CombatStats`

### Item tooltip

```
+------------------------+
| Iron Sword             |
| 4 tiles away           |
| Uncommon               |
| Damage: 1d8+2          |
| A sturdy blade, well   |
| worn but reliable.     |
+------------------------+
```

Fields:
- Name
- Distance
- Rarity (color-coded: grey=Common, green=Uncommon, blue=Rare, purple=Epic, orange=Legendary)
- Key stat: damage dice (weapon), defense value (armor), effect description (consumable/spellbook)
- Flavor description if present

---

## Keyboard Navigation

The game is keyboard-first; mouse hover is a bonus interaction.

| Key | Action |
|-----|--------|
| `Tab` | Cycle forward through the entity list; wraps around |
| `Shift+Tab` | Cycle backward |
| `Escape` | Clear selection |
| Any movement key | Clear selection |

The selected row is highlighted with a subtle background tint. The pulsing map ring tracks the selection.

---

## Data Source & Update Logic

- Entity list is derived from the **player's `Viewshed.visible_tiles`**
- Each visible tile position is cross-referenced against entities with a matching `Position` component that have either a `Monster` or `Item` marker
- Distance computed as Euclidean from player `Position`, rounded to nearest integer tile
- List rebuilds whenever `Viewshed` is marked changed (i.e., each turn the player acts or FOV updates)
- Entities that die or are picked up mid-turn are removed from the list on the next FOV update

---

## Overflow Handling

If the panel contains more entries than it has vertical space for, it becomes scrollable using the same held-key auto-scroll pattern as the log history screen (W/S or arrow keys, 50ms repeat timer, fires immediately on first press).

---

## Open Questions / Future Ideas

- Could double as a targeting selector — pressing `Enter` on a selected monster could initiate a ranged attack or spell cast targeting that entity
- Could show status effects (poison, slow, etc.) as small icons on the monster row once the status effect system (M7) is implemented
- Item rows could show a "G — pick up" hint when the item is adjacent to the player
