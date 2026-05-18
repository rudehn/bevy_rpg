//! Goal-Oriented Action Planning (GOAP) AI — game-side integration.
//!
//! The planner runs each turn for GOAP-enabled entities, producing the single
//! best action to take based on the current world state and prioritized goals.
//!
//! The pure planner framework (`WorldState`, `WorldStateProp`, `Goal`,
//! `ActionDef`, `plan()`) now lives in `roguelike_engine::ai::goap`. This
//! file keeps the game-specific parts: the trait-driven goal/action
//! builders (`build_goap_config`), the legacy archetype functions, the
//! `GoapAI` component, the Bevy dispatch systems, and the action handlers
//! that touch game-side types.

// Re-export the engine planner framework so existing game-side code can
// keep using `crate::game::goap::{WorldState, Goal, ActionDef, plan, ...}`
// unchanged.
pub use roguelike_engine::ai::goap::{ActionDef, Goal, WorldState, WorldStateProp, plan};

// =====================================================================
// Trait-based GOAP Configuration Builder
// =====================================================================

use crate::assets::AiTrait;

/// Build a GOAP goal/action configuration from a set of behavioral traits.
/// Replaces the old per-archetype functions with a composable builder.
pub fn build_goap_config(
    traits: &[AiTrait],
    has_spells: bool,
    has_high_armor: bool,
    is_squad_member: bool,
) -> (Vec<Goal>, Vec<ActionDef>) {
    let mut goals = Vec::new();
    let mut actions = Vec::new();

    let is_cowardly = traits.iter().any(|t| matches!(t, AiTrait::Cowardly));
    let is_aggressive = traits.iter().any(|t| matches!(t, AiTrait::Aggressive));
    let is_reckless = traits.iter().any(|t| matches!(t, AiTrait::Reckless));
    let _is_mindless = traits.iter().any(|t| matches!(t, AiTrait::Mindless));
    let is_bestial = traits.iter().any(|t| matches!(t, AiTrait::Bestial));
    let is_intelligent = traits.iter().any(|t| matches!(t, AiTrait::Intelligent));
    let is_hoarder = traits.iter().any(|t| matches!(t, AiTrait::Hoarder));
    let is_support = traits.iter().any(|t| matches!(t, AiTrait::Support));
    let is_commander = traits.iter().any(|t| matches!(t, AiTrait::Commander));
    let ranged_range = traits.iter().find_map(|t| match t {
        AiTrait::Ranged { range } => Some(*range),
        _ => None,
    });

    // --- Base: always present ---
    if !is_reckless {
        goals.push(Goal { name: "survive", priority: 10, desired: vec![(WorldStateProp::AdjacentToThreat, false)] });
    }

    // --- Courage ---
    if is_cowardly {
        // Cowardly monsters flee when hurt/demoralized, not just because a threat is adjacent.
        actions.push(ActionDef {
            name: "flee", cost: 1,
            preconditions: vec![
                (WorldStateProp::AdjacentToThreat, true),
                (WorldStateProp::HasEscapeRoute, true),
                (WorldStateProp::HpLow, true), // Only flee when actually hurt
            ],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        });
    } else if is_aggressive || is_reckless {
        // No flee action. Low-cost melee.
        actions.push(ActionDef {
            name: "attack_melee", cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        });
        actions.push(ActionDef {
            name: "engage_enemy", cost: 2,
            preconditions: vec![(WorldStateProp::PlayerVisible, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, true)],
        });
        goals.push(Goal { name: "engage", priority: 5, desired: vec![(WorldStateProp::AdjacentToThreat, true)] });
    } else {
        // Default: moderate flee
        actions.push(ActionDef {
            name: "flee", cost: 3,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true), (WorldStateProp::HasEscapeRoute, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        });
    }

    // Default melee if not already added by aggressive
    if !is_aggressive && !is_reckless {
        let melee_cost = if has_high_armor { 2 } else { 4 };
        actions.push(ActionDef {
            name: "attack_melee", cost: melee_cost,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        });
    }

    // Engage for non-aggressive (if player visible + no other priority)
    if !is_aggressive && !is_reckless {
        actions.push(ActionDef {
            name: "engage_enemy", cost: 3,
            preconditions: vec![(WorldStateProp::PlayerVisible, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, true)],
        });
        goals.push(Goal { name: "engage", priority: 3, desired: vec![(WorldStateProp::AdjacentToThreat, true)] });
    }

    // --- Intelligence: tactical actions ---
    if is_intelligent || is_bestial {
        // Retreat (not mindless)
        actions.push(ActionDef {
            name: "retreat_to_fallback", cost: 1,
            preconditions: vec![(WorldStateProp::SquadRetreating, true)],
            effects: vec![(WorldStateProp::SquadRetreating, false)],
        });
        goals.push(Goal { name: "retreat", priority: 8, desired: vec![(WorldStateProp::SquadRetreating, false)] });
    }

    if is_intelligent {
        // Repositioning
        actions.push(ActionDef {
            name: "reposition_behind_ally", cost: 2,
            preconditions: vec![(WorldStateProp::AllyBetweenSelfAndThreat, false)],
            effects: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)],
        });

        // Spell casting (if has spells)
        if has_spells {
            actions.push(ActionDef {
                name: "cast_spell", cost: 2,
                preconditions: vec![(WorldStateProp::CanCastUsefulSpell, true)],
                effects: vec![(WorldStateProp::CanCastUsefulSpell, false)],
            });
        }
    }

    // --- Squad ---
    if is_squad_member {
        goals.push(Goal { name: "follow_squad", priority: 7, desired: vec![(WorldStateProp::NearLeader, true)] });
        actions.push(ActionDef {
            name: "move_to_leader", cost: 3,
            preconditions: vec![(WorldStateProp::NearLeader, false)],
            effects: vec![(WorldStateProp::NearLeader, true)],
        });
    }

    if is_commander {
        goals.push(Goal { name: "order_retreat", priority: 9, desired: vec![(WorldStateProp::SelfMoraleLow, false)] });
        goals.push(Goal { name: "command_position", priority: 6, desired: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)] });
        actions.push(ActionDef {
            name: "order_retreat", cost: 1,
            preconditions: vec![(WorldStateProp::SelfMoraleLow, true)],
            effects: vec![(WorldStateProp::SelfMoraleLow, false)],
        });
        actions.push(ActionDef {
            name: "command_position", cost: 2,
            preconditions: vec![(WorldStateProp::AllyBetweenSelfAndThreat, false)],
            effects: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)],
        });

        if has_spells {
            goals.push(Goal { name: "cast_spells", priority: 7, desired: vec![(WorldStateProp::CanCastUsefulSpell, false)] });
            // cast_spell action already added by Intelligent check above if has_spells
        }
    }

    if is_support {
        goals.push(Goal { name: "cast_spells", priority: 6, desired: vec![(WorldStateProp::CanCastUsefulSpell, false)] });
        goals.push(Goal { name: "stay_safe", priority: 7, desired: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)] });
        // cast_spell action already added by Intelligent check if applicable
    }

    // --- Ranged ---
    if let Some(_range) = ranged_range {
        actions.push(ActionDef {
            name: "ranged_attack", cost: 3,
            preconditions: vec![(WorldStateProp::PlayerVisible, true), (WorldStateProp::AllyBetweenSelfAndThreat, true)],
            effects: vec![],
        });
        if !is_intelligent {
            // Non-intelligent ranged: basic repositioning
            actions.push(ActionDef {
                name: "reposition_behind_ally", cost: 2,
                preconditions: vec![(WorldStateProp::AllyBetweenSelfAndThreat, false)],
                effects: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)],
            });
        }
        goals.push(Goal { name: "maintain_distance", priority: 6, desired: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)] });
    }

    // --- Hoarder ---
    if is_hoarder {
        goals.push(Goal { name: "hoard_treasure", priority: 5, desired: vec![
            (WorldStateProp::CarryingItems, false), (WorldStateProp::AtHoard, true),
        ]});
        goals.push(Goal { name: "collect", priority: 3, desired: vec![(WorldStateProp::CarryingItems, true)] });

        actions.push(ActionDef {
            name: "seek_item", cost: 2,
            preconditions: vec![(WorldStateProp::ItemVisible, true), (WorldStateProp::AdjacentToItem, false), (WorldStateProp::AdjacentToChest, false)],
            effects: vec![(WorldStateProp::AdjacentToItem, true)],
        });
        actions.push(ActionDef {
            name: "pick_up_item", cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToItem, true)],
            effects: vec![(WorldStateProp::CarryingItems, true), (WorldStateProp::AdjacentToItem, false)],
        });
        actions.push(ActionDef {
            name: "open_chest", cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToChest, true)],
            effects: vec![(WorldStateProp::AdjacentToChest, false), (WorldStateProp::AdjacentToItem, true)],
        });
        actions.push(ActionDef {
            name: "return_to_hoard", cost: 3,
            preconditions: vec![(WorldStateProp::CarryingItems, true), (WorldStateProp::AtHoard, false)],
            effects: vec![(WorldStateProp::AtHoard, true)],
        });
        actions.push(ActionDef {
            name: "drop_items", cost: 1,
            preconditions: vec![(WorldStateProp::AtHoard, true), (WorldStateProp::CarryingItems, true)],
            effects: vec![(WorldStateProp::CarryingItems, false)],
        });
    }

    // --- Fallback ---
    actions.push(ActionDef { name: "roam", cost: 8, preconditions: vec![], effects: vec![] });

    (goals, actions)
}

// =====================================================================
// Legacy archetype functions (kept temporarily for reference)
// =====================================================================

#[allow(dead_code)]
pub fn kobold_hoarder_goals() -> Vec<Goal> {
    vec![
        Goal {
            name: "survive",
            priority: 10,
            desired: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        Goal {
            name: "hoard_treasure",
            priority: 5,
            desired: vec![
                (WorldStateProp::CarryingItems, false),
                (WorldStateProp::AtHoard, true),
            ],
        },
        Goal {
            name: "collect",
            priority: 3,
            desired: vec![(WorldStateProp::CarryingItems, true)],
        },
    ]
}

#[allow(dead_code)]
pub fn kobold_hoarder_actions() -> Vec<ActionDef> {
    vec![
        ActionDef {
            name: "flee",
            cost: 1,
            preconditions: vec![
                (WorldStateProp::AdjacentToThreat, true),
                (WorldStateProp::HasEscapeRoute, true),
            ],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        ActionDef {
            name: "attack",
            cost: 10,
            preconditions: vec![
                (WorldStateProp::AdjacentToThreat, true),
                (WorldStateProp::HasEscapeRoute, false),
            ],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        ActionDef {
            name: "seek_item",
            cost: 2,
            preconditions: vec![
                (WorldStateProp::ItemVisible, true),
                (WorldStateProp::AdjacentToItem, false),
                (WorldStateProp::AdjacentToChest, false),
            ],
            effects: vec![(WorldStateProp::AdjacentToItem, true)],
        },
        ActionDef {
            name: "pick_up_item",
            cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToItem, true)],
            effects: vec![
                (WorldStateProp::CarryingItems, true),
                (WorldStateProp::AdjacentToItem, false),
            ],
        },
        ActionDef {
            name: "open_chest",
            cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToChest, true)],
            effects: vec![
                (WorldStateProp::AdjacentToChest, false),
                (WorldStateProp::AdjacentToItem, true), // Chest spawns items on floor
            ],
        },
        ActionDef {
            name: "return_to_hoard",
            cost: 3,
            preconditions: vec![
                (WorldStateProp::CarryingItems, true),
                (WorldStateProp::AtHoard, false),
            ],
            effects: vec![(WorldStateProp::AtHoard, true)],
        },
        ActionDef {
            name: "drop_items",
            cost: 1,
            preconditions: vec![
                (WorldStateProp::AtHoard, true),
                (WorldStateProp::CarryingItems, true),
            ],
            effects: vec![(WorldStateProp::CarryingItems, false)],
        },
        ActionDef {
            name: "roam",
            cost: 8,
            preconditions: vec![],
            effects: vec![], // Fallback — doesn't advance any goal.
        },
    ]
}

// =====================================================================
// Goblin Grunt — cowardly melee, follows leader, flees when hurt
// =====================================================================

#[allow(dead_code)]
pub fn goblin_grunt_goals() -> Vec<Goal> {
    vec![
        Goal { name: "survive",       priority: 10, desired: vec![(WorldStateProp::AdjacentToThreat, false)] },
        Goal { name: "retreat",        priority: 8,  desired: vec![(WorldStateProp::SquadRetreating, false)] },
        Goal { name: "follow_squad",   priority: 7,  desired: vec![(WorldStateProp::NearLeader, true)] },
        Goal { name: "engage",         priority: 3,  desired: vec![(WorldStateProp::AdjacentToThreat, true)] },
    ]
}

#[allow(dead_code)]
pub fn goblin_grunt_actions() -> Vec<ActionDef> {
    vec![
        ActionDef {
            name: "flee",
            cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true), (WorldStateProp::HasEscapeRoute, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        ActionDef {
            name: "attack_melee",
            cost: 4,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        ActionDef {
            name: "move_to_leader",
            cost: 3,
            preconditions: vec![(WorldStateProp::NearLeader, false)],
            effects: vec![(WorldStateProp::NearLeader, true)],
        },
        ActionDef {
            name: "retreat_to_fallback",
            cost: 1,
            preconditions: vec![(WorldStateProp::SquadRetreating, true)],
            effects: vec![(WorldStateProp::SquadRetreating, false)],
        },
        ActionDef {
            name: "engage_enemy",
            cost: 3,
            preconditions: vec![(WorldStateProp::PlayerVisible, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, true)],
        },
        ActionDef { name: "roam", cost: 8, preconditions: vec![], effects: vec![] },
    ]
}

// =====================================================================
// Goblin Archer — skirmisher, stays behind allies, ranged attacks
// =====================================================================

#[allow(dead_code)]
pub fn goblin_archer_goals() -> Vec<Goal> {
    vec![
        Goal { name: "survive",           priority: 10, desired: vec![(WorldStateProp::AdjacentToThreat, false)] },
        Goal { name: "retreat",            priority: 8,  desired: vec![(WorldStateProp::SquadRetreating, false)] },
        Goal { name: "maintain_distance",  priority: 6,  desired: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)] },
        // Note: engage_ranged is handled as a default action when safe.
        // The archer attacks when player is visible AND ally is between.
        // Since ranged_attack has no state effects, it's dispatched as the
        // fallback (same as roam) when all goals are satisfied.
    ]
}

#[allow(dead_code)]
pub fn goblin_archer_actions() -> Vec<ActionDef> {
    vec![
        ActionDef {
            name: "flee",
            cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true), (WorldStateProp::HasEscapeRoute, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        ActionDef {
            name: "ranged_attack",
            cost: 3,
            preconditions: vec![(WorldStateProp::PlayerVisible, true), (WorldStateProp::AllyBetweenSelfAndThreat, true)],
            effects: vec![],
        },
        ActionDef {
            name: "reposition_behind_ally",
            cost: 2,
            preconditions: vec![(WorldStateProp::AllyBetweenSelfAndThreat, false)],
            effects: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)],
        },
        ActionDef {
            name: "retreat_to_fallback",
            cost: 1,
            preconditions: vec![(WorldStateProp::SquadRetreating, true)],
            effects: vec![(WorldStateProp::SquadRetreating, false)],
        },
        ActionDef { name: "roam", cost: 8, preconditions: vec![], effects: vec![] },
    ]
}

// =====================================================================
// Goblin Brute — bodyguard, holds chokepoints, reluctant to flee
// =====================================================================

#[allow(dead_code)]
pub fn goblin_brute_goals() -> Vec<Goal> {
    vec![
        Goal { name: "survive",         priority: 10, desired: vec![(WorldStateProp::HpLow, false)] },
        Goal { name: "protect_leader",  priority: 8,  desired: vec![(WorldStateProp::NearLeader, true)] },
        Goal { name: "engage",          priority: 5,  desired: vec![(WorldStateProp::AdjacentToThreat, true)] },
    ]
}

#[allow(dead_code)]
pub fn goblin_brute_actions() -> Vec<ActionDef> {
    vec![
        ActionDef {
            name: "flee",
            cost: 5, // Brutes are very reluctant to flee
            preconditions: vec![(WorldStateProp::HpLow, true), (WorldStateProp::HasEscapeRoute, true)],
            effects: vec![(WorldStateProp::HpLow, false)], // abstract: "not dying anymore"
        },
        ActionDef {
            name: "attack_melee",
            cost: 2, // Low cost — brutes prefer fighting
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        ActionDef {
            name: "move_to_leader",
            cost: 3,
            preconditions: vec![(WorldStateProp::NearLeader, false)],
            effects: vec![(WorldStateProp::NearLeader, true)],
        },
        ActionDef {
            name: "engage_enemy",
            cost: 3,
            preconditions: vec![(WorldStateProp::PlayerVisible, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, true)],
        },
        ActionDef { name: "roam", cost: 8, preconditions: vec![], effects: vec![] },
    ]
}

// =====================================================================
// Goblin Shaman — support, heals allies, stays behind the line
// =====================================================================

#[allow(dead_code)]
pub fn goblin_support_goals() -> Vec<Goal> {
    vec![
        Goal { name: "survive",    priority: 10, desired: vec![(WorldStateProp::AdjacentToThreat, false)] },
        Goal { name: "retreat",     priority: 8,  desired: vec![(WorldStateProp::SquadRetreating, false)] },
        Goal { name: "stay_safe",   priority: 7,  desired: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)] },
        Goal { name: "cast_spells", priority: 6,  desired: vec![(WorldStateProp::CanCastUsefulSpell, false)] },
        Goal { name: "follow",      priority: 4,  desired: vec![(WorldStateProp::NearLeader, true)] },
    ]
}

#[allow(dead_code)]
pub fn goblin_support_actions() -> Vec<ActionDef> {
    vec![
        ActionDef {
            name: "flee",
            cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true), (WorldStateProp::HasEscapeRoute, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        ActionDef {
            name: "cast_spell",
            cost: 2,
            preconditions: vec![(WorldStateProp::CanCastUsefulSpell, true)],
            effects: vec![(WorldStateProp::CanCastUsefulSpell, false)],
        },
        ActionDef {
            name: "reposition_behind_ally",
            cost: 2,
            preconditions: vec![(WorldStateProp::AllyBetweenSelfAndThreat, false)],
            effects: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)],
        },
        ActionDef {
            name: "retreat_to_fallback",
            cost: 1,
            preconditions: vec![(WorldStateProp::SquadRetreating, true)],
            effects: vec![(WorldStateProp::SquadRetreating, false)],
        },
        ActionDef {
            name: "move_to_leader",
            cost: 3,
            preconditions: vec![(WorldStateProp::NearLeader, false)],
            effects: vec![(WorldStateProp::NearLeader, true)],
        },
        ActionDef { name: "roam", cost: 8, preconditions: vec![], effects: vec![] },
    ]
}

// =====================================================================
// Goblin Warchief — commander, buffs squad, orders retreat
// =====================================================================

#[allow(dead_code)]
pub fn goblin_commander_goals() -> Vec<Goal> {
    vec![
        Goal { name: "survive",        priority: 10, desired: vec![(WorldStateProp::AdjacentToThreat, false)] },
        Goal { name: "order_retreat",   priority: 9,  desired: vec![(WorldStateProp::SelfMoraleLow, false)] },
        Goal { name: "cast_spells",     priority: 7,  desired: vec![(WorldStateProp::CanCastUsefulSpell, false)] },
        Goal { name: "command_position",priority: 6,  desired: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)] },
        Goal { name: "engage",          priority: 4,  desired: vec![(WorldStateProp::AdjacentToThreat, true)] },
    ]
}

#[allow(dead_code)]
pub fn goblin_commander_actions() -> Vec<ActionDef> {
    vec![
        ActionDef {
            name: "flee",
            cost: 1,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true), (WorldStateProp::HasEscapeRoute, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        ActionDef {
            name: "cast_spell",
            cost: 3,
            preconditions: vec![(WorldStateProp::CanCastUsefulSpell, true)],
            effects: vec![(WorldStateProp::CanCastUsefulSpell, false)],
        },
        ActionDef {
            name: "order_retreat",
            cost: 1,
            preconditions: vec![(WorldStateProp::SelfMoraleLow, true)],
            effects: vec![(WorldStateProp::SelfMoraleLow, false)],
        },
        ActionDef {
            name: "command_position",
            cost: 2,
            preconditions: vec![(WorldStateProp::AllyBetweenSelfAndThreat, false)],
            effects: vec![(WorldStateProp::AllyBetweenSelfAndThreat, true)],
        },
        ActionDef {
            name: "attack_melee",
            cost: 4,
            preconditions: vec![(WorldStateProp::AdjacentToThreat, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, false)],
        },
        ActionDef {
            name: "engage_enemy",
            cost: 3,
            preconditions: vec![(WorldStateProp::PlayerVisible, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, true)],
        },
        ActionDef { name: "roam", cost: 8, preconditions: vec![], effects: vec![] },
    ]
}

// =====================================================================
// Bevy Integration
// =====================================================================

use bevy::prelude::*;
use bracket_lib::prelude::{DistanceAlg, Point};

use crate::{
    components::{Chest, Faction, FloorEntityMarker, InInventory, Inventory, Item, Monster, Position, Viewshed},
    game::{
        actions::{ActionFinishedEvent, ActionGuard, MeleeIntent, MovementIntent, OpenChestIntent, PickUpIntent, WaitIntent},
        ai::{idle_movement, pathfind_toward, try_flee_movement, try_stun_skip},
        combat::Health,
        factions::FactionMatrix,
        turns::MyTurn,
    },
    map::Map,
    player::Player,
};

/// GOAP-driven AI component. Attached to entities that use the planner
/// instead of the standard `MonsterAI` state machine.
#[derive(Component)]
pub struct GoapAI {
    pub goals: Vec<Goal>,
    pub actions: Vec<ActionDef>,
    pub hoard_position: Option<Point>,
    /// Current roam destination. Cleared when reached or when a higher-priority action fires.
    pub roam_target: Option<Point>,
    /// Last action chosen by the planner, for UI display.
    pub last_action: Option<&'static str>,
}

impl GoapAI {
    /// Returns a human-readable label for the current AI state.
    pub fn display_state(&self) -> &'static str {
        match self.last_action {
            Some("attack_melee" | "ranged_attack") => "Hunting",
            Some("flee") => "Fleeing",
            Some("pick_up_item" | "move_to_item" | "loot_chest" | "drop_at_hoard") => "Looting",
            Some("roam" | "move_to_leader") => "Wandering",
            None => "Sleeping",
            _ => "Hunting",
        }
    }
}

/// Message emitted when a GOAP entity drops all inventory items at its current position.
#[derive(Message)]
pub struct DropAtHoardMessage {
    pub entity: Entity,
}

/// Dispatch system for GOAP-driven entities. Runs in `ProcessingPhase::Brain`
/// before `monster_ai_dispatch` so GOAP entities consume their `MyTurn` first.
pub fn goap_ai_dispatch(world: &mut World) {
    let mut query = world.query_filtered::<Entity, (With<GoapAI>, With<MyTurn>)>();
    let entities: Vec<Entity> = query.iter(world).collect();

    for entity in entities {
        if let Some(mut goap_ai) = world.entity_mut(entity).take::<GoapAI>() {
            world.entity_mut(entity).insert(ActionGuard);
            execute_goap(entity, &mut goap_ai, world);
            world.entity_mut(entity).insert(goap_ai);
            world.entity_mut(entity).remove::<MyTurn>();
        }
    }
}

fn execute_goap(entity: Entity, ai: &mut GoapAI, world: &mut World) {
    // 1. Stun check
    if try_stun_skip(entity, world) {
        return;
    }

    // 2. Gather world state
    let state = gather_world_state(entity, ai, world);

    // 3. Run planner
    let action_name = {
        let result = plan(&state, &ai.goals, &ai.actions);
        let name = result.map(|a| a.name);
        let monster_name = world.get::<crate::components::Name>(entity)
            .map(|n| n.0.clone())
            .unwrap_or_default();
        let monster_pos = world.get::<crate::components::Position>(entity)
            .map(|p| format!("({},{})", p.x, p.y))
            .unwrap_or_default();
        bevy::log::info!(
            "GOAP {} {} {entity:?}: action={:?} player_vis={}",
            monster_name, monster_pos, name, state.player_visible,
        );
        name
    };

    // 4. Store last action for UI display
    ai.last_action = action_name;

    // 5. Dispatch action
    match action_name {
        Some(name) => dispatch_action(entity, name, ai, world),
        None => {
            // All goals satisfied — try opportunistic actions before idling.
            // Ranged monsters shoot if player is visible and they're safe.
            if state.player_visible && state.ally_between_self_and_threat {
                dispatch_action(entity, "ranged_attack", ai, world);
            } else if state.player_visible && state.adjacent_to_threat {
                dispatch_action(entity, "attack_melee", ai, world);
            } else if ai.hoard_position.is_some() {
                // Hoarders roam to seek items when idle.
                dispatch_action(entity, "roam", ai, world);
            } else {
                // Combat-oriented monsters wait in place when all goals are satisfied.
                world.write_message(WaitIntent { entity });
            }
        }
    }
}

fn gather_world_state(entity: Entity, ai: &GoapAI, world: &mut World) -> WorldState {
    // Snapshot all entity data, then snapshot resources, releasing borrows before queries.
    let pos = world.get::<Position>(entity).map(|p| p.to_point()).unwrap_or(Point::new(0, 0));
    let visible_tiles: std::collections::HashSet<Point> = world.get::<Viewshed>(entity)
        .map(|v| v.visible_tiles.clone())
        .unwrap_or_default();
    let actor_faction = world.get::<Faction>(entity).cloned();
    let hp_low = world.get::<Health>(entity)
        .is_some_and(|h| h.max > 0 && (h.current as f32 / h.max as f32) < 0.3);
    let carrying_items = world.get::<Inventory>(entity)
        .is_some_and(|inv| !inv.items.is_empty());
    // Clone the faction matrix into a fully-owned value so world is not borrowed.
    let faction_matrix: FactionMatrix = world.resource::<FactionMatrix>().clone();

    let player_pos: Option<Point> = {
        let mut q = world.query_filtered::<&Position, With<Player>>();
        q.iter(world).next().map(|p| p.to_point())
    };
    let player_visible = player_pos
        .map(|pp| visible_tiles.contains(&pp))
        .unwrap_or(false);

    let adjacent_to_threat = player_pos
        .map(|pp| DistanceAlg::Chebyshev.distance2d(pos, pp) <= 1.5)
        .unwrap_or(false)
        && player_visible;

    // Has escape route (needs &mut World for pathfinding)
    let has_escape_route = if adjacent_to_threat {
        player_pos
            .map(|pp| try_flee_movement(entity, pos, pp, world).is_some())
            .unwrap_or(true)
    } else {
        true
    };

    let at_hoard = ai.hoard_position
        .map(|hp| pos == hp)
        .unwrap_or(false);

    // Scan for visible floor items and chests.
    // Exclude items at the hoard position to prevent pick-up/drop feedback loops.
    let hoard_pt = ai.hoard_position;
    let mut item_visible = false;
    let mut adjacent_to_item = false;
    let mut adjacent_to_chest = false;

    {
        let mut item_query = world.query_filtered::<&Position, (With<Item>, Without<InInventory>)>();
        for item_pos in item_query.iter(world) {
            let ipt = item_pos.to_point();
            // Skip items sitting at the hoard — those are already "delivered."
            if hoard_pt.is_some_and(|hp| ipt == hp) {
                continue;
            }
            if visible_tiles.contains(&ipt) {
                item_visible = true;
                if pos == ipt {
                    adjacent_to_item = true;
                }
            }
        }
    }
    {
        let mut chest_query = world.query_filtered::<&Position, With<Chest>>();
        for chest_pos in chest_query.iter(world) {
            let cpt = chest_pos.to_point();
            if visible_tiles.contains(&cpt) {
                item_visible = true;
                if DistanceAlg::Chebyshev.distance2d(pos, cpt) <= 1.5 {
                    adjacent_to_chest = true;
                }
            }
        }
    }

    // Hostile nearby
    let hostile_nearby = if let Some(af) = &actor_faction {
        let mut entity_query = world.query::<(&Position, &Faction)>();
        entity_query.iter(world).any(|(epos, efaction)| {
            let ept = epos.to_point();
            ept != pos && visible_tiles.contains(&ept) && faction_matrix.is_hostile_to(&af.0.0, &efaction.0.0)
        })
    } else {
        false
    };

    // --- Squad-derived state ---
    use crate::game::squad::{Morale, SquadBlackboard, SquadId, SquadLeader};

    let self_morale_low = world.get::<Morale>(entity)
        .map(|m| m.0 < 0.3)
        .unwrap_or(false);

    // Find this entity's squad blackboard (on the leader)
    let squad_id = world.get::<SquadId>(entity).copied();
    let (squad_retreating, near_leader) = if let Some(sid) = squad_id {
        let mut bb_query = world.query_filtered::<(&SquadId, &SquadBlackboard, &Position), With<SquadLeader>>();
        let bb_data = bb_query.iter(world)
            .find(|(leader_sid, _, _)| **leader_sid == sid)
            .map(|(_, bb, leader_pos)| (bb.retreat_ordered, leader_pos.to_point()));

        match bb_data {
            Some((retreating, leader_pt)) => {
                let near = DistanceAlg::Chebyshev.distance2d(pos, leader_pt) <= 4.0;
                (retreating, near)
            }
            None => (false, false),
        }
    } else {
        (false, false)
    };

    // Check if an ally is between us and the player.
    // Vacuously true when no same-faction allies exist — can't reposition behind nobody.
    let ally_between_self_and_threat = if let Some(pp) = player_pos {
        let self_dist = DistanceAlg::Chebyshev.distance2d(pos, pp);
        let mut ally_query = world.query_filtered::<(&Position, &Faction), With<Monster>>();
        let mut has_any_ally = false;
        let mut ally_closer = false;
        for (apos, afaction) in ally_query.iter(world) {
            let apt = apos.to_point();
            if apt == pos { continue; }
            if let Some(ref cf) = actor_faction {
                if !faction_matrix.is_allied_to(&cf.0.0, &afaction.0.0) { continue; }
            }
            has_any_ally = true;
            if DistanceAlg::Chebyshev.distance2d(apt, pp) < self_dist {
                ally_closer = true;
                break;
            }
        }
        !has_any_ally || ally_closer
    } else {
        false
    };

    WorldState {
        player_visible,
        hostile_nearby,
        hp_low,
        has_escape_route,
        adjacent_to_threat,
        carrying_items,
        at_hoard,
        item_visible,
        adjacent_to_item,
        adjacent_to_chest,
        squad_retreating,
        near_leader,
        self_morale_low,
        can_cast_useful_spell: {
            // Check if entity has any monster ability off cooldown.
            // SummonCapped abilities are only useful when below their cap.
            let abilities = world.get::<crate::game::staves::MonsterAbilities>(entity)
                .map(|ma| ma.0.clone());
            if let Some(abilities) = abilities {
                abilities.iter().any(|a| {
                    if a.current_cooldown > 0 { return false; }
                    match &a.kind {
                        crate::game::staves::MonsterAbilityKind::SummonCapped { max_summons, .. } => {
                            crate::game::magic::count_active_summons(entity, world) < *max_summons
                        }
                        _ => true,
                    }
                })
            } else {
                false
            }
        },
        ally_between_self_and_threat,
    }
}

fn dispatch_action(entity: Entity, action_name: &str, ai: &mut GoapAI, world: &mut World) {
    let pos = world.get::<Position>(entity).map(|p| p.to_point()).unwrap_or(Point::new(0, 0));

    // Any non-roam action clears the roam target.
    if action_name != "roam" {
        ai.roam_target = None;
    }

    match action_name {
        "flee" => {
            let mut player_query = world.query_filtered::<&Position, With<Player>>();
            let player_pos = player_query.iter(world).next().map(|p| p.to_point());
            if let Some(pp) = player_pos {
                if let Some(intent) = try_flee_movement(entity, pos, pp, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "attack" => {
            // Attack the nearest adjacent hostile (usually the player).
            let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
            let target = player_query.iter(world).next()
                .filter(|(_, pp)| DistanceAlg::Chebyshev.distance2d(pos, pp.to_point()) <= 1.5)
                .map(|(e, _)| e);
            if let Some(target) = target {
                world.write_message(MeleeIntent { attacker: entity, target });
            } else {
                world.write_message(WaitIntent { entity });
            }
        }

        "seek_item" => {
            // Pathfind toward nearest visible floor item or chest.
            let viewshed = world.get::<Viewshed>(entity).cloned();
            let target = find_nearest_loot(entity, pos, ai.hoard_position, viewshed.as_ref(), world);
            if let Some(target_pt) = target {
                if let Some(intent) = pathfind_toward(entity, pos, target_pt, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "pick_up_item" => {
            world.write_message(PickUpIntent { entity });
        }

        "open_chest" => {
            // Find the nearest adjacent chest and emit OpenChestIntent.
            let chest_entity = {
                let mut chest_query = world.query_filtered::<(Entity, &Position), With<Chest>>();
                chest_query.iter(world)
                    .filter(|(_, cp)| DistanceAlg::Chebyshev.distance2d(pos, cp.to_point()) <= 1.5)
                    .min_by_key(|(_, cp)| {
                        let d = DistanceAlg::Pythagoras.distance2d(pos, cp.to_point());
                        (d * 100.0) as i32
                    })
                    .map(|(e, _)| e)
            };
            if let Some(chest) = chest_entity {
                world.write_message(OpenChestIntent { entity, chest_entity: chest });
            } else {
                world.write_message(WaitIntent { entity });
            }
        }

        "return_to_hoard" => {
            if let Some(hoard_pos) = ai.hoard_position {
                if let Some(intent) = pathfind_toward(entity, pos, hoard_pos, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "drop_items" => {
            world.write_message(DropAtHoardMessage { entity });
        }

        // --- Goblin squad actions ---

        "attack_melee" | "attack" => {
            let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
            let target = player_query.iter(world).next()
                .filter(|(_, pp)| DistanceAlg::Chebyshev.distance2d(pos, pp.to_point()) <= 1.5)
                .map(|(e, _)| e);
            if let Some(target) = target {
                world.write_message(MeleeIntent { attacker: entity, target });
            } else {
                world.write_message(WaitIntent { entity });
            }
        }

        "engage_enemy" => {
            // Pathfind toward the player.
            let mut player_query = world.query_filtered::<&Position, With<Player>>();
            let target = player_query.iter(world).next().map(|p| p.to_point());
            if let Some(target_pt) = target {
                if let Some(intent) = pathfind_toward(entity, pos, target_pt, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "move_to_leader" | "command_position" => {
            // Pathfind toward the squad leader.
            use crate::game::squad::{SquadId, SquadLeader};
            let leader_pos = world.get::<SquadId>(entity).copied().and_then(|sid| {
                let mut q = world.query_filtered::<(&SquadId, &Position), With<SquadLeader>>();
                q.iter(world)
                    .find(|(lsid, _)| **lsid == sid)
                    .map(|(_, p)| p.to_point())
            });
            if let Some(lp) = leader_pos {
                if let Some(intent) = pathfind_toward(entity, pos, lp, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "retreat_to_fallback" => {
            // Pathfind toward the squad's fallback point.
            use crate::game::squad::{SquadBlackboard, SquadId, SquadLeader};
            let fallback = world.get::<SquadId>(entity).copied().and_then(|sid| {
                let mut q = world.query_filtered::<(&SquadId, &SquadBlackboard), With<SquadLeader>>();
                q.iter(world)
                    .find(|(lsid, _)| **lsid == sid)
                    .and_then(|(_, bb)| bb.fallback_point)
            });
            if let Some(fb) = fallback {
                if let Some(intent) = pathfind_toward(entity, pos, fb, world) {
                    world.write_message(intent);
                    return;
                }
            }
            // No fallback point — just flee from player instead.
            let mut player_query = world.query_filtered::<&Position, With<Player>>();
            if let Some(pp) = player_query.iter(world).next().map(|p| p.to_point()) {
                if let Some(intent) = try_flee_movement(entity, pos, pp, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "reposition_behind_ally" => {
            // Move to a tile where an ally is between us and the player.
            let mut player_query = world.query_filtered::<&Position, With<Player>>();
            let player_pos = player_query.iter(world).next().map(|p| p.to_point());
            if let Some(pp) = player_pos {
                // Flee from player — this naturally puts us behind allies.
                if let Some(intent) = try_flee_movement(entity, pos, pp, world) {
                    world.write_message(intent);
                    return;
                }
            }
            world.write_message(WaitIntent { entity });
        }

        "ranged_attack" => {
            // Fire at the player via RangedAttackIntent.
            use crate::game::actions::RangedAttackIntent;
            let mut player_query = world.query_filtered::<(Entity, &Position), With<Player>>();
            let target = player_query.iter(world).next()
                .map(|(e, _)| e);
            if let Some(target) = target {
                world.write_message(RangedAttackIntent { attacker: entity, target });
            } else {
                world.write_message(WaitIntent { entity });
            }
        }

        "cast_spell" => {
            // Delegate to the monster ability system.
            use crate::game::ai::try_use_ability_world;
            if !try_use_ability_world(entity, world) {
                world.write_message(WaitIntent { entity });
            }
        }

        "order_retreat" => {
            // Commander orders retreat — set retreat_ordered on the blackboard.
            use crate::game::squad::{SquadBlackboard, SquadId, SquadLeader};
            let sid = world.get::<SquadId>(entity).copied();
            let spawn_pos = world.get::<crate::game::MonsterAI>(entity)
                .and_then(|ai| ai.spawn_position);
            if let Some(sid) = sid {
                let mut q = world.query_filtered::<(&SquadId, &mut SquadBlackboard), With<SquadLeader>>();
                for (lsid, mut bb) in q.iter_mut(world) {
                    if *lsid == sid {
                        bb.retreat_ordered = true;
                        if bb.fallback_point.is_none() {
                            bb.fallback_point = spawn_pos;
                        }
                        break;
                    }
                }
            }
            world.write_message(WaitIntent { entity });
        }

        _ => {
            // "roam" — pathfind to a random reachable tile. Pick a new target
            // when we don't have one or we've reached the current one.
            if ai.roam_target.is_none() || ai.roam_target == Some(pos) {
                let old = ai.roam_target;
                ai.roam_target = pick_random_walkable_tile_near(pos, world);
                let name = world.get::<crate::components::Name>(entity)
                    .map(|n| n.0.clone()).unwrap_or_default();
                bevy::log::info!(
                    "ROAM {} {entity:?}: pos=({},{}) old_target={:?} new_target={:?}",
                    name, pos.x, pos.y, old, ai.roam_target
                );
            }

            if let Some(target) = ai.roam_target {
                if let Some(intent) = pathfind_toward(entity, pos, target, world) {
                    world.write_message(intent);
                    return;
                }
                // Pathfinding failed — pick a new target next turn.
                ai.roam_target = None;
            }
            world.write_message(WaitIntent { entity });
        }
    }
}

/// Pick a random walkable tile near a position for roaming.
/// Searches within ROAM_RADIUS tiles to keep monsters in their local area.
const ROAM_RADIUS: i32 = 12;

fn pick_random_walkable_tile_near(pos: Point, world: &mut World) -> Option<Point> {
    use crate::map::tile::is_walkable;
    let map = world.resource::<Map>();
    let w = map.width();
    let h = map.height();
    let tiles = &map.tiles;

    // Collect walkable positions within ROAM_RADIUS of the current position.
    let walkable: Vec<Point> = ((pos.x - ROAM_RADIUS).max(0)..=(pos.x + ROAM_RADIUS).min(w - 1))
        .flat_map(|x| ((pos.y - ROAM_RADIUS).max(0)..=(pos.y + ROAM_RADIUS).min(h - 1)).map(move |y| (x, y)))
        .filter(|&(x, y)| is_walkable(tiles[(y * w + x) as usize]))
        .map(|(x, y)| Point::new(x, y))
        .collect();

    if walkable.is_empty() {
        return None;
    }
    let mut rng = rand::rng();
    use rand::Rng;
    let idx = rng.random_range(0..walkable.len());
    Some(walkable[idx])
}

/// Find the nearest visible floor item or chest, excluding items at the hoard position.
fn find_nearest_loot(
    _entity: Entity,
    pos: Point,
    hoard_position: Option<Point>,
    viewshed: Option<&Viewshed>,
    world: &mut World,
) -> Option<Point> {
    let vt = viewshed.map(|v| &v.visible_tiles)?;
    let mut best: Option<(Point, f32)> = None;

    let mut item_query = world.query_filtered::<&Position, (With<Item>, Without<InInventory>)>();
    for item_pos in item_query.iter(world) {
        let ipt = item_pos.to_point();
        if hoard_position.is_some_and(|hp| ipt == hp) { continue; }
        if vt.contains(&ipt) {
            let dist = DistanceAlg::Pythagoras.distance2d(pos, ipt);
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((ipt, dist));
            }
        }
    }

    let mut chest_query = world.query_filtered::<&Position, With<Chest>>();
    for chest_pos in chest_query.iter(world) {
        let cpt = chest_pos.to_point();
        if vt.contains(&cpt) {
            let dist = DistanceAlg::Pythagoras.distance2d(pos, cpt);
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((cpt, dist));
            }
        }
    }

    best.map(|(pt, _)| pt)
}

/// System that handles `DropAtHoardMessage`: drops all inventory items at the entity's position.
pub fn handle_drop_at_hoard(
    mut commands: Commands,
    mut messages: MessageReader<DropAtHoardMessage>,
    mut finish_writer: MessageWriter<ActionFinishedEvent>,
    mut inv_query: Query<(&Position, &mut Inventory)>,
) {
    for msg in messages.read() {
        let Ok((pos, mut inv)) = inv_query.get_mut(msg.entity) else { continue; };
        let drop_pos = *pos;
        for item_entity in inv.items.drain(..) {
            commands.entity(item_entity)
                .remove::<InInventory>()
                .insert(Position { x: drop_pos.x, y: drop_pos.y })
                .insert(Visibility::Inherited)
                .insert(FloorEntityMarker);
        }
        crate::game::actions::finish_turn(&mut commands, &mut finish_writer, msg.entity, crate::constants::BASE_ACTION_COST, crate::game::actions::ActionKind::Movement);
    }
}

// =====================================================================
// Plugin
// =====================================================================

pub struct GoapPlugin;

impl Plugin for GoapPlugin {
    fn build(&self, app: &mut App) {
        use crate::game::turns::ProcessingPhase;
        app.add_message::<DropAtHoardMessage>()
            .add_systems(
                Update,
                handle_drop_at_hoard.in_set(ProcessingPhase::ResolveActions),
            );
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn run_plan(state: &WorldState) -> Option<&'static str> {
        let goals = kobold_hoarder_goals();
        let actions = kobold_hoarder_actions();
        plan(state, &goals, &actions).map(|a| a.name)
    }

    #[test]
    fn flee_when_threatened_with_escape() {
        let state = WorldState {
            adjacent_to_threat: true,
            has_escape_route: true,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("flee"));
    }

    #[test]
    fn attack_when_cornered() {
        let state = WorldState {
            adjacent_to_threat: true,
            has_escape_route: false,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("attack"));
    }

    #[test]
    fn seek_item_when_visible_and_safe() {
        let state = WorldState {
            item_visible: true,
            adjacent_to_item: false,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("seek_item"));
    }

    #[test]
    fn pick_up_when_adjacent_to_item() {
        let state = WorldState {
            adjacent_to_item: true,
            item_visible: true,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("pick_up_item"));
    }

    #[test]
    fn return_to_hoard_when_carrying() {
        let state = WorldState {
            carrying_items: true,
            at_hoard: false,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("return_to_hoard"));
    }

    #[test]
    fn drop_items_at_hoard() {
        let state = WorldState {
            carrying_items: true,
            at_hoard: true,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("drop_items"));
    }

    #[test]
    fn idle_when_nothing_to_do() {
        // No threats, no items, not carrying — roam doesn't advance any goal,
        // so planner returns None. The dispatch layer treats None as "wait"
        // (hoarders roam instead).
        assert_eq!(run_plan(&WorldState::default()), None);
    }

    #[test]
    fn survive_overrides_hoard_when_threat_appears() {
        let state = WorldState {
            adjacent_to_threat: true,
            has_escape_route: true,
            carrying_items: true,
            item_visible: true,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("flee"));
    }

    #[test]
    fn multi_step_plan_returns_first_action() {
        let state = WorldState {
            item_visible: true,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("seek_item"));
    }

    #[test]
    fn full_hoard_cycle_produces_return() {
        let state = WorldState {
            carrying_items: true,
            at_hoard: false,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("return_to_hoard"));
    }

    #[test]
    fn after_chest_open_seeks_spawned_items() {
        // After opening a chest, items are on the floor nearby (visible but not adjacent).
        let state = WorldState {
            item_visible: true,
            adjacent_to_item: false,
            adjacent_to_chest: false, // Chest was despawned.
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("seek_item"));
    }

    #[test]
    fn open_chest_when_adjacent() {
        let state = WorldState {
            item_visible: true,
            adjacent_to_chest: true,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), Some("open_chest"));
    }

    #[test]
    fn no_feedback_loop_at_hoard() {
        let state = WorldState {
            at_hoard: true,
            item_visible: false,
            ..Default::default()
        };
        assert_eq!(run_plan(&state), None);
    }

    // --- Goblin Grunt Tests ---

    fn run_goblin_grunt(state: &WorldState) -> Option<&'static str> {
        let goals = goblin_grunt_goals();
        let actions = goblin_grunt_actions();
        plan(state, &goals, &actions).map(|a| a.name)
    }

    #[test]
    fn grunt_flees_when_threatened_with_escape() {
        let state = WorldState {
            adjacent_to_threat: true,
            has_escape_route: true,
            ..Default::default()
        };
        assert_eq!(run_goblin_grunt(&state), Some("flee"));
    }

    #[test]
    fn grunt_attacks_when_adjacent() {
        let state = WorldState {
            adjacent_to_threat: true,
            has_escape_route: false,
            ..Default::default()
        };
        assert_eq!(run_goblin_grunt(&state), Some("attack_melee"));
    }

    #[test]
    fn grunt_retreats_when_squad_retreating() {
        let state = WorldState {
            squad_retreating: true,
            ..Default::default()
        };
        assert_eq!(run_goblin_grunt(&state), Some("retreat_to_fallback"));
    }

    #[test]
    fn grunt_moves_to_leader_when_far() {
        let state = WorldState {
            near_leader: false,
            ..Default::default()
        };
        assert_eq!(run_goblin_grunt(&state), Some("move_to_leader"));
    }

    #[test]
    fn grunt_engages_when_player_visible() {
        let state = WorldState {
            player_visible: true,
            near_leader: true,
            ..Default::default()
        };
        assert_eq!(run_goblin_grunt(&state), Some("engage_enemy"));
    }

    // --- Goblin Archer Tests ---

    fn run_goblin_archer(state: &WorldState) -> Option<&'static str> {
        let goals = goblin_archer_goals();
        let actions = goblin_archer_actions();
        plan(state, &goals, &actions).map(|a| a.name)
    }

    #[test]
    fn archer_flees_when_threatened() {
        let state = WorldState {
            adjacent_to_threat: true,
            has_escape_route: true,
            ..Default::default()
        };
        assert_eq!(run_goblin_archer(&state), Some("flee"));
    }

    #[test]
    fn archer_safe_behind_allies_all_goals_met() {
        // When behind allies and not threatened, all goals are satisfied.
        // The execute_goap fallback dispatches ranged_attack (not tested here,
        // that's in the dispatch layer). Planner returns None.
        let state = WorldState {
            player_visible: true,
            ally_between_self_and_threat: true,
            ..Default::default()
        };
        assert_eq!(run_goblin_archer(&state), None);
    }

    #[test]
    fn archer_repositions_when_exposed() {
        let state = WorldState {
            player_visible: true,
            ally_between_self_and_threat: false,
            ..Default::default()
        };
        assert_eq!(run_goblin_archer(&state), Some("reposition_behind_ally"));
    }

    // --- Goblin Commander Tests ---

    fn run_commander(state: &WorldState) -> Option<&'static str> {
        let goals = goblin_commander_goals();
        let actions = goblin_commander_actions();
        plan(state, &goals, &actions).map(|a| a.name)
    }

    #[test]
    fn commander_orders_retreat_when_morale_low() {
        let state = WorldState {
            self_morale_low: true,
            ..Default::default()
        };
        assert_eq!(run_commander(&state), Some("order_retreat"));
    }

    #[test]
    fn commander_casts_when_able() {
        let state = WorldState {
            can_cast_useful_spell: true,
            ally_between_self_and_threat: true,
            ..Default::default()
        };
        assert_eq!(run_commander(&state), Some("cast_spell"));
    }

    #[test]
    fn commander_stays_behind_line() {
        let state = WorldState {
            ally_between_self_and_threat: false,
            ..Default::default()
        };
        assert_eq!(run_commander(&state), Some("command_position"));
    }

    // --- Goblin Support (Shaman) Tests ---

    fn run_support(state: &WorldState) -> Option<&'static str> {
        let goals = goblin_support_goals();
        let actions = goblin_support_actions();
        plan(state, &goals, &actions).map(|a| a.name)
    }

    #[test]
    fn support_casts_when_able() {
        let state = WorldState {
            can_cast_useful_spell: true,
            ally_between_self_and_threat: true,
            ..Default::default()
        };
        assert_eq!(run_support(&state), Some("cast_spell"));
    }

    #[test]
    fn support_repositions_when_exposed() {
        let state = WorldState {
            ally_between_self_and_threat: false,
            ..Default::default()
        };
        assert_eq!(run_support(&state), Some("reposition_behind_ally"));
    }

    #[test]
    fn support_casts_over_reposition_when_no_allies() {
        // When ally_between_self_and_threat is vacuously true (no allies),
        // support should cast spells rather than trying to reposition.
        let state = WorldState {
            can_cast_useful_spell: true,
            ally_between_self_and_threat: true, // vacuously true — no allies exist
            ..Default::default()
        };
        assert_eq!(run_support(&state), Some("cast_spell"));
    }

    // Custom `WorldStateProp` extensibility tests moved to
    // `roguelike_engine::ai::goap::tests`.
}
