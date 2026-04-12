//! Goal-Oriented Action Planning (GOAP) framework.
//!
//! A generic forward-chaining planner for turn-based AI. Given a goal
//! (a desired partial world state) and a set of actions (each with
//! preconditions and effects), [`plan`] returns the first action of the
//! cheapest plan that satisfies the goal.
//!
//! # Design overview
//!
//! - [`WorldState`] is a bundle of boolean facts about an entity and its
//!   environment. The engine ships a blessed set of fields commonly
//!   useful across roguelikes (player visible, HP low, hostile nearby,
//!   adjacent to threat, squad retreating, ...) plus a `custom` map
//!   for game-defined predicates.
//! - [`WorldStateProp`] is the named address of a field. It's
//!   `#[non_exhaustive]` and has a `Custom { id }` variant so games can
//!   extend the world state without touching the engine.
//! - [`Goal`] is a priority and a list of `(prop, value)` pairs the
//!   planner tries to satisfy.
//! - [`ActionDef`] is a named action with preconditions (required
//!   world state) and effects (world state after the action runs).
//! - [`plan`] runs the planner. It sorts goals by priority and returns
//!   the first action of the best plan found within a depth limit of 4.
//!
//! # What's NOT in this module
//!
//! Bevy integration, entity-specific state gathering, action dispatch,
//! and the game's trait-driven goal/action builders all live in the
//! game crate. The engine ships only the data structures and the
//! planner algorithm.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BinaryHeap;

// =====================================================================
// World State
// =====================================================================

/// Boolean facts about an entity and its environment, gathered each turn.
///
/// The engine ships with a blessed set of named fields for commonly-useful
/// predicates. Games can extend the world state with custom props via the
/// `custom` map (keyed by game-assigned `u32` ids) without editing the
/// struct. `BTreeMap` is used so `WorldState` stays `Hash + Eq` for planner
/// caching.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WorldState {
    // --- Core (all GOAP entities) ---
    pub player_visible: bool,
    pub hostile_nearby: bool,
    pub hp_low: bool,
    pub has_escape_route: bool,
    pub adjacent_to_threat: bool,

    // --- Kobold hoarder ---
    pub carrying_items: bool,
    pub at_hoard: bool,
    pub item_visible: bool,
    pub adjacent_to_item: bool,
    pub adjacent_to_chest: bool,

    // --- Squad-derived (set from SquadBlackboard) ---
    pub squad_retreating: bool,
    pub near_leader: bool,
    pub self_morale_low: bool,
    pub can_cast_useful_spell: bool,
    pub ally_between_self_and_threat: bool,

    // --- Extension point ---
    /// Game-registered custom predicates keyed by game-assigned id.
    pub custom: BTreeMap<u32, bool>,
}

/// Named property of the world state, used in goal conditions and action
/// preconditions/effects.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldStateProp {
    PlayerVisible,
    HostileNearby,
    HpLow,
    HasEscapeRoute,
    AdjacentToThreat,
    CarryingItems,
    AtHoard,
    ItemVisible,
    AdjacentToItem,
    AdjacentToChest,
    SquadRetreating,
    NearLeader,
    SelfMoraleLow,
    CanCastUsefulSpell,
    AllyBetweenSelfAndThreat,
    /// Game-defined custom predicate identified by `id`. Games read/write
    /// these through the usual `(WorldStateProp, bool)` planner tuples.
    Custom { id: u32 },
}

impl WorldState {
    fn get(&self, prop: WorldStateProp) -> bool {
        match prop {
            WorldStateProp::PlayerVisible => self.player_visible,
            WorldStateProp::HostileNearby => self.hostile_nearby,
            WorldStateProp::HpLow => self.hp_low,
            WorldStateProp::HasEscapeRoute => self.has_escape_route,
            WorldStateProp::AdjacentToThreat => self.adjacent_to_threat,
            WorldStateProp::CarryingItems => self.carrying_items,
            WorldStateProp::AtHoard => self.at_hoard,
            WorldStateProp::ItemVisible => self.item_visible,
            WorldStateProp::AdjacentToItem => self.adjacent_to_item,
            WorldStateProp::AdjacentToChest => self.adjacent_to_chest,
            WorldStateProp::SquadRetreating => self.squad_retreating,
            WorldStateProp::NearLeader => self.near_leader,
            WorldStateProp::SelfMoraleLow => self.self_morale_low,
            WorldStateProp::CanCastUsefulSpell => self.can_cast_useful_spell,
            WorldStateProp::AllyBetweenSelfAndThreat => self.ally_between_self_and_threat,
            WorldStateProp::Custom { id } => self.custom.get(&id).copied().unwrap_or(false),
        }
    }

    fn set(&mut self, prop: WorldStateProp, value: bool) {
        match prop {
            WorldStateProp::PlayerVisible => self.player_visible = value,
            WorldStateProp::HostileNearby => self.hostile_nearby = value,
            WorldStateProp::HpLow => self.hp_low = value,
            WorldStateProp::HasEscapeRoute => self.has_escape_route = value,
            WorldStateProp::AdjacentToThreat => self.adjacent_to_threat = value,
            WorldStateProp::CarryingItems => self.carrying_items = value,
            WorldStateProp::AtHoard => self.at_hoard = value,
            WorldStateProp::ItemVisible => self.item_visible = value,
            WorldStateProp::AdjacentToItem => self.adjacent_to_item = value,
            WorldStateProp::AdjacentToChest => self.adjacent_to_chest = value,
            WorldStateProp::SquadRetreating => self.squad_retreating = value,
            WorldStateProp::NearLeader => self.near_leader = value,
            WorldStateProp::SelfMoraleLow => self.self_morale_low = value,
            WorldStateProp::CanCastUsefulSpell => self.can_cast_useful_spell = value,
            WorldStateProp::AllyBetweenSelfAndThreat => self.ally_between_self_and_threat = value,
            WorldStateProp::Custom { id } => {
                self.custom.insert(id, value);
            }
        }
    }
}

// =====================================================================
// Goals and Actions
// =====================================================================

/// A desired partial world state. Higher priority goals are attempted first.
#[derive(Clone, Debug)]
pub struct Goal {
    pub name: &'static str,
    pub priority: u32,
    pub desired: Vec<(WorldStateProp, bool)>,
}

/// An action the entity can take. Has preconditions (required world state)
/// and effects (world state changes after the action).
#[derive(Clone, Debug)]
pub struct ActionDef {
    pub name: &'static str,
    pub cost: u32,
    pub preconditions: Vec<(WorldStateProp, bool)>,
    pub effects: Vec<(WorldStateProp, bool)>,
}

// =====================================================================
// Planner
// =====================================================================

/// Search node for the forward planner.
#[derive(Clone, Debug)]
struct SearchNode {
    state: WorldState,
    actions: Vec<usize>, // indices into the action list
    cost: u32,
}

impl Eq for SearchNode {}
impl PartialEq for SearchNode {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost
    }
}
impl PartialOrd for SearchNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SearchNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (lowest cost first).
        other.cost.cmp(&self.cost)
    }
}

fn goal_satisfied(state: &WorldState, goal: &Goal) -> bool {
    goal.desired.iter().all(|(prop, val)| state.get(*prop) == *val)
}

fn preconditions_met(state: &WorldState, action: &ActionDef) -> bool {
    action
        .preconditions
        .iter()
        .all(|(prop, val)| state.get(*prop) == *val)
}

fn apply_effects(state: &mut WorldState, action: &ActionDef) {
    for (prop, val) in &action.effects {
        state.set(*prop, *val);
    }
}

/// Find the cheapest action sequence that transforms `current` into a state
/// satisfying `goal`. Returns `None` if no plan exists within `max_depth` steps.
fn search(
    current: &WorldState,
    goal: &Goal,
    actions: &[ActionDef],
    max_depth: usize,
) -> Option<Vec<usize>> {
    if goal_satisfied(current, goal) {
        return None; // Already achieved.
    }

    let mut heap = BinaryHeap::new();
    heap.push(SearchNode {
        state: current.clone(),
        actions: vec![],
        cost: 0,
    });

    let mut best: Option<Vec<usize>> = None;
    let mut best_cost = u32::MAX;

    while let Some(node) = heap.pop() {
        if node.cost >= best_cost {
            continue; // Pruned — already found a cheaper plan.
        }
        if node.actions.len() >= max_depth {
            continue; // Depth limit.
        }

        for (i, action) in actions.iter().enumerate() {
            if !preconditions_met(&node.state, action) {
                continue;
            }

            let mut new_state = node.state.clone();
            apply_effects(&mut new_state, action);

            let mut new_actions = node.actions.clone();
            new_actions.push(i);
            let new_cost = node.cost + action.cost;

            if goal_satisfied(&new_state, goal) && new_cost < best_cost {
                best_cost = new_cost;
                best = Some(new_actions);
            } else if new_cost < best_cost {
                heap.push(SearchNode {
                    state: new_state,
                    actions: new_actions,
                    cost: new_cost,
                });
            }
        }
    }

    best
}

/// Run the GOAP planner: try each goal in priority order, return the first
/// action of the cheapest plan. Returns `None` if no goal needs action.
pub fn plan<'a>(
    current: &WorldState,
    goals: &[Goal],
    actions: &'a [ActionDef],
) -> Option<&'a ActionDef> {
    let mut sorted_goals: Vec<&Goal> = goals.iter().collect();
    sorted_goals.sort_by(|a, b| b.priority.cmp(&a.priority));

    for goal in sorted_goals {
        if goal_satisfied(current, goal) {
            continue;
        }
        if let Some(plan) = search(current, goal, actions, 4) {
            if let Some(&first_action_idx) = plan.first() {
                return Some(&actions[first_action_idx]);
            }
        }
    }
    None
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Extensibility: custom WorldStateProp (WorldStateProp::Custom) ---

    const CUSTOM_ALERTED: u32 = 1;
    const CUSTOM_TORCH_LIT: u32 = 2;

    #[test]
    fn custom_world_state_props_default_false() {
        let state = WorldState::default();
        assert!(!state.get(WorldStateProp::Custom { id: CUSTOM_ALERTED }));
        assert!(!state.get(WorldStateProp::Custom { id: 999 }));
    }

    #[test]
    fn custom_world_state_props_set_and_get() {
        let mut state = WorldState::default();
        state.set(WorldStateProp::Custom { id: CUSTOM_ALERTED }, true);
        assert!(state.get(WorldStateProp::Custom { id: CUSTOM_ALERTED }));
        assert!(!state.get(WorldStateProp::Custom { id: CUSTOM_TORCH_LIT }));
    }

    #[test]
    fn planner_consumes_custom_preconditions_and_effects() {
        // A single-step plan: light the torch → alerted becomes true.
        let actions = vec![ActionDef {
            name: "light_torch",
            cost: 1,
            preconditions: vec![(WorldStateProp::Custom { id: CUSTOM_TORCH_LIT }, false)],
            effects: vec![
                (WorldStateProp::Custom { id: CUSTOM_TORCH_LIT }, true),
                (WorldStateProp::Custom { id: CUSTOM_ALERTED }, true),
            ],
        }];
        let goal = Goal {
            name: "rally",
            priority: 1,
            desired: vec![(WorldStateProp::Custom { id: CUSTOM_ALERTED }, true)],
        };
        let state = WorldState::default();
        let selected = plan(&state, &[goal], &actions);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().name, "light_torch");
    }

    #[test]
    fn planner_skips_custom_goal_already_satisfied() {
        let mut state = WorldState::default();
        state.set(WorldStateProp::Custom { id: CUSTOM_ALERTED }, true);
        let actions = vec![ActionDef {
            name: "shout_alarm",
            cost: 1,
            preconditions: vec![],
            effects: vec![(WorldStateProp::Custom { id: CUSTOM_ALERTED }, true)],
        }];
        let goal = Goal {
            name: "stay_alert",
            priority: 1,
            desired: vec![(WorldStateProp::Custom { id: CUSTOM_ALERTED }, true)],
        };
        assert!(plan(&state, &[goal], &actions).is_none());
    }
}
