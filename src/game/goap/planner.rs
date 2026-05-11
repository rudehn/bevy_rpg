//! Goal-Oriented Action Planning (GOAP) AI system.
//!
//! The planner runs each turn for GOAP-enabled entities, producing the single
//! best action to take based on the current world state and prioritized goals.

use std::collections::BinaryHeap;
use std::cmp::Ordering;

// =====================================================================
// World State
// =====================================================================

/// Boolean facts about an entity and its environment, gathered each turn.
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
}

/// Named property of the world state, used in goal conditions and action
/// preconditions/effects.
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
    fn eq(&self, other: &Self) -> bool { self.cost == other.cost }
}
impl PartialOrd for SearchNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
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
    action.preconditions.iter().all(|(prop, val)| state.get(*prop) == *val)
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
