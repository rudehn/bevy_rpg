# Phase 6: The Tyrant — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** The game has a winnable ending. Tyrant on floor 20 with 4 Aspects that grow via hunger clock. Victory and death screens show run summary.

**Architecture:** Boss system already exists (BossAI, TyrantPower, FinalBoss marker, death/victory screens). We're replacing the simple TyrantPower tiers with the Aspect system and changing the final floor from 10 to 20.

**Current State:**
- Final floor is 10 (needs → 20)
- TyrantPower has simple escalation tiers (needs → 3 random Aspects with growth stages)
- Tyrant spawns with 6 spells and necrotic damage
- Victory/death screens exist but show minimal info
- Hunger clock works (TurnManager.current_time, 100 ticks/action)

---

## Tasks

### Task 1: Change final floor to 20
- `src/map/builders/exit_points.rs`: change depth == 10 → depth == 20
- Search for any other hardcoded "10" floor references
- Verify: floors 1-19 have DownStairs, floor 20 does not

### Task 2: Replace TyrantPower with Aspect system
- Modify `src/game/boss.rs`:
  - Define `TyrantAspect` enum: Flame, Iron, Blood, Storm
  - Define `TyrantAspects` resource: 3 randomly selected aspects + stage per aspect
  - Hunger clock thresholds: Stage 1 at 25k, Stage 2 at 60k, Stage 3 at 100k
  - Beyond: +15 HP, +1 armor per 50k after Stage 3
  - Replace `tyrant_escalation_system` with aspect stage advancement
  - Whisper messages on stage transitions (non-specific, don't reveal which aspects)
- Initialize `TyrantAspects` at run start (3 random from 4)
- Persist in save/load (replace TyrantPower in GameSaveData)

### Task 3: Apply Aspect abilities to Tyrant at spawn
- Modify `apply_tyrant_power_on_spawn` → `apply_tyrant_aspects_on_spawn`
- For each aspect at its current stage, apply:

  **Flame:**
  - Stage 1: Add Fire Dart spell
  - Stage 2: + Add Fireball, 50% fire resistance
  - Stage 3: + Fire immune (100%), 40% chance Burning on melee

  **Iron:**
  - Stage 1: +2 armor
  - Stage 2: + +4 armor total, 2 reflect damage (RoughBody)
  - Stage 3: + +6 armor total, 3 reflect, 50% physical resistance

  **Blood:**
  - Stage 1: +15 HP, regen 3/turn
  - Stage 2: + +30 HP, regen 6/turn, +3 damage below 40% HP
  - Stage 3: + +45 HP, regen 8/turn, +6 damage below 60% HP

  **Storm:**
  - Stage 1: Add Spark spell
  - Stage 2: + Add Chain Lightning, 15% stun on melee
  - Stage 3: + 30% stun on melee, knockback 2

### Task 4: Improve victory/death screens
- Add to RunSummary: essence_collected, shrines_purchased, enemies_killed
- Track enemies_killed in a new resource (increment on DeathEvent)
- Victory screen: show floor, essence, shrines, enemies
- Death screen: show floor, cause of death, essence, enemies

### Task 5: Smoke test
- Play through to floor 20 (use cheat menu to skip floors)
- Verify aspects apply to Tyrant
- Verify hunger clock advances stages
- Verify whisper messages appear
- Kill Tyrant → victory screen
- Die → death screen with summary

---

## Verification
1. `cargo test` — all pass
2. Floor 20 spawns Tyrant (no stairs)
3. Aspects randomly selected at run start
4. Hunger clock advances stages with whisper messages
5. Tyrant has aspect-appropriate abilities/resistances
6. Victory screen shows run summary
7. Death screen shows cause + stats
