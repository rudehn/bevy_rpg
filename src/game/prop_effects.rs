//! Shared effect vocabulary for interactive props and decoration step-effects.
//!
//! Pure data types describing "when an actor steps onto / bumps into
//! this prop, do X." The data here is asset-shaped — `PropTrigger`
//! is what `props.ron` will declare on a prop, `Effected` is the
//! component the spawner attaches at runtime, `EverFired` is the
//! per-instance activation flag.
//!
//! Distinct from [`crate::game::effects`], which owns *consumable item*
//! effects (HealHp, ZapStaff, etc.) — that vocabulary is player-driven
//! ("I used a scroll"), this vocabulary is world-driven ("the world
//! stepped on me").
//!
//! ## History
//!
//! Replaces the legacy Machine system (deleted in RFC 0002 step 5).
//! See [`docs/rfcs/0002-prop-machine-decoration-unification.md`] for
//! the migration history.

use bevy::prelude::*;
use bracket_lib::prelude::Point;
use bracket_lib::random::RandomNumberGenerator;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use roguelike_engine::combat::events::DamageEvent;
use roguelike_engine::combat::{DamageSource, DamageType};
use roguelike_engine::dice::roll_dice_string;
use roguelike_engine::status::StatusEffectKind;

use crate::components::{Collider, Name, Position, Prop};
use crate::map::tile::Decoration;
use crate::constants::BASE_ACTION_COST;
use crate::game::actions::{finish_turn, ActionFinishedEvent, ActionKind};
use crate::game::combat::Health;
use crate::game::magic::{GameStatusEffectsExt, StatusEffects};
use crate::game::turns::{ProcessingPhase, TurnManager, TurnState};
use crate::game::AppState;
use crate::map::map::Map;
use crate::player::Player;
use crate::ui::game_log::GameLogMessage;

// =====================================================================
// TileEffect — what happens when an effect fires
// =====================================================================

/// The vocabulary of effects a prop trigger or decoration step can fire.
///
/// Pure data. Application lives in the (currently empty) `PropEffectsPlugin`
/// systems and the existing Machine adapter for now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TileEffect {
    /// Roll dice damage of the given type against the activator.
    DealDamage { dice: String, kind: DamageType },
    /// Apply a status effect for N turns.
    ApplyStatus {
        effect: StatusEffectKind,
        duration: u32,
    },
    /// Heal the activator to full HP.
    HealFull,
    /// Spawn an item at an adjacent walkable tile.
    SpawnItem { item_name: String },
    /// Spawn N monsters at adjacent walkable tiles. Empty `monster_name`
    /// picks level-appropriate entries from the spawn table.
    SpawnMonsters { monster_name: String, count: u32 },
    /// Apply multiple effects in order.
    Multi(Vec<TileEffect>),
}

impl TileEffect {
    /// Flatten nested `Multi(Multi(...))` chains into a single ordered
    /// list of leaf effects. Useful for adapters that want to iterate
    /// without recursion.
    pub fn flatten(&self) -> Vec<&TileEffect> {
        let mut out = Vec::new();
        self.flatten_into(&mut out);
        out
    }

    fn flatten_into<'a>(&'a self, out: &mut Vec<&'a TileEffect>) {
        match self {
            TileEffect::Multi(children) => {
                for child in children {
                    child.flatten_into(out);
                }
            }
            leaf => out.push(leaf),
        }
    }
}

// =====================================================================
// EffectAudience — who can trigger
// =====================================================================

/// Who is allowed to trip a prop's trigger. Default is `Anyone`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EffectAudience {
    /// Any actor (player + monsters). The default.
    #[default]
    Anyone,
    /// Only the player triggers the effect.
    PlayerOnly,
    /// Only monsters trigger the effect (e.g., player-laid traps).
    MonstersOnly,
}

impl EffectAudience {
    /// Whether this audience permits the given activator.
    pub fn applies_to(self, activator: ActivatorKind) -> bool {
        match (self, activator) {
            (EffectAudience::Anyone, _) => true,
            (EffectAudience::PlayerOnly, ActivatorKind::Player) => true,
            (EffectAudience::MonstersOnly, ActivatorKind::Monster) => true,
            _ => false,
        }
    }
}

/// Coarse classification of an activator entity, used by audience
/// filtering. The adapter (a future Bevy system) computes this from
/// ECS components.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ActivatorKind {
    Player,
    Monster,
}

// =====================================================================
// ActivationMode — how the trigger persists
// =====================================================================

/// How a prop's trigger persists after first activation.
///
/// Collapses the prior two-bool `single_use` × `consume_on_activate`
/// space into three explicit states. See RFC 0002 for the rationale.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ActivationMode {
    /// Fires every time the prop is activated. Default — campfire pattern.
    #[default]
    Repeating,
    /// Fires once; the prop remains visible/blocking but inert afterward.
    /// Used-altar pattern.
    OnceInert,
    /// Fires once; the prop entity despawns afterward. Sprung-trap pattern.
    OnceConsumed,
}

impl ActivationMode {
    /// Whether a prop with this mode should fire given its prior
    /// activation state.
    pub fn should_fire(self, ever_fired: bool) -> bool {
        match self {
            ActivationMode::Repeating => true,
            ActivationMode::OnceInert | ActivationMode::OnceConsumed => !ever_fired,
        }
    }

    /// Whether the prop entity should despawn after this firing.
    pub fn should_despawn_after_firing(self) -> bool {
        matches!(self, ActivationMode::OnceConsumed)
    }
}

// =====================================================================
// PropTrigger — the bundle a PropAsset will declare
// =====================================================================

/// Optional trigger configuration declared on a `PropAsset`.
///
/// Step direction (step vs bump) is **not** stored here — it is
/// derived from the prop's `is_blocking` flag at spawn time. Blocking
/// props can only be bumped; non-blocking props can only be stepped
/// onto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropTrigger {
    /// What happens when this prop is activated.
    pub effect: TileEffect,
    /// Who can trigger the effect.
    #[serde(default)]
    pub audience: EffectAudience,
    /// Activation lifecycle.
    #[serde(default)]
    pub mode: ActivationMode,
}

// =====================================================================
// ECS Components
// =====================================================================

/// Marker + payload component attached to spawned interactive props.
///
/// Carries the static trigger configuration copied from the prop's
/// `PropAsset` at spawn. Mutated state (whether the prop has fired)
/// lives on [`EverFired`].
#[derive(Component, Debug, Clone)]
pub struct Effected(pub PropTrigger);

/// Per-instance activation state for an `Effected` prop.
///
/// Starts `false` at spawn; flipped to `true` on first firing. The
/// dispatch system reads this against `Effected.0.mode` to decide
/// whether to fire again and whether to despawn.
///
/// **Save:** persisted from RFC 0002 Step 4 onward (save schema v10).
#[derive(Component, Debug, Default, Copy, Clone, Serialize, Deserialize)]
pub struct EverFired(pub bool);

// =====================================================================
// Messages
// =====================================================================

/// Sent when an actor bumps into a blocking prop that carries an
/// `Effected` trigger. The bump-handler in `handle_movement`
/// (actions.rs) emits this for prop-driven activations.
#[derive(Message, Debug)]
pub struct PropBumpMessage {
    pub activator: Entity,
    pub prop_entity: Entity,
}

// =====================================================================
// Deferred-spawn resources
// =====================================================================
//
// `SpawnItem` and `SpawnMonsters` effects need heavy resources
// (manifests, sprite assets, turn manager) that can't be borrowed
// inside the bump/step systems alongside `Health`, `StatusEffects`,
// etc. The Machine system solved this with deferred singleton
// resources processed on the next frame. We mirror that pattern.

#[derive(Resource)]
pub struct PendingPropSpawnItem {
    pub item_name: String,
    pub pos: Position,
}

#[derive(Resource)]
pub struct PendingPropSpawnMonsters {
    pub monster_name: String,
    pub count: u32,
    pub pos: Position,
}

// =====================================================================
// Activation classification (pure)
// =====================================================================

/// Outcome of evaluating a prop trigger against an activator.
/// Pure value — bump/step systems map it to side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationOutcome {
    /// Audience filter rejected this activator. Do nothing.
    AudienceRejected,
    /// Mode is single-use and this prop has already fired. Do nothing.
    AlreadyFired,
    /// Fire the effect. `despawn_after` is true for `OnceConsumed` mode.
    Fire { despawn_after: bool },
}

/// Decide what should happen when `kind` activates a prop with this
/// `trigger` and current `ever_fired` state. Pure — testable without
/// Bevy.
pub fn classify_activation(
    trigger: &PropTrigger,
    ever_fired: bool,
    kind: ActivatorKind,
) -> ActivationOutcome {
    if !trigger.audience.applies_to(kind) {
        return ActivationOutcome::AudienceRejected;
    }
    if !trigger.mode.should_fire(ever_fired) {
        return ActivationOutcome::AlreadyFired;
    }
    ActivationOutcome::Fire {
        despawn_after: trigger.mode.should_despawn_after_firing(),
    }
}

/// Resolve an entity to its `ActivatorKind` (Player vs Monster) for
/// audience filtering. Any entity not flagged as the player falls
/// back to `Monster` — the prop dispatch is symmetric by default and
/// only diverges when audience is `PlayerOnly` or `MonstersOnly`.
fn classify_activator(activator: Entity, player_query: &Query<(), With<Player>>) -> ActivatorKind {
    if player_query.get(activator).is_ok() {
        ActivatorKind::Player
    } else {
        ActivatorKind::Monster
    }
}

// =====================================================================
// Effect application
// =====================================================================

/// Apply the effects that only need direct component access (Health,
/// StatusEffects). Damage effects emit a `DamageEvent` via the writer
/// so the engine's `damage_application_system` handles armor +
/// resistance uniformly with the rest of combat.
fn apply_inline(
    effect: &TileEffect,
    activator: Entity,
    health_query: &mut Query<&mut Health>,
    status_query: &mut Query<&mut StatusEffects>,
    damage_writer: &mut MessageWriter<DamageEvent>,
    log_writer: &mut MessageWriter<GameLogMessage>,
) {
    match effect {
        TileEffect::HealFull => {
            if let Ok(mut health) = health_query.get_mut(activator) {
                let healed = health.max - health.current;
                health.current = health.max;
                if healed > 0 {
                    log_writer.write(GameLogMessage(format!(
                        "You are healed for {} HP!",
                        healed
                    )));
                }
            }
        }
        TileEffect::DealDamage { dice, kind } => {
            let mut rng = RandomNumberGenerator::new();
            let raw = roll_dice_string(&mut rng, dice);
            if raw > 0 {
                damage_writer.write(DamageEvent {
                    target: activator,
                    amount: raw,
                    damage_type: *kind,
                    source: DamageSource::Environment,
                    attacker: None,
                    armor: 0,
                });
            }
        }
        TileEffect::ApplyStatus { effect, duration } => {
            if let Ok(mut effects) = status_query.get_mut(activator) {
                effects.add_effect(*effect, *duration);
            }
        }
        TileEffect::Multi(children) => {
            for child in children {
                apply_inline(child, activator, health_query, status_query, damage_writer, log_writer);
            }
        }
        // Spawn effects are deferred — handled by queue_deferred.
        TileEffect::SpawnItem { .. } | TileEffect::SpawnMonsters { .. } => {}
    }
}

/// Stash any spawn-style effects into deferred resources to be picked
/// up next frame by their dedicated systems. Mirrors `machines.rs`.
fn queue_deferred(effect: &TileEffect, pos: &Position, commands: &mut Commands) {
    match effect {
        TileEffect::SpawnItem { item_name } => {
            commands.insert_resource(PendingPropSpawnItem {
                item_name: item_name.clone(),
                pos: *pos,
            });
        }
        TileEffect::SpawnMonsters {
            monster_name,
            count,
        } => {
            commands.insert_resource(PendingPropSpawnMonsters {
                monster_name: monster_name.clone(),
                count: *count,
                pos: *pos,
            });
        }
        TileEffect::Multi(children) => {
            for child in children {
                queue_deferred(child, pos, commands);
            }
        }
        _ => {}
    }
}

// =====================================================================
// Bump dispatch
// =====================================================================

/// Apply prop bump activations. Mirrors `handle_machine_bump`.
///
/// Reads `PropBumpMessage`, gates on audience + activation mode, fires
/// effects (inline + deferred), finalizes activation state, and ends
/// the activator's turn.
pub fn handle_prop_bump(
    mut commands: Commands,
    mut messages: MessageReader<PropBumpMessage>,
    mut props_query: Query<(&Effected, &mut EverFired, &Position, &Name), With<Prop>>,
    mut health_query: Query<&mut Health>,
    mut status_query: Query<&mut StatusEffects>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    player_query: Query<(), With<Player>>,
) {
    for msg in messages.read() {
        let Ok((effected, mut ever_fired, pos, name)) = props_query.get_mut(msg.prop_entity) else {
            continue;
        };
        let trigger = &effected.0;
        let kind = classify_activator(msg.activator, &player_query);

        match classify_activation(trigger, ever_fired.0, kind) {
            ActivationOutcome::AudienceRejected => {
                // Silent no-op; bump still consumes a turn.
            }
            ActivationOutcome::AlreadyFired => {
                log_writer.write(GameLogMessage(format!("The {} is inert.", name.0)));
            }
            ActivationOutcome::Fire { despawn_after } => {
                log_writer.write(GameLogMessage(format!("You activate the {}.", name.0)));
                apply_inline(
                    &trigger.effect,
                    msg.activator,
                    &mut health_query,
                    &mut status_query,
                    &mut damage_writer,
                    &mut log_writer,
                );
                queue_deferred(&trigger.effect, pos, &mut commands);
                ever_fired.0 = true;
                if despawn_after {
                    commands.entity(msg.prop_entity).despawn();
                }
            }
        }

        finish_turn(
            &mut commands,
            &mut finish_writer,
            msg.activator,
            BASE_ACTION_COST,
            ActionKind::Movement,
        );
    }
}

// =====================================================================
// Decoration step lookup
// =====================================================================

/// Map a `Decoration` variant to the effect that fires when an actor
/// steps onto a tile carrying it. Pure data — testable without Bevy.
///
/// Most decorations are passive flavor (grass, moss, embers, rubble)
/// and return `None`. The only opted-in variant today is `Cobweb`
/// (Slowed 3 turns). Embers stays silent on purpose — see RFC 0002
/// for the "post-fire trace shouldn't punish you" rationale.
pub fn decoration_step_effect(decoration: Decoration) -> Option<TileEffect> {
    match decoration {
        Decoration::Cobweb => Some(TileEffect::ApplyStatus {
            effect: StatusEffectKind::Slowed,
            duration: 3,
        }),
        _ => None,
    }
}

/// Fire a decoration step effect when any actor moves onto a decorated
/// tile. Mirrors `prop_step_system` but for tile-packed `Decoration`
/// data (no entity colocation).
pub fn decoration_step_system(
    moved_query: Query<(Entity, &Position), Changed<Position>>,
    mut health_query: Query<&mut Health>,
    mut status_query: Query<&mut StatusEffects>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    map: Res<Map>,
    turn_state: Res<bevy::state::state::State<TurnState>>,
) {
    if *turn_state.get() != TurnState::Processing {
        return;
    }

    for (mover, pos) in moved_query.iter() {
        let idx = map.xy_idx(pos.x, pos.y);
        if idx >= map.tiles.len() {
            continue;
        }
        let Some(effect) = decoration_step_effect(map.tiles[idx].decoration) else {
            continue;
        };

        // Audience filtering doesn't apply to decoration effects —
        // they're physics, not authored content. Anyone stepping on
        // cobwebs gets slowed.
        //
        // Spawn-style effects are intentionally unsupported for
        // decoration triggers — they need per-tile fire-once state
        // that the packed Decoration enum doesn't carry. Use a prop
        // with OnceConsumed if you need that.
        apply_inline(
            &effect,
            mover,
            &mut health_query,
            &mut status_query,
            &mut damage_writer,
            &mut log_writer,
        );
    }
}

// =====================================================================
// Step dispatch
// =====================================================================

/// Detect when any actor moves onto a non-blocking Effected prop tile.
///
/// Unlike the legacy `machine_step_system` (player-only), prop-step
/// fires for any actor whose audience the trigger permits — see
/// [`EffectAudience`]. Step effects do **not** cost an additional turn;
/// they ride alongside the movement that triggered them, same as lava.
pub fn prop_step_system(
    mut commands: Commands,
    moved_query: Query<(Entity, &Position), Changed<Position>>,
    mut props_query: Query<
        (Entity, &Effected, &mut EverFired, &Position, &Name),
        (With<Prop>, Without<Collider>),
    >,
    mut health_query: Query<&mut Health>,
    mut status_query: Query<&mut StatusEffects>,
    mut damage_writer: MessageWriter<DamageEvent>,
    mut log_writer: MessageWriter<GameLogMessage>,
    player_query: Query<(), With<Player>>,
    turn_state: Res<bevy::state::state::State<TurnState>>,
) {
    if *turn_state.get() != TurnState::Processing {
        return;
    }

    for (mover, mover_pos) in moved_query.iter() {
        for (prop_entity, effected, mut ever_fired, prop_pos, name) in props_query.iter_mut() {
            if mover_pos.x != prop_pos.x || mover_pos.y != prop_pos.y {
                continue;
            }
            let trigger = &effected.0;
            let kind = classify_activator(mover, &player_query);

            if let ActivationOutcome::Fire { despawn_after } =
                classify_activation(trigger, ever_fired.0, kind)
            {
                log_writer.write(GameLogMessage(format!("You step onto the {}.", name.0)));
                apply_inline(
                    &trigger.effect,
                    mover,
                    &mut health_query,
                    &mut status_query,
                    &mut damage_writer,
                    &mut log_writer,
                );
                queue_deferred(&trigger.effect, prop_pos, &mut commands);
                ever_fired.0 = true;
                if despawn_after {
                    commands.entity(prop_entity).despawn();
                }
                break; // One activation per move tick
            }
        }
    }
}

// =====================================================================
// Deferred-spawn processing
// =====================================================================

/// Spawn the queued item at an adjacent walkable tile.
pub fn process_pending_prop_item(
    mut commands: Commands,
    pending: Option<Res<PendingPropSpawnItem>>,
    item_manifests: Res<bevy::asset::Assets<crate::assets::ItemManifest>>,
    item_manifest_handle: Res<crate::assets::ItemManifestHandle>,
    item_sprite_assets: Res<crate::assets::ItemSpriteAssets>,
    ascii_font: Option<Res<crate::game::ascii_mode::AsciiFont>>,
    map: Res<Map>,
    collider_query: Query<&Position, With<Collider>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    let Some(pending) = pending else {
        return;
    };
    use crate::map::tile::is_walkable;

    let occupied: HashSet<(i32, i32)> = collider_query.iter().map(|p| (p.x, p.y)).collect();
    let directions = [
        (0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1),
    ];
    let mut spawn_point = Point::new(pending.pos.x, pending.pos.y);
    for (dx, dy) in &directions {
        let nx = pending.pos.x + dx;
        let ny = pending.pos.y + dy;
        let idx = map.xy_idx(nx, ny);
        if idx < map.tiles.len() && is_walkable(map.tiles[idx]) && !occupied.contains(&(nx, ny)) {
            spawn_point = Point::new(nx, ny);
            break;
        }
    }

    crate::game::spawner::spawn_item(
        &mut commands,
        &pending.item_name,
        &spawn_point,
        &item_manifests,
        &item_manifest_handle,
        &item_sprite_assets,
        ascii_font.as_deref(),
        None,
    );
    log_writer.write(GameLogMessage(format!("A {} appears!", pending.item_name)));
    commands.remove_resource::<PendingPropSpawnItem>();
}

/// Spawn the queued monster(s) at adjacent walkable tiles.
/// Empty monster_name picks from the level's spawn table.
pub fn process_pending_prop_monsters(
    mut commands: Commands,
    pending: Option<Res<PendingPropSpawnMonsters>>,
    mut turn_manager: ResMut<TurnManager>,
    monster_manifests: Res<bevy::asset::Assets<crate::assets::MonsterManifest>>,
    monster_manifest_handle: Res<crate::assets::MonsterManifestHandle>,
    monster_sprite_assets: Res<crate::assets::MonsterSpriteAssets>,
    monster_spawn_table_handle: Res<crate::assets::MonsterSpawnTableHandle>,
    monster_spawn_tables: Res<bevy::asset::Assets<crate::assets::MonsterSpawnTable>>,
    floor: Res<crate::map::dungeon::Floor>,
    ascii_font: Option<Res<crate::game::ascii_mode::AsciiFont>>,
    map: Res<Map>,
    collider_query: Query<&Position, With<Collider>>,
    mut log_writer: MessageWriter<GameLogMessage>,
) {
    let Some(pending) = pending else {
        return;
    };

    let occupied: HashSet<(i32, i32)> = collider_query.iter().map(|p| (p.x, p.y)).collect();
    let mut rng = RandomNumberGenerator::new();
    let depth = floor.0 as i32;

    let directions = [
        (0, -1), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, -1), (-1, 1), (1, 1),
    ];
    let mut spawned = 0u32;
    for (dx, dy) in &directions {
        if spawned >= pending.count {
            break;
        }
        let nx = pending.pos.x + dx;
        let ny = pending.pos.y + dy;
        let idx = map.xy_idx(nx, ny);
        if idx >= map.tiles.len()
            || !crate::map::tile::is_walkable(map.tiles[idx])
            || occupied.contains(&(nx, ny))
        {
            continue;
        }

        let monster_name = if pending.monster_name.is_empty() {
            pick_level_monster(&monster_spawn_tables, &monster_spawn_table_handle, depth, &mut rng)
        } else {
            Some(pending.monster_name.clone())
        };

        if let Some(name) = monster_name {
            crate::game::spawner::spawn_monster_by_name(
                &mut commands,
                &name,
                &Point::new(nx, ny),
                &mut turn_manager,
                &monster_manifests,
                &monster_manifest_handle,
                &monster_sprite_assets,
                ascii_font.as_deref(),
            );
            spawned += 1;
        }
    }
    if spawned > 0 {
        log_writer.write(GameLogMessage("Monsters emerge from the shadows!".to_string()));
    }
    commands.remove_resource::<PendingPropSpawnMonsters>();
}

fn pick_level_monster(
    spawn_tables: &Res<bevy::asset::Assets<crate::assets::MonsterSpawnTable>>,
    handle: &Res<crate::assets::MonsterSpawnTableHandle>,
    depth: i32,
    rng: &mut RandomNumberGenerator,
) -> Option<String> {
    let table = spawn_tables.get(&handle.0)?;
    let eligible: Vec<&crate::assets::MonsterSpawnInfo> = table
        .spawns
        .iter()
        .filter(|s| depth >= s.min_floor && depth <= s.max_floor && !s.monster.is_empty())
        .collect();
    if eligible.is_empty() {
        return None;
    }
    let idx = rng.range(0, eligible.len() as i32) as usize;
    Some(eligible[idx].monster.clone())
}

// =====================================================================
// Plugin
// =====================================================================

/// Wires bump + step dispatchers and deferred-spawn processors.
pub struct PropEffectsPlugin;

impl Plugin for PropEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PropBumpMessage>()
            .add_systems(
                Update,
                handle_prop_bump.in_set(ProcessingPhase::ResolveActions),
            )
            .add_systems(
                Update,
                (
                    prop_step_system,
                    decoration_step_system,
                    process_pending_prop_item,
                    process_pending_prop_monsters,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- TileEffect::flatten ----

    #[test]
    fn flatten_leaf_returns_self() {
        let e = TileEffect::HealFull;
        let flat = e.flatten();
        assert_eq!(flat.len(), 1);
        assert!(matches!(flat[0], TileEffect::HealFull));
    }

    #[test]
    fn flatten_multi_unpacks_in_order() {
        let e = TileEffect::Multi(vec![
            TileEffect::HealFull,
            TileEffect::SpawnItem {
                item_name: "Scroll of Enchanting".into(),
            },
        ]);
        let flat = e.flatten();
        assert_eq!(flat.len(), 2);
        assert!(matches!(flat[0], TileEffect::HealFull));
        assert!(matches!(flat[1], TileEffect::SpawnItem { .. }));
    }

    #[test]
    fn flatten_nested_multi_collapses_fully() {
        let e = TileEffect::Multi(vec![
            TileEffect::Multi(vec![
                TileEffect::HealFull,
                TileEffect::DealDamage {
                    dice: "1d4".into(),
                    kind: DamageType::Fire,
                },
            ]),
            TileEffect::ApplyStatus {
                effect: StatusEffectKind::Slowed,
                duration: 3,
            },
        ]);
        let flat = e.flatten();
        assert_eq!(flat.len(), 3);
        assert!(matches!(flat[0], TileEffect::HealFull));
        assert!(matches!(flat[1], TileEffect::DealDamage { .. }));
        assert!(matches!(flat[2], TileEffect::ApplyStatus { .. }));
    }

    // ---- EffectAudience ----

    #[test]
    fn audience_anyone_permits_both_kinds() {
        assert!(EffectAudience::Anyone.applies_to(ActivatorKind::Player));
        assert!(EffectAudience::Anyone.applies_to(ActivatorKind::Monster));
    }

    #[test]
    fn audience_player_only_rejects_monsters() {
        assert!(EffectAudience::PlayerOnly.applies_to(ActivatorKind::Player));
        assert!(!EffectAudience::PlayerOnly.applies_to(ActivatorKind::Monster));
    }

    #[test]
    fn audience_monsters_only_rejects_player() {
        assert!(!EffectAudience::MonstersOnly.applies_to(ActivatorKind::Player));
        assert!(EffectAudience::MonstersOnly.applies_to(ActivatorKind::Monster));
    }

    #[test]
    fn audience_default_is_anyone() {
        assert_eq!(EffectAudience::default(), EffectAudience::Anyone);
    }

    // ---- ActivationMode ----

    #[test]
    fn repeating_fires_regardless_of_history() {
        assert!(ActivationMode::Repeating.should_fire(false));
        assert!(ActivationMode::Repeating.should_fire(true));
    }

    #[test]
    fn once_inert_fires_only_first_time() {
        assert!(ActivationMode::OnceInert.should_fire(false));
        assert!(!ActivationMode::OnceInert.should_fire(true));
    }

    #[test]
    fn once_consumed_fires_only_first_time() {
        assert!(ActivationMode::OnceConsumed.should_fire(false));
        assert!(!ActivationMode::OnceConsumed.should_fire(true));
    }

    #[test]
    fn only_once_consumed_triggers_despawn() {
        assert!(!ActivationMode::Repeating.should_despawn_after_firing());
        assert!(!ActivationMode::OnceInert.should_despawn_after_firing());
        assert!(ActivationMode::OnceConsumed.should_despawn_after_firing());
    }

    #[test]
    fn activation_mode_default_is_repeating() {
        assert_eq!(ActivationMode::default(), ActivationMode::Repeating);
    }

    // ---- PropTrigger serde round-trip ----

    #[test]
    fn prop_trigger_round_trips_through_ron() {
        let original = PropTrigger {
            effect: TileEffect::Multi(vec![
                TileEffect::HealFull,
                TileEffect::SpawnItem {
                    item_name: "Scroll of Enchanting".into(),
                },
            ]),
            audience: EffectAudience::PlayerOnly,
            mode: ActivationMode::OnceInert,
        };

        let s = ron::ser::to_string(&original).expect("serialize");
        let parsed: PropTrigger = ron::de::from_str(&s).expect("deserialize");

        assert!(matches!(parsed.effect, TileEffect::Multi(_)));
        assert_eq!(parsed.audience, EffectAudience::PlayerOnly);
        assert_eq!(parsed.mode, ActivationMode::OnceInert);
    }

    #[test]
    fn prop_trigger_uses_defaults_when_omitted() {
        // Effect required; audience + mode default.
        let s = "(effect: HealFull)";
        let parsed: PropTrigger = ron::de::from_str(s).expect("deserialize");
        assert!(matches!(parsed.effect, TileEffect::HealFull));
        assert_eq!(parsed.audience, EffectAudience::Anyone);
        assert_eq!(parsed.mode, ActivationMode::Repeating);
    }

    // ---- EverFired default ----

    #[test]
    fn ever_fired_default_is_false() {
        assert!(!EverFired::default().0);
    }

    // ---- classify_activation ----

    fn trigger(audience: EffectAudience, mode: ActivationMode) -> PropTrigger {
        PropTrigger {
            effect: TileEffect::HealFull,
            audience,
            mode,
        }
    }

    #[test]
    fn classify_player_only_rejects_monster() {
        let t = trigger(EffectAudience::PlayerOnly, ActivationMode::Repeating);
        assert_eq!(
            classify_activation(&t, false, ActivatorKind::Monster),
            ActivationOutcome::AudienceRejected
        );
    }

    #[test]
    fn classify_anyone_player_repeating_fires_without_despawn() {
        let t = trigger(EffectAudience::Anyone, ActivationMode::Repeating);
        assert_eq!(
            classify_activation(&t, false, ActivatorKind::Player),
            ActivationOutcome::Fire { despawn_after: false }
        );
    }

    #[test]
    fn classify_repeating_fires_even_when_already_fired() {
        let t = trigger(EffectAudience::Anyone, ActivationMode::Repeating);
        assert_eq!(
            classify_activation(&t, true, ActivatorKind::Player),
            ActivationOutcome::Fire { despawn_after: false }
        );
    }

    #[test]
    fn classify_once_inert_first_time_fires_no_despawn() {
        let t = trigger(EffectAudience::Anyone, ActivationMode::OnceInert);
        assert_eq!(
            classify_activation(&t, false, ActivatorKind::Player),
            ActivationOutcome::Fire { despawn_after: false }
        );
    }

    #[test]
    fn classify_once_inert_second_time_blocked() {
        let t = trigger(EffectAudience::Anyone, ActivationMode::OnceInert);
        assert_eq!(
            classify_activation(&t, true, ActivatorKind::Player),
            ActivationOutcome::AlreadyFired
        );
    }

    #[test]
    fn classify_once_consumed_first_time_fires_with_despawn() {
        let t = trigger(EffectAudience::Anyone, ActivationMode::OnceConsumed);
        assert_eq!(
            classify_activation(&t, false, ActivatorKind::Player),
            ActivationOutcome::Fire { despawn_after: true }
        );
    }

    #[test]
    fn classify_once_consumed_second_time_blocked_no_despawn() {
        let t = trigger(EffectAudience::Anyone, ActivationMode::OnceConsumed);
        assert_eq!(
            classify_activation(&t, true, ActivatorKind::Player),
            ActivationOutcome::AlreadyFired
        );
    }

    #[test]
    fn classify_audience_filter_runs_before_mode_check() {
        // PlayerOnly + OnceConsumed: a monster bumping it should be
        // rejected by audience, NOT consumed.
        let t = trigger(EffectAudience::PlayerOnly, ActivationMode::OnceConsumed);
        assert_eq!(
            classify_activation(&t, false, ActivatorKind::Monster),
            ActivationOutcome::AudienceRejected
        );
    }

    #[test]
    fn classify_monsters_only_lets_monster_through() {
        let t = trigger(EffectAudience::MonstersOnly, ActivationMode::Repeating);
        assert_eq!(
            classify_activation(&t, false, ActivatorKind::Monster),
            ActivationOutcome::Fire { despawn_after: false }
        );
        assert_eq!(
            classify_activation(&t, false, ActivatorKind::Player),
            ActivationOutcome::AudienceRejected
        );
    }

    // ---- decoration_step_effect ----

    #[test]
    fn cobweb_decoration_applies_slowed_three_turns() {
        let effect = decoration_step_effect(Decoration::Cobweb)
            .expect("Cobweb should have a step effect");
        match effect {
            TileEffect::ApplyStatus { effect, duration } => {
                assert_eq!(effect, StatusEffectKind::Slowed);
                assert_eq!(duration, 3);
            }
            other => panic!("expected ApplyStatus(Slowed, 3), got {:?}", other),
        }
    }

    #[test]
    fn embers_decoration_is_silent_on_step() {
        // RFC 0002 §"Decorations that intentionally stay silent" —
        // walking through post-fire embers should not punish the
        // player who just won a fight.
        assert!(decoration_step_effect(Decoration::Embers).is_none());
    }

    #[test]
    fn passive_decorations_have_no_step_effect() {
        // Spot-check the broad flavor set — none of these should ever
        // fire an effect.
        assert!(decoration_step_effect(Decoration::None).is_none());
        assert!(decoration_step_effect(Decoration::Grass).is_none());
        assert!(decoration_step_effect(Decoration::TallGrass).is_none());
        assert!(decoration_step_effect(Decoration::Moss).is_none());
        assert!(decoration_step_effect(Decoration::Rubble).is_none());
        assert!(decoration_step_effect(Decoration::Bloodstain).is_none());
        assert!(decoration_step_effect(Decoration::Ash).is_none());
        assert!(decoration_step_effect(Decoration::Fungus).is_none());
        assert!(decoration_step_effect(Decoration::CrackedFloor).is_none());
    }
}
