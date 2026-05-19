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
//! ## Scope (RFC 0002 Step 1)
//!
//! This file lands the type vocabulary + pure decision helpers without
//! wiring any behavior. The existing `Machine` system in
//! [`crate::game::machines`] continues to handle live activations.
//! Subsequent steps replace it module-by-module.
//!
//! See [`docs/rfcs/0002-prop-machine-decoration-unification.md`] for
//! the migration plan.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use roguelike_engine::combat::DamageType;
use roguelike_engine::status::StatusEffectKind;

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
// Plugin
// =====================================================================

/// Plugin scaffold for the prop effect system.
///
/// Currently registers nothing — the dispatch systems land in later
/// RFC 0002 steps. Lives here so `src/game/mod.rs` can wire it once
/// and the migration steps add systems incrementally.
pub struct PropEffectsPlugin;

impl Plugin for PropEffectsPlugin {
    fn build(&self, _app: &mut App) {
        // Intentionally empty. Subsequent RFC 0002 steps add:
        //   - PropBumpMessage + bump dispatch system
        //   - Step dispatch system (Changed<Position> watcher)
        //   - Decoration::step_effect dispatch
        //   - Save serialization of EverFired
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
}
