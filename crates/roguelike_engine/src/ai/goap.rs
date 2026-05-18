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
//!   adjacent to threat, squad retreating, ...).
//! - [`WorldStateProp`] is the named address of a field.
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
use std::collections::BinaryHeap;
use std::collections::HashSet;

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
    let mut visited = HashSet::new();

    heap.push(SearchNode {
        state: current.clone(),
        actions: vec![],
        cost: 0,
    });
    visited.insert(current.clone());

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

            // Skip if we've already explored this state
            if visited.contains(&new_state) {
                continue;
            }
            visited.insert(new_state.clone());

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

/// Like [`plan`], but returns the complete action sequence (all action
/// names) instead of just the first action. Useful for debugging AI
/// behavior and for tests that verify multi-step plans.
pub fn plan_full<'a>(
    current: &WorldState,
    goals: &[Goal],
    actions: &'a [ActionDef],
) -> Option<Vec<&'a str>> {
    let mut sorted_goals: Vec<&Goal> = goals.iter().collect();
    sorted_goals.sort_by(|a, b| b.priority.cmp(&a.priority));

    for goal in sorted_goals {
        if goal_satisfied(current, goal) {
            continue;
        }
        if let Some(plan_indices) = search(current, goal, actions, 4) {
            return Some(plan_indices.iter().map(|&i| actions[i].name).collect());
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

    // --- Blessed prop tests ---

    #[test]
    fn plan_with_blessed_props_single_step() {
        // Monster sees player, needs to be adjacent to attack
        let mut current = WorldState::default();
        current.player_visible = true;

        let actions = vec![ActionDef {
            name: "engage_enemy",
            cost: 1,
            preconditions: vec![(WorldStateProp::PlayerVisible, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, true)],
        }];
        let goal = Goal {
            name: "attack",
            priority: 1,
            desired: vec![(WorldStateProp::AdjacentToThreat, true)],
        };
        let result = plan(&current, &[goal], &actions);
        assert_eq!(result.unwrap().name, "engage_enemy");
    }

    #[test]
    fn plan_two_step_chain() {
        // Must first spot player, then engage
        let actions = vec![
            ActionDef {
                name: "search",
                cost: 1,
                preconditions: vec![],
                effects: vec![(WorldStateProp::PlayerVisible, true)],
            },
            ActionDef {
                name: "engage",
                cost: 1,
                preconditions: vec![(WorldStateProp::PlayerVisible, true)],
                effects: vec![(WorldStateProp::AdjacentToThreat, true)],
            },
        ];
        let goal = Goal {
            name: "attack",
            priority: 1,
            desired: vec![(WorldStateProp::AdjacentToThreat, true)],
        };
        let current = WorldState::default();
        let result = plan(&current, &[goal], &actions);
        assert_eq!(result.unwrap().name, "search"); // First step of two-step plan
    }

    #[test]
    fn plan_full_returns_complete_sequence() {
        let actions = vec![
            ActionDef {
                name: "search",
                cost: 1,
                preconditions: vec![],
                effects: vec![(WorldStateProp::PlayerVisible, true)],
            },
            ActionDef {
                name: "engage",
                cost: 1,
                preconditions: vec![(WorldStateProp::PlayerVisible, true)],
                effects: vec![(WorldStateProp::AdjacentToThreat, true)],
            },
        ];
        let goal = Goal {
            name: "attack",
            priority: 1,
            desired: vec![(WorldStateProp::AdjacentToThreat, true)],
        };
        let result = plan_full(&WorldState::default(), &[goal], &actions);
        assert_eq!(result.unwrap(), vec!["search", "engage"]);
    }

    #[test]
    fn plan_three_step_chain() {
        let actions = vec![
            ActionDef { name: "a", cost: 1, preconditions: vec![], effects: vec![(WorldStateProp::PlayerVisible, true)] },
            ActionDef { name: "b", cost: 1, preconditions: vec![(WorldStateProp::PlayerVisible, true)], effects: vec![(WorldStateProp::HostileNearby, true)] },
            ActionDef { name: "c", cost: 1, preconditions: vec![(WorldStateProp::HostileNearby, true)], effects: vec![(WorldStateProp::AdjacentToThreat, true)] },
        ];
        let goal = Goal { name: "g", priority: 1, desired: vec![(WorldStateProp::AdjacentToThreat, true)] };
        let result = plan_full(&WorldState::default(), &[goal], &actions);
        assert_eq!(result.unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn plan_priority_ordering_higher_first() {
        // Two goals: high-priority flee and low-priority attack
        let mut current = WorldState::default();
        current.hp_low = true;

        let actions = vec![
            ActionDef {
                name: "flee",
                cost: 1,
                preconditions: vec![(WorldStateProp::HpLow, true)],
                effects: vec![(WorldStateProp::HasEscapeRoute, true)],
            },
            ActionDef {
                name: "attack",
                cost: 1,
                preconditions: vec![],
                effects: vec![(WorldStateProp::AdjacentToThreat, true)],
            },
        ];
        let goals = vec![
            Goal { name: "survive", priority: 10, desired: vec![(WorldStateProp::HasEscapeRoute, true)] },
            Goal { name: "kill", priority: 1, desired: vec![(WorldStateProp::AdjacentToThreat, true)] },
        ];
        let result = plan(&current, &goals, &actions);
        assert_eq!(result.unwrap().name, "flee"); // Higher priority goal
    }

    #[test]
    fn plan_unreachable_goal_returns_none() {
        // Goal requires adjacent_to_threat, but no action produces it
        let actions = vec![ActionDef {
            name: "wander",
            cost: 1,
            preconditions: vec![],
            effects: vec![(WorldStateProp::PlayerVisible, true)],
        }];
        let goal = Goal {
            name: "attack",
            priority: 1,
            desired: vec![(WorldStateProp::AdjacentToThreat, true)],
        };
        assert!(plan(&WorldState::default(), &[goal], &actions).is_none());
    }

    #[test]
    fn plan_empty_actions_returns_none() {
        let goal = Goal {
            name: "g",
            priority: 1,
            desired: vec![(WorldStateProp::PlayerVisible, true)],
        };
        assert!(plan(&WorldState::default(), &[goal], &[]).is_none());
    }

    #[test]
    fn plan_all_goals_satisfied_returns_none() {
        let mut current = WorldState::default();
        current.player_visible = true;
        current.adjacent_to_threat = true;

        let actions = vec![ActionDef {
            name: "attack",
            cost: 1,
            preconditions: vec![],
            effects: vec![(WorldStateProp::AdjacentToThreat, true)],
        }];
        let goals = vec![
            Goal { name: "see", priority: 2, desired: vec![(WorldStateProp::PlayerVisible, true)] },
            Goal { name: "hit", priority: 1, desired: vec![(WorldStateProp::AdjacentToThreat, true)] },
        ];
        assert!(plan(&current, &goals, &actions).is_none());
    }

    #[test]
    fn plan_chooses_cheaper_plan() {
        // Two paths to same goal: cheap (cost 1) and expensive (cost 10)
        let actions = vec![
            ActionDef {
                name: "cheap_engage",
                cost: 1,
                preconditions: vec![],
                effects: vec![(WorldStateProp::AdjacentToThreat, true)],
            },
            ActionDef {
                name: "expensive_engage",
                cost: 10,
                preconditions: vec![],
                effects: vec![(WorldStateProp::AdjacentToThreat, true)],
            },
        ];
        let goal = Goal {
            name: "attack",
            priority: 1,
            desired: vec![(WorldStateProp::AdjacentToThreat, true)],
        };
        let result = plan(&WorldState::default(), &[goal], &actions);
        assert_eq!(result.unwrap().name, "cheap_engage");
    }

    #[test]
    fn plan_skips_satisfied_goal_tries_next() {
        // First goal already satisfied, second needs work
        let mut current = WorldState::default();
        current.player_visible = true;

        let actions = vec![ActionDef {
            name: "engage",
            cost: 1,
            preconditions: vec![],
            effects: vec![(WorldStateProp::AdjacentToThreat, true)],
        }];
        let goals = vec![
            Goal { name: "see", priority: 10, desired: vec![(WorldStateProp::PlayerVisible, true)] },
            Goal { name: "hit", priority: 5, desired: vec![(WorldStateProp::AdjacentToThreat, true)] },
        ];
        let result = plan(&current, &goals, &actions);
        assert_eq!(result.unwrap().name, "engage"); // Skips "see", works on "hit"
    }

    #[test]
    fn plan_precondition_blocks_action() {
        // Action has a precondition that's not met
        let actions = vec![ActionDef {
            name: "attack",
            cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true)],
            effects: vec![(WorldStateProp::HpLow, false)], // random effect
        }];
        let goal = Goal {
            name: "g",
            priority: 1,
            desired: vec![(WorldStateProp::HpLow, false)],
        };
        // current has hp_low=false already by default, but let's set it true
        let mut current = WorldState::default();
        current.hp_low = true;
        // adjacent_to_threat is false, so "attack" precondition fails
        assert!(plan(&current, &[goal], &actions).is_none());
    }

    #[test]
    fn plan_depth_limit_exceeded_returns_none() {
        // Create a chain that requires 5 steps (depth limit is 4)
        let actions = vec![
            ActionDef { name: "step1", cost: 1, preconditions: vec![], effects: vec![(WorldStateProp::PlayerVisible, true)] },
            ActionDef { name: "step2", cost: 1, preconditions: vec![(WorldStateProp::PlayerVisible, true)], effects: vec![(WorldStateProp::HostileNearby, true)] },
            ActionDef { name: "step3", cost: 1, preconditions: vec![(WorldStateProp::HostileNearby, true)], effects: vec![(WorldStateProp::HasEscapeRoute, true)] },
            ActionDef { name: "step4", cost: 1, preconditions: vec![(WorldStateProp::HasEscapeRoute, true)], effects: vec![(WorldStateProp::CarryingItems, true)] },
            ActionDef { name: "step5", cost: 1, preconditions: vec![(WorldStateProp::CarryingItems, true)], effects: vec![(WorldStateProp::AdjacentToThreat, true)] },
        ];
        let goal = Goal {
            name: "deep",
            priority: 1,
            desired: vec![(WorldStateProp::AdjacentToThreat, true)],
        };
        // Requires 5 steps but depth limit is 4
        assert!(plan(&WorldState::default(), &[goal], &actions).is_none());
    }
}
