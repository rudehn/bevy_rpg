// =====================================================================
// Trait-based GOAP Configuration Builder
// =====================================================================

use crate::assets::AiTrait;
use super::planner::{Goal, ActionDef, WorldStateProp};

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
        // Cowardly monsters always flee when adjacent to a threat.
        actions.push(ActionDef {
            name: "flee", cost: 1,
            preconditions: vec![
                (WorldStateProp::AdjacentToThreat, true),
                (WorldStateProp::HasEscapeRoute, true),
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
