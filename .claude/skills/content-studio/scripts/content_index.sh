#!/usr/bin/env bash
# content_index.sh — live scan of The Veiled Tyrant's content state.
# Call from anywhere; resolves the project root from the script's path.
# Always reads current RON files; never caches. Use before content workflows.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
cd "$PROJECT_ROOT"

banner() { printf '\n==== %s ====\n' "$1"; }
note()   { printf '  %s\n' "$1"; }

# Extract entry names (lines like `"Name": (` or `"Name": TypeName(`) that
# are NOT commented out. Portable across BSD and GNU sed; uses [[:space:]] not \s.
entry_names() {
  local file="$1"
  [ -f "$file" ] || return
  grep -E '^[[:space:]]*"[A-Za-z][^"]*":[[:space:]]*([A-Za-z_][A-Za-z0-9_]*)?\(' "$file" \
    | grep -vE '^[[:space:]]*//' \
    | sed -E 's/^[[:space:]]*"([^"]+)":.*$/\1/'
}

# Extract the value of a `field: Value,` or `field: "Value",` line, uncommented.
field_values() {
  local file="$1" field="$2"
  [ -f "$file" ] || return
  grep -E "^[[:space:]]*${field}:" "$file" \
    | grep -vE '^[[:space:]]*//' \
    | sed -E "s/^[[:space:]]*${field}:[[:space:]]*\"?([^\",]+)\"?.*$/\1/"
}

# ==================================================================
# MONSTERS
# ==================================================================
banner "MONSTERS (assets/monsters.ron)"
if [ -f assets/monsters.ron ]; then
  monsters="$(entry_names assets/monsters.ron)"
  count="$(printf '%s\n' "$monsters" | grep -c . || true)"
  note "Active monsters: $count"

  printf '\n  By faction:\n'
  field_values assets/monsters.ron "faction" | sort | uniq -c | sort -rn \
    | while read -r n faction; do
        printf '    %3s  %s\n' "$n" "$faction"
      done

  printf '\n  By species:\n'
  field_values assets/monsters.ron "species" | sort | uniq -c | sort -rn \
    | while read -r n sp; do
        printf '    %3s  %s\n' "$n" "$sp"
      done

  printf '\n  Monster names:\n'
  printf '%s\n' "$monsters" | sort \
    | while read -r m; do
        [ -n "$m" ] && printf '    - %s\n' "$m"
      done
else
  note "(missing)"
fi

# ==================================================================
# MONSTER SPAWNS
# ==================================================================
banner "MONSTER SPAWN COVERAGE (assets/monster_spawns.ron)"
if [ -f assets/monster_spawns.ron ]; then
  active_spawns="$(grep -E 'min_floor:[[:space:]]*[0-9]+' assets/monster_spawns.ron \
    | grep -vE '^[[:space:]]*//' || true)"
  spawn_count="$(printf '%s\n' "$active_spawns" | grep -c . || true)"
  note "Active spawn entries: $spawn_count"

  printf '\n  Per-floor spawn entry counts:\n'
  for f in $(seq 1 26); do
    n="$(printf '%s\n' "$active_spawns" \
      | awk -v f="$f" '
          {
            min = 0; max = 0;
            if (match($0, /min_floor:[ \t]*[0-9]+/)) {
              s = substr($0, RSTART, RLENGTH); sub(/[^0-9]*/, "", s); min = s + 0;
            }
            if (match($0, /max_floor:[ \t]*[0-9]+/)) {
              s = substr($0, RSTART, RLENGTH); sub(/[^0-9]*/, "", s); max = s + 0;
            }
            if (min > 0 && max > 0 && f >= min && f <= max) c++;
          }
          END { print (c ? c : 0) }')"
    marker=""
    if   [ "$n" -eq 0 ]; then marker="  <-- GAP"
    elif [ "$n" -le 2 ]; then marker="  (thin)"
    fi
    printf '    floor %2d: %2s%s\n' "$f" "$n" "$marker"
  done
fi

# ==================================================================
# ITEMS
# ==================================================================
banner "ITEMS (assets/items.ron)"
if [ -f assets/items.ron ]; then
  items="$(entry_names assets/items.ron)"
  count="$(printf '%s\n' "$items" | grep -c . || true)"
  note "Active items: $count"

  printf '\n  By item_kind:\n'
  field_values assets/items.ron "item_kind" | sort | uniq -c | sort -rn \
    | while read -r n kind; do
        printf '    %3s  %s\n' "$n" "$kind"
      done

  printf '\n  By rarity:\n'
  field_values assets/items.ron "rarity" | sort | uniq -c | sort -rn \
    | while read -r n r; do
        printf '    %3s  %s\n' "$n" "$r"
      done
fi

# ==================================================================
# ITEM SPAWNS
# ==================================================================
banner "ITEM SPAWN COVERAGE (assets/item_spawns.ron)"
if [ -f assets/item_spawns.ron ]; then
  active_item_spawns="$(grep -E 'min_floor:[[:space:]]*[0-9]+' assets/item_spawns.ron \
    | grep -vE '^[[:space:]]*//' || true)"
  isp_count="$(printf '%s\n' "$active_item_spawns" | grep -c . || true)"
  note "Active item spawn entries: $isp_count"
fi

# ==================================================================
# FACTIONS
# ==================================================================
banner "FACTIONS (assets/factions.ron)"
if [ -f assets/factions.ron ]; then
  hostile="$(grep -E 'relation:[[:space:]]*Hostile' assets/factions.ron \
    | grep -vE '^[[:space:]]*//' \
    | sed -E 's/.*a:[[:space:]]*"([^"]+)".*b:[[:space:]]*"([^"]+)".*relation:[[:space:]]*Hostile.*/\1 <-> \2/' || true)"
  note "Hostile pairs:"
  if [ -n "$hostile" ]; then
    printf '%s\n' "$hostile" | while read -r p; do note "  $p"; done
  else
    note "  (none)"
  fi

  active_factions="$(field_values assets/monsters.ron "faction" | sort -u | paste -sd, -)"
  printf '\n  Factions with active monsters: %s\n' "${active_factions:-none}"
fi

# ==================================================================
# DECORATIONS / PROPS / TILES
# ==================================================================
banner "TERRAIN CATEGORIES"
for f in assets/decorations.ron assets/props.ron assets/tiles.ron; do
  if [ -f "$f" ]; then
    n="$(entry_names "$f" | grep -c . || true)"
    note "$(basename "$f"): $n entries"
  fi
done

# ==================================================================
# DESIGN DOCS
# ==================================================================
banner "DESIGN DOCS (docs/design/)"
if [ -d docs/design ]; then
  for f in docs/design/*.md; do
    [ -f "$f" ] || continue
    lines="$(wc -l < "$f" | tr -d ' ')"
    note "$(basename "$f") — ${lines} lines"
  done
fi

# ==================================================================
# CONTENT TODOs
# ==================================================================
banner "CONTENT TODOs (docs/TODO.md, content-scoped)"
if [ -f docs/TODO.md ]; then
  grep -niE 'trap|stealth|cursed|axe|spear|mace|undead|orc|fungal|dragon|giant|boss|shrine' docs/TODO.md \
    | head -20 \
    | while read -r line; do note "$line"; done
fi

echo ""
