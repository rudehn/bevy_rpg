//! MachineBuilder — places interactive machine rooms during map generation.
//!
//! Unlike prefabs (tile grids), machines use feature-based blueprints:
//! a list of features with placement hints that are resolved procedurally
//! within a room.
//!
//! Two coexisting systems:
//! - **V1** (`MachineBlueprint`): simple feature-only blueprints (Shrine, Trapped Vault)
//! - **V2** (`MachineBlueprintV2`): horde-aware blueprints with gate types, placement
//!   hints, liquid/decoration fills, and sub-machine references.

use bevy::log::info;
use bracket_lib::prelude::{Point, Rect};
use bracket_lib::random::RandomNumberGenerator;

use crate::game::squad::SquadConfig;
use crate::map::builders::{BuilderMap, MetaMapBuilder, SpawnEntry};
use crate::map::tile::{Decoration, LiquidType, TerrainType, is_walkable};

// =====================================================================
// V1 Blueprint Data Model (kept for backwards compatibility)
// =====================================================================

/// Describes where to place a feature within a room.
#[derive(Debug, Clone)]
pub enum FeaturePlacement {
    /// Center of the room.
    Center,
    /// Within 2 tiles of center.
    NearCenter,
    /// Any walkable floor tile in the room.
    RandomFloor,
    /// Adjacent to a wall (for levers, etc.)
    AgainstWall,
}

/// What to place at the resolved position.
#[derive(Debug, Clone)]
pub enum FeatureKind {
    /// Place a prop + machine trigger/effect.
    MachineEntity {
        prop_name: String,
        trigger: crate::game::machines::MachineTrigger,
        effect: crate::game::machines::MachineEffect,
        /// If true, the machine entity is despawned after activation.
        consume_on_use: bool,
    },
    /// Place a regular chest prop (opens normally via chest logic).
    Chest,
    /// Place a candle (light source) at this position.
    Light,
    /// Spread a decoration in a radius around this position.
    Decorate { decoration: Decoration, radius: i32 },
}

/// A single feature within a machine blueprint.
#[derive(Debug, Clone)]
pub struct MachineFeature {
    pub placement: FeaturePlacement,
    pub kind: FeatureKind,
}

/// A machine blueprint — describes what to build, not where. (V1)
pub struct MachineBlueprint {
    pub name: &'static str,
    pub min_floor: i32,
    pub max_floor: i32,
    pub min_room_width: i32,
    pub min_room_height: i32,
    pub frequency: u32,
    pub features: Vec<MachineFeature>,
}

/// Data recorded by the builder for later entity spawning.
#[derive(Debug, Clone)]
pub struct MachineSpawn {
    pub pos: Point,
    pub prop_name: String,
    pub trigger: crate::game::machines::MachineTrigger,
    pub effect: crate::game::machines::MachineEffect,
    pub consume_on_use: bool,
}

// =====================================================================
// Horde System
// =====================================================================

/// Role tag for a horde — determines how it is used within a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HordeTag {
    Swarm,
    Patrol,
    Guard,
    Ranged,
    Support,
    Elite,
    Threat,
    Hazard,
    Brute,
    Guardian,
    Ambush,
    Apex,
}

/// A named group of monsters with a role tag.
struct HordeDef {
    name: &'static str,
    tag: HordeTag,
    /// (monster_name, min_count, max_count)
    monsters: Vec<(&'static str, u32, u32)>,
}

/// Floor-gated horde eligibility.
struct HordeSpawnEntry {
    horde_name: &'static str,
    min_floor: i32,
    max_floor: i32,
}

fn all_horde_defs() -> Vec<HordeDef> {
    // Monster names must match `assets/monsters.ron` keys exactly (PascalCase
    // with spaces) — `spawn_monster_by_name` does not normalize.
    vec![
        HordeDef {
            name: "rat_pack",
            tag: HordeTag::Swarm,
            monsters: vec![("Giant Rat", 2, 4)],
        },
        HordeDef {
            name: "goblin_patrol",
            tag: HordeTag::Patrol,
            monsters: vec![("Goblin", 2, 3)],
        },
        HordeDef {
            name: "goblin_squad",
            tag: HordeTag::Guard,
            monsters: vec![("Goblin", 2, 3), ("Goblin Brute", 1, 1)],
        },
        HordeDef {
            // Kept the original `goblin_archers` horde name for stability,
            // but "Goblin Archer" never existed in the manifest — replaced
            // by the Goblin Conjurer (see `assets/monsters.ron`).
            name: "goblin_archers",
            tag: HordeTag::Ranged,
            monsters: vec![("Goblin Conjurer", 1, 2)],
        },
        HordeDef {
            name: "goblin_casters",
            tag: HordeTag::Support,
            monsters: vec![("Goblin Shaman", 1, 1)],
        },
        HordeDef {
            name: "goblin_war_party",
            tag: HordeTag::Elite,
            monsters: vec![
                ("Goblin Warchief", 1, 1),
                ("Goblin Brute", 1, 2),
            ],
        },
        HordeDef {
            name: "spider_nest",
            tag: HordeTag::Ambush,
            monsters: vec![("Giant Spider", 1, 2)],
        },
        HordeDef {
            name: "wolf_pack",
            tag: HordeTag::Patrol,
            monsters: vec![("Wolf", 2, 3)],
        },
        HordeDef {
            name: "bat_colony",
            tag: HordeTag::Swarm,
            monsters: vec![("Giant Bat", 2, 4)],
        },
        HordeDef {
            name: "salamander_pair",
            tag: HordeTag::Threat,
            monsters: vec![("Fire Salamander", 1, 2)],
        },
        HordeDef {
            name: "jelly_blob",
            tag: HordeTag::Threat,
            monsters: vec![("Jelly", 1, 1)],
        },
        HordeDef {
            name: "bloat_cluster",
            tag: HordeTag::Hazard,
            monsters: vec![("Bloat", 1, 2)],
        },
        HordeDef {
            name: "troll_den",
            tag: HordeTag::Brute,
            monsters: vec![("Cave Troll", 1, 1)],
        },
        HordeDef {
            name: "sentinel_post",
            tag: HordeTag::Guardian,
            monsters: vec![("Stone Sentinel", 1, 1)],
        },
        HordeDef {
            name: "dragon_guard",
            tag: HordeTag::Elite,
            monsters: vec![("Dragon Whelp", 1, 1)],
        },
        HordeDef {
            name: "young_dragon_lair",
            tag: HordeTag::Apex,
            monsters: vec![("Young Dragon", 1, 1)],
        },
        HordeDef {
            name: "skeleton_patrol",
            tag: HordeTag::Guard,
            monsters: vec![("Skeleton", 1, 3)],
        },
        HordeDef {
            name: "bone_crypt_defenders",
            tag: HordeTag::Ranged,
            monsters: vec![("Bone Archer", 1, 2)],
        },
        HordeDef {
            name: "fungal_cluster",
            tag: HordeTag::Hazard,
            monsters: vec![("Fungal Spore", 1, 3)],
        },

        // ==================== T3-T6 hordes (f10-26) ====================
        // Use PascalCase names that match `assets/monsters.ron` keys directly.
        HordeDef {
            name: "goblin_fortress",
            tag: HordeTag::Elite,
            monsters: vec![
                ("Goblin Warchief", 1, 1),
                ("Goblin", 2, 3),
                ("Goblin Firebomber", 0, 1),
            ],
        },
        HordeDef {
            name: "orc_raid",
            tag: HordeTag::Guard,
            monsters: vec![
                ("Orc Warrior", 1, 2),
                ("Orc Archer", 1, 1),
            ],
        },
        HordeDef {
            name: "fungal_infestation",
            tag: HordeTag::Hazard,
            monsters: vec![
                ("Spore Crawler", 1, 1),
                ("Fungal Spore", 2, 3),
            ],
        },
        HordeDef {
            name: "crypt_vanguard",
            tag: HordeTag::Guard,
            monsters: vec![
                ("Skeleton", 2, 3),
                ("Bone Archer", 1, 1),
            ],
        },
        HordeDef {
            name: "wraith_hall",
            tag: HordeTag::Elite,
            monsters: vec![
                ("Wraith", 1, 1),
                ("Zombie", 1, 2),
            ],
        },
        HordeDef {
            name: "necromancer_lair",
            tag: HordeTag::Apex,
            monsters: vec![
                ("Necromancer", 1, 1),
                ("Skeleton", 1, 2),
                ("Zombie", 0, 1),
            ],
        },
        HordeDef {
            name: "giant_lair",
            tag: HordeTag::Brute,
            monsters: vec![
                ("Ogre", 1, 1),
                ("Troll", 0, 1),
            ],
        },
        HordeDef {
            name: "dragon_roost",
            tag: HordeTag::Apex,
            monsters: vec![("Dragon Whelp", 1, 2)],
        },
        HordeDef {
            name: "construct_vault",
            tag: HordeTag::Guardian,
            monsters: vec![("Stone Sentinel", 1, 2)],
        },
        HordeDef {
            name: "dragon_lair",
            tag: HordeTag::Apex,
            monsters: vec![
                ("Young Dragon", 1, 1),
                ("Drake", 0, 1),
            ],
        },
        HordeDef {
            name: "giant_keep",
            tag: HordeTag::Brute,
            monsters: vec![
                ("Hill Giant", 1, 1),
                ("Ogre Mage", 0, 1),
            ],
        },
        HordeDef {
            name: "undead_citadel",
            tag: HordeTag::Apex,
            monsters: vec![
                ("Bone Colossus", 1, 1),
                ("Wraith", 1, 2),
            ],
        },
        HordeDef {
            name: "dragon_flight",
            tag: HordeTag::Apex,
            monsters: vec![
                ("Elder Drake", 1, 1),
                ("Drake", 0, 1),
                ("Wyrm", 0, 1),
            ],
        },
        HordeDef {
            name: "amulet_chamber",
            tag: HordeTag::Apex,
            monsters: vec![
                ("Amulet Guardian", 1, 1),
                ("Stone Sentinel", 1, 2),
            ],
        },
    ]
}

fn all_horde_spawn_entries() -> Vec<HordeSpawnEntry> {
    vec![
        HordeSpawnEntry { horde_name: "rat_pack",          min_floor: 1, max_floor: 4  },
        HordeSpawnEntry { horde_name: "bat_colony",        min_floor: 1, max_floor: 3  },
        HordeSpawnEntry { horde_name: "goblin_patrol",     min_floor: 1, max_floor: 5  },
        HordeSpawnEntry { horde_name: "goblin_archers",    min_floor: 2, max_floor: 7  },
        HordeSpawnEntry { horde_name: "wolf_pack",         min_floor: 2, max_floor: 5  },
        HordeSpawnEntry { horde_name: "salamander_pair",   min_floor: 2, max_floor: 5  },
        HordeSpawnEntry { horde_name: "bloat_cluster",     min_floor: 1, max_floor: 5  },
        HordeSpawnEntry { horde_name: "goblin_squad",      min_floor: 3, max_floor: 7  },
        HordeSpawnEntry { horde_name: "spider_nest",       min_floor: 3, max_floor: 6  },
        HordeSpawnEntry { horde_name: "jelly_blob",        min_floor: 2, max_floor: 7  },
        HordeSpawnEntry { horde_name: "goblin_casters",    min_floor: 4, max_floor: 9  },
        HordeSpawnEntry { horde_name: "troll_den",         min_floor: 4, max_floor: 8  },
        HordeSpawnEntry { horde_name: "sentinel_post",     min_floor: 2, max_floor: 26 },
        HordeSpawnEntry { horde_name: "dragon_guard",      min_floor: 5, max_floor: 9  },
        HordeSpawnEntry { horde_name: "goblin_war_party",  min_floor: 7, max_floor: 9  },
        HordeSpawnEntry { horde_name: "young_dragon_lair", min_floor: 8, max_floor: 9  },
        HordeSpawnEntry { horde_name: "skeleton_patrol",   min_floor: 4, max_floor: 8  },
        HordeSpawnEntry { horde_name: "bone_crypt_defenders", min_floor: 5, max_floor: 9 },
        HordeSpawnEntry { horde_name: "fungal_cluster",    min_floor: 3, max_floor: 8  },

        // T3-T6 (f10-26)
        HordeSpawnEntry { horde_name: "goblin_fortress",    min_floor: 10, max_floor: 14 },
        HordeSpawnEntry { horde_name: "orc_raid",           min_floor: 10, max_floor: 17 },
        HordeSpawnEntry { horde_name: "fungal_infestation", min_floor: 10, max_floor: 16 },
        HordeSpawnEntry { horde_name: "crypt_vanguard",     min_floor: 12, max_floor: 17 },
        HordeSpawnEntry { horde_name: "wraith_hall",        min_floor: 15, max_floor: 20 },
        HordeSpawnEntry { horde_name: "necromancer_lair",   min_floor: 17, max_floor: 22 },
        HordeSpawnEntry { horde_name: "giant_lair",         min_floor: 15, max_floor: 22 },
        HordeSpawnEntry { horde_name: "dragon_roost",       min_floor: 16, max_floor: 22 },
        HordeSpawnEntry { horde_name: "construct_vault",    min_floor: 20, max_floor: 26 },
        HordeSpawnEntry { horde_name: "dragon_lair",        min_floor: 20, max_floor: 25 },
        HordeSpawnEntry { horde_name: "giant_keep",         min_floor: 21, max_floor: 25 },
        HordeSpawnEntry { horde_name: "undead_citadel",     min_floor: 21, max_floor: 25 },
        HordeSpawnEntry { horde_name: "dragon_flight",      min_floor: 23, max_floor: 26 },
        HordeSpawnEntry { horde_name: "amulet_chamber",     min_floor: 26, max_floor: 26 },
    ]
}

// =====================================================================
// Tag Resolution
// =====================================================================

/// Resolve a horde by tag for the given floor. Returns a list of
/// (monster_name, count) pairs, or `None` if no eligible horde exists.
fn resolve_horde_by_tag(
    tag: HordeTag,
    floor: i32,
    rng: &mut RandomNumberGenerator,
) -> Option<Vec<(String, u32)>> {
    let defs = all_horde_defs();
    let entries = all_horde_spawn_entries();

    // Filter spawn entries for this floor
    let eligible_names: Vec<&str> = entries
        .iter()
        .filter(|e| floor >= e.min_floor && floor <= e.max_floor)
        .map(|e| e.horde_name)
        .collect();

    // Filter horde defs by tag AND floor eligibility
    let candidates: Vec<&HordeDef> = defs
        .iter()
        .filter(|d| d.tag == tag && eligible_names.contains(&d.name))
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Pick one randomly
    let idx = rng.range(0, candidates.len() as i32) as usize;
    let chosen = &candidates[idx];

    // Expand to concrete counts
    let mut result = Vec::new();
    for &(name, min, max) in &chosen.monsters {
        let count = if min == max {
            min
        } else {
            rng.range(min as i32, max as i32 + 1) as u32
        };
        if count > 0 {
            result.push((name.to_string(), count));
        }
    }

    info!(
        "Horde tag {:?} floor {} -> '{}' ({:?})",
        tag, floor, chosen.name, result
    );

    Some(result)
}

// =====================================================================
// V2 Blueprint Data Model
// =====================================================================

/// Gate type for the machine entrance.
#[derive(Debug, Clone)]
enum GateType {
    /// Normal door at the chokepoint.
    Open,
    /// Monster placed at the gate (no door change).
    Guardian { tag: HordeTag },
}

/// Interior preparation before placing features.
#[derive(Debug, Clone)]
enum InteriorPrep {
    None,
    /// Clear all terrain to plain floor.
    Purge,
    /// Remove isolated wall pillars (walls with 6+ floor neighbors).
    Open,
}

/// Where to place a horde within the machine interior.
#[derive(Debug, Clone, Copy)]
enum PlacementHint {
    AtGate,
    NearGate,
    Center,
    DeepInterior,
    AlongWalls,
    Random,
}

/// Liquid or decoration fill for the machine interior.
#[derive(Debug, Clone)]
enum InteriorFill {
    None,
    Liquid(LiquidType),
    DecorationFill(Decoration),
}

/// A horde slot in a machine blueprint.
struct HordeSlot {
    tag: HordeTag,
    hint: PlacementHint,
}

/// A V2 feature placed by a machine (props, chests, lights).
struct V2Feature {
    hint: PlacementHint,
    kind: V2FeatureKind,
}

#[derive(Debug, Clone)]
enum V2FeatureKind {
    Chest,
    Prop { name: &'static str },
    Light,
    /// A machine entity (altar, shrine, etc.) with trigger + effect.
    MachineEntity {
        prop_name: &'static str,
        trigger: MachineTrigger,
        effect: MachineEffect,
        consume_on_use: bool,
    },
    /// Place a named monster directly at this position.
    MonsterSpawn { monster_name: String },
}

/// Extended machine blueprint with horde slots and gate types.
struct MachineBlueprintV2 {
    name: &'static str,
    min_floor: i32,
    max_floor: i32,
    /// Interior tile count range (room area).
    min_interior: i32,
    max_interior: i32,
    gate: GateType,
    prep: InteriorPrep,
    fill: InteriorFill,
    horde_slots: Vec<HordeSlot>,
    features: Vec<V2Feature>,
    /// Machine budget weight.
    frequency: u32,
}

// =====================================================================
// V1 Blueprint Catalog (unchanged)
// =====================================================================

use crate::game::machines::{MachineEffect, MachineTrigger};

fn shrine_blueprint() -> MachineBlueprint {
    MachineBlueprint {
        name: "Shrine",
        min_floor: 1,
        max_floor: 20,
        min_room_width: 5,
        min_room_height: 5,
        frequency: 3,
        features: vec![
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::MachineEntity {
                    prop_name: "altar".to_string(),
                    trigger: MachineTrigger::BumpActivate,
                    effect: MachineEffect::Multi(vec![
                        MachineEffect::HealFull,
                        MachineEffect::SpawnItem {
                            item_name: "Scroll of Enchanting".to_string(),
                        },
                    ]),
                    consume_on_use: false,
                },
            },
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::Decorate {
                    decoration: Decoration::Moss,
                    radius: 2,
                },
            },
        ],
    }
}

fn trapped_vault_blueprint() -> MachineBlueprint {
    MachineBlueprint {
        name: "Trapped Vault",
        min_floor: 3,
        max_floor: 20,
        min_room_width: 5,
        min_room_height: 5,
        frequency: 2,
        features: vec![
            // Normal chest — opens via regular chest logic, spawns loot
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::Chest,
            },
            // Hidden step-trigger at same position — when player walks onto the
            // tile (after chest despawns and they pick up loot), monsters ambush
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::MachineEntity {
                    prop_name: "".to_string(), // invisible trigger, no prop
                    trigger: MachineTrigger::StepActivate,
                    effect: MachineEffect::SpawnMonsters {
                        monster_name: String::new(), // empty = pick level-appropriate monsters
                        count: 2,
                    },
                    consume_on_use: true,
                },
            },
            MachineFeature {
                placement: FeaturePlacement::Center,
                kind: FeatureKind::Decorate {
                    decoration: Decoration::Cobweb,
                    radius: 2,
                },
            },
        ],
    }
}

fn all_v1_blueprints() -> Vec<MachineBlueprint> {
    vec![shrine_blueprint(), trapped_vault_blueprint()]
}

// =====================================================================
// V2 Blueprint Catalog
// =====================================================================

fn all_v2_blueprints() -> Vec<MachineBlueprintV2> {
    vec![
        // --- Goblin Encounters ---
        MachineBlueprintV2 {
            name: "Goblin Scuffle",
            min_floor: 1,
            max_floor: 3,
            min_interior: 8,
            max_interior: 20,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Patrol,
                hint: PlacementHint::NearGate,
            }],
            features: vec![V2Feature {
                hint: PlacementHint::DeepInterior,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 3,
        },
        MachineBlueprintV2 {
            name: "Goblin Camp",
            min_floor: 2,
            max_floor: 5,
            min_interior: 12,
            max_interior: 35,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![
                HordeSlot {
                    tag: HordeTag::Guard,
                    hint: PlacementHint::NearGate,
                },
                HordeSlot {
                    tag: HordeTag::Ranged,
                    hint: PlacementHint::AlongWalls,
                },
            ],
            features: vec![
                V2Feature {
                    hint: PlacementHint::Center,
                    kind: V2FeatureKind::Prop { name: "watchfire" },
                },
                V2Feature {
                    hint: PlacementHint::NearGate,
                    kind: V2FeatureKind::Prop { name: "barricade" },
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 3,
        },
        MachineBlueprintV2 {
            name: "Goblin Outpost",
            min_floor: 4,
            max_floor: 7,
            min_interior: 20,
            max_interior: 50,
            gate: GateType::Open,
            prep: InteriorPrep::Open,
            fill: InteriorFill::None,
            horde_slots: vec![
                HordeSlot {
                    tag: HordeTag::Guard,
                    hint: PlacementHint::NearGate,
                },
                HordeSlot {
                    tag: HordeTag::Ranged,
                    hint: PlacementHint::AlongWalls,
                },
                HordeSlot {
                    tag: HordeTag::Support,
                    hint: PlacementHint::Center,
                },
            ],
            features: vec![
                V2Feature {
                    hint: PlacementHint::Center,
                    kind: V2FeatureKind::Prop { name: "watchfire" },
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Goblin Fort",
            min_floor: 7,
            max_floor: 9,
            min_interior: 35,
            max_interior: 70,
            gate: GateType::Open, // Simplified from Locked for POC
            prep: InteriorPrep::Open,
            fill: InteriorFill::None,
            horde_slots: vec![
                HordeSlot {
                    tag: HordeTag::Elite,
                    hint: PlacementHint::Center,
                },
                HordeSlot {
                    tag: HordeTag::Guard,
                    hint: PlacementHint::NearGate,
                },
                HordeSlot {
                    tag: HordeTag::Ranged,
                    hint: PlacementHint::AlongWalls,
                },
                HordeSlot {
                    tag: HordeTag::Support,
                    hint: PlacementHint::DeepInterior,
                },
            ],
            features: vec![
                V2Feature {
                    hint: PlacementHint::Center,
                    kind: V2FeatureKind::Prop { name: "watchfire" },
                },
                V2Feature {
                    hint: PlacementHint::NearGate,
                    kind: V2FeatureKind::Prop { name: "barricade" },
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 1,
        },
        // --- Reward Machines ---
        MachineBlueprintV2 {
            name: "Treasure Vault",
            min_floor: 2,
            max_floor: 26,
            min_interior: 8,
            max_interior: 40,
            gate: GateType::Open, // Simplified from Locked for POC
            prep: InteriorPrep::Purge,
            fill: InteriorFill::None,
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Guard,
                hint: PlacementHint::Center,
            }],
            features: vec![
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Guardian Corridor",
            min_floor: 3,
            max_floor: 26,
            min_interior: 5,
            max_interior: 15,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Brute,
                hint: PlacementHint::AtGate,
            }],
            features: vec![V2Feature {
                hint: PlacementHint::DeepInterior,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 2,
        },
        // --- Environmental ---
        MachineBlueprintV2 {
            name: "Flooded Chamber",
            min_floor: 3,
            max_floor: 8,
            min_interior: 10,
            max_interior: 50,
            gate: GateType::Open,
            prep: InteriorPrep::Purge,
            fill: InteriorFill::Liquid(LiquidType::ShallowWater),
            horde_slots: vec![],
            features: vec![V2Feature {
                hint: PlacementHint::DeepInterior,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Fungal Grotto",
            min_floor: 3,
            max_floor: 9,
            min_interior: 10,
            max_interior: 40,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::DecorationFill(Decoration::Fungus),
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Threat,
                hint: PlacementHint::Random,
            }],
            features: vec![],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Bone Crypt",
            min_floor: 4,
            max_floor: 26,
            min_interior: 10,
            max_interior: 40,
            gate: GateType::Open, // Simplified from Locked for POC
            prep: InteriorPrep::Purge,
            fill: InteriorFill::DecorationFill(Decoration::Bloodstain),
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Guard,
                hint: PlacementHint::NearGate,
            }],
            features: vec![V2Feature {
                hint: PlacementHint::DeepInterior,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Lava Vault",
            min_floor: 10,
            max_floor: 26,
            min_interior: 10,
            max_interior: 40,
            gate: GateType::Open, // Simplified from Locked for POC
            prep: InteriorPrep::Purge,
            fill: InteriorFill::Liquid(LiquidType::Lava),
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Brute,
                hint: PlacementHint::NearGate,
            }],
            features: vec![
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 1,
        },
        // --- Hazard ---
        MachineBlueprintV2 {
            name: "Monster Den",
            min_floor: 2,
            max_floor: 9,
            min_interior: 15,
            max_interior: 60,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![
                HordeSlot {
                    tag: HordeTag::Threat,
                    hint: PlacementHint::Random,
                },
                HordeSlot {
                    tag: HordeTag::Swarm,
                    hint: PlacementHint::Random,
                },
            ],
            features: vec![
                V2Feature {
                    hint: PlacementHint::Center,
                    kind: V2FeatureKind::Prop { name: "watchfire" },
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 3,
        },
        MachineBlueprintV2 {
            name: "Ambush Room",
            min_floor: 3,
            max_floor: 8,
            min_interior: 8,
            max_interior: 25,
            gate: GateType::Guardian {
                tag: HordeTag::Ambush,
            },
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Ambush,
                hint: PlacementHint::AlongWalls,
            }],
            features: vec![V2Feature {
                hint: PlacementHint::Center,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 2,
        },
        // --- Trapped Chest Machine ---
        MachineBlueprintV2 {
            name: "Trapped Chest",
            min_floor: 2,
            max_floor: 26,
            min_interior: 6,
            max_interior: 30,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![],
            features: vec![V2Feature {
                hint: PlacementHint::Center,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 2,
        },
        // --- Simple Filler Machines ---
        MachineBlueprintV2 {
            name: "Lone Chest",
            min_floor: 1,
            max_floor: 26,
            min_interior: 4,
            max_interior: 999,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![],
            features: vec![V2Feature {
                hint: PlacementHint::Center,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 5,
        },
        MachineBlueprintV2 {
            name: "Sleeping Den",
            min_floor: 1,
            max_floor: 8,
            min_interior: 6,
            max_interior: 30,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Swarm,
                hint: PlacementHint::Random,
            }],
            features: vec![V2Feature {
                hint: PlacementHint::DeepInterior,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 4,
        },
        MachineBlueprintV2 {
            name: "Bone Pile",
            min_floor: 4,
            max_floor: 26,
            min_interior: 4,
            max_interior: 20,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::DecorationFill(Decoration::Bloodstain),
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Guard,
                hint: PlacementHint::Center,
            }],
            features: vec![],
            frequency: 3,
        },
        MachineBlueprintV2 {
            name: "Abandoned Camp",
            min_floor: 2,
            max_floor: 8,
            min_interior: 6,
            max_interior: 30,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::DecorationFill(Decoration::Rubble),
            horde_slots: vec![],
            features: vec![
                V2Feature {
                    hint: PlacementHint::Center,
                    kind: V2FeatureKind::Prop { name: "watchfire" },
                },
                V2Feature {
                    hint: PlacementHint::Random,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 4,
        },
        MachineBlueprintV2 {
            name: "Fungal Patch",
            min_floor: 3,
            max_floor: 8,
            min_interior: 4,
            max_interior: 25,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::DecorationFill(Decoration::Fungus),
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Hazard,
                hint: PlacementHint::Random,
            }],
            features: vec![],
            frequency: 3,
        },
        MachineBlueprintV2 {
            name: "Arrow Trap Corridor",
            min_floor: 3,
            max_floor: 26,
            min_interior: 5,
            max_interior: 20,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Guardian,
                hint: PlacementHint::AtGate,
            }],
            features: vec![V2Feature {
                hint: PlacementHint::DeepInterior,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Guarded Shrine",
            min_floor: 2,
            max_floor: 26,
            min_interior: 6,
            max_interior: 25,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Guard,
                hint: PlacementHint::NearGate,
            }],
            features: vec![V2Feature {
                hint: PlacementHint::Center,
                kind: V2FeatureKind::MachineEntity {
                    prop_name: "altar",
                    trigger: MachineTrigger::BumpActivate,
                    effect: MachineEffect::Multi(vec![
                        MachineEffect::HealFull,
                        MachineEffect::SpawnItem {
                            item_name: "Scroll of Enchanting".to_string(),
                        },
                    ]),
                    consume_on_use: false,
                },
            }],
            frequency: 2,
        },
        // =============================================================
        // Signature Encounters — rare, impactful encounters
        // =============================================================
        MachineBlueprintV2 {
            name: "Spider's Web",
            min_floor: 3,
            max_floor: 7,
            min_interior: 15,
            max_interior: 40,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::DecorationFill(Decoration::Cobweb),
            horde_slots: vec![
                HordeSlot {
                    tag: HordeTag::Ambush,
                    hint: PlacementHint::AlongWalls,
                },
                HordeSlot {
                    tag: HordeTag::Threat,
                    hint: PlacementHint::AlongWalls,
                },
            ],
            features: vec![V2Feature {
                hint: PlacementHint::Center,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Troll Bridge",
            min_floor: 4,
            max_floor: 8,
            min_interior: 10,
            max_interior: 30,
            gate: GateType::Open,
            prep: InteriorPrep::Purge,
            fill: InteriorFill::Liquid(LiquidType::ShallowWater),
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Brute,
                hint: PlacementHint::Center,
            }],
            features: vec![V2Feature {
                hint: PlacementHint::DeepInterior,
                kind: V2FeatureKind::Chest,
            }],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Jelly Pit",
            min_floor: 3,
            max_floor: 7,
            min_interior: 10,
            max_interior: 30,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Threat,
                hint: PlacementHint::Center,
            }],
            features: vec![
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
                V2Feature {
                    hint: PlacementHint::AlongWalls,
                    kind: V2FeatureKind::Prop { name: "candle" },
                },
                V2Feature {
                    hint: PlacementHint::AlongWalls,
                    kind: V2FeatureKind::Prop { name: "candle" },
                },
                V2Feature {
                    hint: PlacementHint::AlongWalls,
                    kind: V2FeatureKind::Prop { name: "candle" },
                },
            ],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Dragon's Hoard",
            min_floor: 7,
            max_floor: 26,
            min_interior: 25,
            max_interior: 60,
            gate: GateType::Open,
            prep: InteriorPrep::Open,
            fill: InteriorFill::None,
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Apex,
                hint: PlacementHint::Center,
            }],
            features: vec![
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
                V2Feature {
                    hint: PlacementHint::AlongWalls,
                    kind: V2FeatureKind::Chest,
                },
                V2Feature {
                    hint: PlacementHint::Random,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 1,
        },
        MachineBlueprintV2 {
            name: "Mimic Treasury",
            min_floor: 4,
            max_floor: 26,
            min_interior: 12,
            max_interior: 30,
            gate: GateType::Open,
            prep: InteriorPrep::Purge,
            fill: InteriorFill::None,
            horde_slots: vec![],
            features: vec![
                V2Feature {
                    hint: PlacementHint::Center,
                    kind: V2FeatureKind::Chest,
                },
                V2Feature {
                    hint: PlacementHint::AlongWalls,
                    kind: V2FeatureKind::Chest,
                },
                V2Feature {
                    hint: PlacementHint::Random,
                    kind: V2FeatureKind::MonsterSpawn {
                        monster_name: "Mimic".to_string(),
                    },
                },
                V2Feature {
                    hint: PlacementHint::Random,
                    kind: V2FeatureKind::MonsterSpawn {
                        monster_name: "Mimic".to_string(),
                    },
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::MonsterSpawn {
                        monster_name: "Mimic".to_string(),
                    },
                },
            ],
            frequency: 1,
        },
        MachineBlueprintV2 {
            name: "The Gauntlet",
            min_floor: 3,
            max_floor: 26,
            min_interior: 15,
            max_interior: 40,
            gate: GateType::Open,
            prep: InteriorPrep::None,
            fill: InteriorFill::None,
            horde_slots: vec![
                HordeSlot {
                    tag: HordeTag::Guardian,
                    hint: PlacementHint::NearGate,
                },
                HordeSlot {
                    tag: HordeTag::Guardian,
                    hint: PlacementHint::DeepInterior,
                },
            ],
            features: vec![
                V2Feature {
                    hint: PlacementHint::NearGate,
                    kind: V2FeatureKind::Prop { name: "barricade" },
                },
                V2Feature {
                    hint: PlacementHint::Center,
                    kind: V2FeatureKind::Prop { name: "barricade" },
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 2,
        },
        MachineBlueprintV2 {
            name: "Flooded Shrine",
            min_floor: 3,
            max_floor: 8,
            min_interior: 15,
            max_interior: 40,
            gate: GateType::Open,
            prep: InteriorPrep::Purge,
            fill: InteriorFill::Liquid(LiquidType::Water),
            horde_slots: vec![HordeSlot {
                tag: HordeTag::Threat,
                hint: PlacementHint::Random,
            }],
            features: vec![V2Feature {
                hint: PlacementHint::Center,
                kind: V2FeatureKind::MachineEntity {
                    prop_name: "altar",
                    trigger: MachineTrigger::BumpActivate,
                    effect: MachineEffect::Multi(vec![
                        MachineEffect::HealFull,
                        MachineEffect::SpawnItem {
                            item_name: "Scroll of Enchanting".to_string(),
                        },
                    ]),
                    consume_on_use: false,
                },
            }],
            frequency: 1,
        },
        MachineBlueprintV2 {
            name: "Goblin War Room",
            min_floor: 6,
            max_floor: 9,
            min_interior: 30,
            max_interior: 70,
            gate: GateType::Open,
            prep: InteriorPrep::Open,
            fill: InteriorFill::None,
            horde_slots: vec![
                HordeSlot {
                    tag: HordeTag::Elite,
                    hint: PlacementHint::Center,
                },
                HordeSlot {
                    tag: HordeTag::Support,
                    hint: PlacementHint::DeepInterior,
                },
                HordeSlot {
                    tag: HordeTag::Ranged,
                    hint: PlacementHint::AlongWalls,
                },
                HordeSlot {
                    tag: HordeTag::Guard,
                    hint: PlacementHint::NearGate,
                },
            ],
            features: vec![
                V2Feature {
                    hint: PlacementHint::Center,
                    kind: V2FeatureKind::Prop { name: "watchfire" },
                },
                V2Feature {
                    hint: PlacementHint::NearGate,
                    kind: V2FeatureKind::Prop { name: "barricade" },
                },
                V2Feature {
                    hint: PlacementHint::NearGate,
                    kind: V2FeatureKind::Prop { name: "barricade" },
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
                V2Feature {
                    hint: PlacementHint::DeepInterior,
                    kind: V2FeatureKind::Chest,
                },
            ],
            frequency: 1,
        },
    ]
}

// =====================================================================
// Placement Hint Resolution
// =====================================================================

/// Resolve a `PlacementHint` to a concrete position within a room.
fn resolve_hint(
    hint: PlacementHint,
    room: Rect,
    gate_pos: Option<Point>,
    map: &crate::map::map::Map,
    rng: &mut RandomNumberGenerator,
) -> Option<Point> {
    let cx = (room.x1 + room.x2) / 2;
    let cy = (room.y1 + room.y2) / 2;
    let gate = gate_pos.unwrap_or(Point::new(room.x1, cy));

    match hint {
        PlacementHint::AtGate => {
            // At or adjacent to the gate tile
            let idx = map.xy_idx(gate.x, gate.y);
            if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                return Some(gate);
            }
            // Try adjacent
            for (dx, dy) in &[(0, -1), (0, 1), (-1, 0), (1, 0)] {
                let nx = gate.x + dx;
                let ny = gate.y + dy;
                let ni = map.xy_idx(nx, ny);
                if ni < map.tiles.len()
                    && is_walkable(map.tiles[ni])
                    && nx >= room.x1
                    && nx <= room.x2
                    && ny >= room.y1
                    && ny <= room.y2
                {
                    return Some(Point::new(nx, ny));
                }
            }
            None
        }
        PlacementHint::NearGate => {
            for _ in 0..20 {
                let x = gate.x + rng.range(-3, 4);
                let y = gate.y + rng.range(-3, 4);
                if x < room.x1 + 1 || x >= room.x2 || y < room.y1 + 1 || y >= room.y2 {
                    continue;
                }
                let idx = map.xy_idx(x, y);
                if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                    return Some(Point::new(x, y));
                }
            }
            // Fallback to gate
            resolve_hint(PlacementHint::AtGate, room, gate_pos, map, rng)
        }
        PlacementHint::Center => {
            let idx = map.xy_idx(cx, cy);
            if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                return Some(Point::new(cx, cy));
            }
            // Spiral search
            for r in 1..=3 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        let x = cx + dx;
                        let y = cy + dy;
                        let ni = map.xy_idx(x, y);
                        if ni < map.tiles.len() && is_walkable(map.tiles[ni]) {
                            return Some(Point::new(x, y));
                        }
                    }
                }
            }
            None
        }
        PlacementHint::DeepInterior => {
            // Farthest walkable tile from the gate within the room
            let mut best: Option<Point> = None;
            let mut best_dist = -1i32;
            for y in (room.y1 + 1)..room.y2 {
                for x in (room.x1 + 1)..room.x2 {
                    let idx = map.xy_idx(x, y);
                    if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                        let dist = (x - gate.x).abs() + (y - gate.y).abs();
                        if dist > best_dist {
                            best_dist = dist;
                            best = Some(Point::new(x, y));
                        }
                    }
                }
            }
            best
        }
        PlacementHint::AlongWalls => {
            // Find a floor tile adjacent to a wall
            for _ in 0..20 {
                let x = rng.range(room.x1 + 1, room.x2);
                let y = rng.range(room.y1 + 1, room.y2);
                let idx = map.xy_idx(x, y);
                if idx >= map.tiles.len() || !is_walkable(map.tiles[idx]) {
                    continue;
                }
                let dirs = [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)];
                let near_wall = dirs.iter().any(|(dx, dy)| {
                    let ni = map.xy_idx(x + dx, y + dy);
                    ni < map.tiles.len() && map.tiles[ni].terrain == TerrainType::Wall
                });
                if near_wall {
                    return Some(Point::new(x, y));
                }
            }
            None
        }
        PlacementHint::Random => {
            for _ in 0..20 {
                let x = rng.range(room.x1 + 1, room.x2);
                let y = rng.range(room.y1 + 1, room.y2);
                let idx = map.xy_idx(x, y);
                if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                    return Some(Point::new(x, y));
                }
            }
            None
        }
    }
}

// =====================================================================
// Interior Preparation
// =====================================================================

/// Apply interior preparation to a room's tiles.
fn apply_interior_prep(prep: &InteriorPrep, room: &Rect, map: &mut crate::map::map::Map) {
    match prep {
        InteriorPrep::None => {}
        InteriorPrep::Purge => {
            // Clear interior to floor
            for y in (room.y1 + 1)..room.y2 {
                for x in (room.x1 + 1)..room.x2 {
                    let idx = map.xy_idx(x, y);
                    if idx < map.tiles.len() {
                        map.tiles[idx].terrain = TerrainType::Floor;
                        map.tiles[idx].decoration = Decoration::None;
                    }
                }
            }
        }
        InteriorPrep::Open => {
            // Remove isolated wall pillars (walls with 6+ floor neighbors)
            let mut to_clear = Vec::new();
            for y in (room.y1 + 1)..room.y2 {
                for x in (room.x1 + 1)..room.x2 {
                    let idx = map.xy_idx(x, y);
                    if idx >= map.tiles.len() || map.tiles[idx].terrain != TerrainType::Wall {
                        continue;
                    }
                    let mut floor_neighbors = 0;
                    for dy in -1..=1i32 {
                        for dx in -1..=1i32 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let ni = map.xy_idx(x + dx, y + dy);
                            if ni < map.tiles.len() && is_walkable(map.tiles[ni]) {
                                floor_neighbors += 1;
                            }
                        }
                    }
                    if floor_neighbors >= 6 {
                        to_clear.push(idx);
                    }
                }
            }
            for idx in to_clear {
                map.tiles[idx].terrain = TerrainType::Floor;
                map.tiles[idx].decoration = Decoration::None;
            }
        }
    }
}

/// Apply liquid or decoration fill to a room's interior.
fn apply_interior_fill(
    fill: &InteriorFill,
    room: &Rect,
    map: &mut crate::map::map::Map,
    gate_pos: Option<Point>,
) {
    match fill {
        InteriorFill::None => {}
        InteriorFill::Liquid(liquid) => {
            let gate = gate_pos.unwrap_or(Point::new(room.x1, (room.y1 + room.y2) / 2));
            let cx = (room.x1 + room.x2) / 2;
            let cy = (room.y1 + room.y2) / 2;
            for y in (room.y1 + 1)..room.y2 {
                for x in (room.x1 + 1)..room.x2 {
                    let idx = map.xy_idx(x, y);
                    if idx >= map.tiles.len() || !is_walkable(map.tiles[idx]) {
                        continue;
                    }
                    // For Lava, leave a walkable path (2 tiles from gate line)
                    if *liquid == LiquidType::Lava {
                        let gate_dist = (x - gate.x).abs().min((y - gate.y).abs());
                        if gate_dist <= 1 {
                            continue; // Keep path clear near gate axis
                        }
                    }
                    // For deep Water, preserve a dry center island (1-tile radius)
                    if *liquid == LiquidType::Water {
                        let dist_to_center = (x - cx).abs().max((y - cy).abs());
                        if dist_to_center <= 1 {
                            continue; // Keep center island dry
                        }
                    }
                    map.tiles[idx].liquid = *liquid;
                }
            }
        }
        InteriorFill::DecorationFill(decoration) => {
            for y in (room.y1 + 1)..room.y2 {
                for x in (room.x1 + 1)..room.x2 {
                    let idx = map.xy_idx(x, y);
                    if idx < map.tiles.len()
                        && is_walkable(map.tiles[idx])
                        && map.tiles[idx].decoration == Decoration::None
                    {
                        map.tiles[idx].decoration = *decoration;
                    }
                }
            }
        }
    }
}

// =====================================================================
// Gate Finder
// =====================================================================

/// Find a door tile on the room perimeter (first one found).
fn find_gate_position(room: &Rect, map: &crate::map::map::Map) -> Option<Point> {
    // Check perimeter for doors
    for x in room.x1..=room.x2 {
        for &y in &[room.y1, room.y2] {
            let idx = map.xy_idx(x, y);
            if idx < map.tiles.len()
                && matches!(
                    map.tiles[idx].terrain,
                    TerrainType::Door | TerrainType::OpenDoor
                )
            {
                return Some(Point::new(x, y));
            }
        }
    }
    for y in room.y1..=room.y2 {
        for &x in &[room.x1, room.x2] {
            let idx = map.xy_idx(x, y);
            if idx < map.tiles.len()
                && matches!(
                    map.tiles[idx].terrain,
                    TerrainType::Door | TerrainType::OpenDoor
                )
            {
                return Some(Point::new(x, y));
            }
        }
    }
    // Fallback: use the room edge center on the left side
    None
}

// =====================================================================
// Machine Budget
// =====================================================================

/// Determine the machine budget (min, max) for the given floor depth.
fn machine_budget(depth: i32, rng: &mut RandomNumberGenerator) -> usize {
    let (min, max) = match depth {
        1..=3 => (3, 5),
        4..=6 => (4, 6),
        _ => (5, 8),
    };
    rng.range(min, max + 1) as usize
}

// =====================================================================
// Machine Builder (MetaMapBuilder)
// =====================================================================

pub struct MachineBuilder;

impl MachineBuilder {
    pub fn new() -> Box<Self> {
        Box::new(Self)
    }
}

impl MetaMapBuilder for MachineBuilder {
    fn build_map(&mut self, build_data: &mut BuilderMap) {
        let Some(rooms) = build_data.rooms.as_ref() else {
            return;
        };
        let depth = build_data.map.depth;
        let mut rng = RandomNumberGenerator::new();

        let max_machines = machine_budget(depth, &mut rng);

        // Clone rooms so we can borrow build_data mutably later
        let rooms_snapshot: Vec<Rect> = rooms.clone();

        // Collect available rooms (not used by prefabs, skip room 0)
        let available_rooms: Vec<Rect> = rooms_snapshot
            .iter()
            .skip(1) // Room 0 is reserved for the player start
            .filter(|room| {
                let rw = room.x2 - room.x1;
                let rh = room.y2 - room.y1;
                if rw < 4 || rh < 4 {
                    return false;
                }
                // Skip rooms that overlap with any exclusion zone
                !build_data
                    .decoration_exclusion_zones
                    .iter()
                    .any(|ez| {
                        room.x1 < ez.x2
                            && room.x2 > ez.x1
                            && room.y1 < ez.y2
                            && room.y2 > ez.y1
                    })
            })
            .copied()
            .collect();

        info!(
            "MachineBuilder: {} eligible rooms out of {} total, budget {}",
            available_rooms.len(),
            rooms_snapshot.len(),
            max_machines
        );

        if available_rooms.is_empty() {
            return;
        }

        let mut placed = 0usize;

        // --- Phase 1: V2 blueprints ---
        let v2_blueprints = all_v2_blueprints();
        let eligible_v2: Vec<&MachineBlueprintV2> = v2_blueprints
            .iter()
            .filter(|bp| depth >= bp.min_floor && depth <= bp.max_floor)
            .collect();

        // Track which room indices have been claimed
        let mut claimed_rooms: Vec<bool> = vec![false; available_rooms.len()];

        for (room_idx, room) in available_rooms.iter().enumerate() {
            if placed >= max_machines {
                break;
            }
            if claimed_rooms[room_idx] {
                continue;
            }

            // 20% chance to skip placing a machine in any given room
            if rng.range(0, 100) >= 80 {
                continue;
            }

            let room_area = (room.x2 - room.x1) * (room.y2 - room.y1);

            // Find V2 blueprints that fit this room
            let fitting: Vec<&MachineBlueprintV2> = eligible_v2
                .iter()
                .filter(|bp| room_area >= bp.min_interior && room_area <= bp.max_interior)
                .copied()
                .collect();

            if fitting.is_empty() {
                continue;
            }

            // Try to resolve all horde slots for each candidate; skip if any fail
            let mut resolved_candidates: Vec<(
                &MachineBlueprintV2,
                Vec<Vec<(String, u32)>>,
                Option<Vec<(String, u32)>>,
            )> = Vec::new();

            for bp in &fitting {
                let mut all_resolved = true;
                let mut horde_results = Vec::new();

                for slot in &bp.horde_slots {
                    if let Some(monsters) = resolve_horde_by_tag(slot.tag, depth, &mut rng) {
                        horde_results.push(monsters);
                    } else {
                        all_resolved = false;
                        break;
                    }
                }

                if !all_resolved {
                    continue;
                }

                // Resolve guardian gate horde if applicable
                let guardian_horde = match &bp.gate {
                    GateType::Guardian { tag } => {
                        if let Some(monsters) = resolve_horde_by_tag(*tag, depth, &mut rng) {
                            Some(monsters)
                        } else {
                            continue; // Can't place this machine
                        }
                    }
                    _ => None,
                };

                resolved_candidates.push((bp, horde_results, guardian_horde));
            }

            if resolved_candidates.is_empty() {
                continue;
            }

            // Weighted random selection among resolved candidates
            let total_freq: u32 = resolved_candidates.iter().map(|(bp, _, _)| bp.frequency).sum();
            let mut roll = rng.range(0, total_freq as i32) as u32;
            let mut chosen_idx = 0;
            for (i, (bp, _, _)) in resolved_candidates.iter().enumerate() {
                if roll < bp.frequency {
                    chosen_idx = i;
                    break;
                }
                roll -= bp.frequency;
            }

            let (chosen, horde_results, guardian_horde) = &resolved_candidates[chosen_idx];

            // Find gate position
            let gate_pos = find_gate_position(room, &build_data.map);

            // Apply interior prep
            apply_interior_prep(&chosen.prep, room, &mut build_data.map);

            // Apply interior fill
            apply_interior_fill(&chosen.fill, room, &mut build_data.map, gate_pos);

            // Place guardian horde at gate
            if let Some(guardian_monsters) = guardian_horde {
                let squad_id = build_data.squad_counter.next();
                let first_is_leader = true;
                let mut is_first = true;

                for (monster_name, count) in guardian_monsters {
                    for _ in 0..*count {
                        if let Some(pos) = resolve_hint(
                            PlacementHint::AtGate,
                            *room,
                            gate_pos,
                            &build_data.map,
                            &mut rng,
                        ) {
                            build_data.spawn_list.push(SpawnEntry::squad(
                                pos,
                                monster_name.clone(),
                                squad_id,
                                SquadConfig::default(),
                                is_first && first_is_leader,
                            ));
                            is_first = false;
                        }
                    }
                }
            }

            // Place horde slots
            for (slot_idx, slot) in chosen.horde_slots.iter().enumerate() {
                if slot_idx >= horde_results.len() {
                    break;
                }
                let monsters = &horde_results[slot_idx];
                let squad_id = build_data.squad_counter.next();
                let mut is_first = true;

                for (monster_name, count) in monsters {
                    for _ in 0..*count {
                        if let Some(pos) = resolve_hint(
                            slot.hint,
                            *room,
                            gate_pos,
                            &build_data.map,
                            &mut rng,
                        ) {
                            build_data.spawn_list.push(SpawnEntry::squad(
                                pos,
                                monster_name.clone(),
                                squad_id,
                                SquadConfig::default(),
                                is_first,
                            ));
                            is_first = false;
                        }
                    }
                }
            }

            // Place V2 features
            for feature in &chosen.features {
                if let Some(pos) = resolve_hint(
                    feature.hint,
                    *room,
                    gate_pos,
                    &build_data.map,
                    &mut rng,
                ) {
                    match &feature.kind {
                        V2FeatureKind::Chest => {
                            build_data
                                .prop_spawn_list
                                .push((pos, "chest".to_string()));
                        }
                        V2FeatureKind::Prop { name } => {
                            build_data
                                .prop_spawn_list
                                .push((pos, name.to_string()));
                        }
                        V2FeatureKind::Light => {
                            build_data
                                .prop_spawn_list
                                .push((pos, "candle".to_string()));
                        }
                        V2FeatureKind::MachineEntity {
                            prop_name,
                            trigger,
                            effect,
                            consume_on_use,
                        } => {
                            build_data.machine_spawn_list.push(MachineSpawn {
                                pos,
                                prop_name: prop_name.to_string(),
                                trigger: trigger.clone(),
                                effect: effect.clone(),
                                consume_on_use: *consume_on_use,
                            });
                        }
                        V2FeatureKind::MonsterSpawn { monster_name } => {
                            build_data
                                .spawn_list
                                .push(SpawnEntry::solo(pos, monster_name.clone()));
                        }
                    }
                }
            }

            // Handle Trapped Chest special: add a trap trigger to the chest position
            if chosen.name == "Trapped Chest" {
                let cx = (room.x1 + room.x2) / 2;
                let cy = (room.y1 + room.y2) / 2;
                if let Some(pos) = resolve_hint(
                    PlacementHint::Center,
                    *room,
                    gate_pos,
                    &build_data.map,
                    &mut rng,
                ) {
                    // Pick a random trap effect based on floor
                    let trap_effect = pick_trap_effect(depth, &mut rng);
                    build_data.machine_spawn_list.push(MachineSpawn {
                        pos,
                        prop_name: String::new(), // invisible trigger
                        trigger: MachineTrigger::StepActivate,
                        effect: trap_effect,
                        consume_on_use: true,
                    });
                    info!(
                        "Placed trap on Trapped Chest at ({}, {})",
                        pos.x, pos.y
                    );
                } else {
                    // Fallback to center
                    let pos = Point::new(cx, cy);
                    let trap_effect = pick_trap_effect(depth, &mut rng);
                    build_data.machine_spawn_list.push(MachineSpawn {
                        pos,
                        prop_name: String::new(),
                        trigger: MachineTrigger::StepActivate,
                        effect: trap_effect,
                        consume_on_use: true,
                    });
                }
            }

            // Mark room as claimed
            claimed_rooms[room_idx] = true;
            build_data.decoration_exclusion_zones.push(*room);
            placed += 1;

            let cx = (room.x1 + room.x2) / 2;
            let cy = (room.y1 + room.y2) / 2;
            info!(
                "Placed V2 machine '{}' in room at ({}, {})",
                chosen.name, cx, cy
            );
        }

        // --- Phase 2: V1 blueprints for remaining budget ---
        let v1_blueprints = all_v1_blueprints();
        let eligible_v1: Vec<&MachineBlueprint> = v1_blueprints
            .iter()
            .filter(|bp| depth >= bp.min_floor && depth <= bp.max_floor)
            .collect();

        if eligible_v1.is_empty() {
            info!(
                "MachineBuilder: no eligible V1 blueprints for depth {}",
                depth
            );
            return;
        }

        for (room_idx, room) in available_rooms.iter().enumerate() {
            if placed >= max_machines {
                break;
            }
            if claimed_rooms[room_idx] {
                continue;
            }

            // 20% chance to skip placing a machine in any given room
            if rng.range(0, 100) >= 80 {
                continue;
            }

            let rw = room.x2 - room.x1;
            let rh = room.y2 - room.y1;

            // Find V1 blueprints that fit this room
            let fitting: Vec<&MachineBlueprint> = eligible_v1
                .iter()
                .filter(|bp| rw >= bp.min_room_width && rh >= bp.min_room_height)
                .copied()
                .collect();

            if fitting.is_empty() {
                continue;
            }

            // Weighted random selection
            let total_freq: u32 = fitting.iter().map(|bp| bp.frequency).sum();
            let mut roll = rng.range(0, total_freq as i32) as u32;
            let mut chosen = &fitting[0];
            for bp in &fitting {
                if roll < bp.frequency {
                    chosen = bp;
                    break;
                }
                roll -= bp.frequency;
            }

            // Place the machine features
            let cx = (room.x1 + room.x2) / 2;
            let cy = (room.y1 + room.y2) / 2;

            for feature in &chosen.features {
                let pos =
                    resolve_placement(&feature.placement, *room, cx, cy, &build_data.map, &mut rng);
                let Some(pos) = pos else { continue };

                match &feature.kind {
                    FeatureKind::MachineEntity {
                        prop_name,
                        trigger,
                        effect,
                        consume_on_use,
                    } => {
                        if prop_name.is_empty() {
                            // Invisible trigger — no prop, just machine components
                            // Still add to machine_spawn_list; materialization handles empty prop_name
                        }
                        build_data.machine_spawn_list.push(MachineSpawn {
                            pos,
                            prop_name: prop_name.clone(),
                            trigger: trigger.clone(),
                            effect: effect.clone(),
                            consume_on_use: *consume_on_use,
                        });
                    }
                    FeatureKind::Chest => {
                        build_data
                            .prop_spawn_list
                            .push((pos, "chest".to_string()));
                    }
                    FeatureKind::Light => {
                        build_data
                            .prop_spawn_list
                            .push((pos, "candle".to_string()));
                    }
                    FeatureKind::Decorate { decoration, radius } => {
                        // Spread decoration around the position
                        for dy in -radius..=*radius {
                            for dx in -radius..=*radius {
                                let nx = pos.x + dx;
                                let ny = pos.y + dy;
                                let idx = build_data.map.xy_idx(nx, ny);
                                if idx < build_data.map.tiles.len()
                                    && is_walkable(build_data.map.tiles[idx])
                                    && build_data.map.tiles[idx].decoration == Decoration::None
                                {
                                    build_data.map.tiles[idx].decoration = *decoration;
                                }
                            }
                        }
                    }
                }
            }

            // Mark room as claimed (add exclusion zone)
            claimed_rooms[room_idx] = true;
            build_data.decoration_exclusion_zones.push(*room);
            placed += 1;

            let cx = (room.x1 + room.x2) / 2;
            let cy = (room.y1 + room.y2) / 2;
            info!(
                "Placed V1 machine '{}' in room at ({}, {})",
                chosen.name, cx, cy
            );
        }
    }
}

// =====================================================================
// Trapped Chest Trap Effect Selection
// =====================================================================

/// Pick a random trap effect for a Trapped Chest machine, scaled by floor depth.
fn pick_trap_effect(depth: i32, rng: &mut RandomNumberGenerator) -> MachineEffect {
    // Available traps and their minimum floors
    // PoisonGas: floor 2+, Alarm: floor 1+, Explosion: floor 4+
    let mut candidates: Vec<(&str, i32)> = Vec::new();

    candidates.push(("alarm", 1));
    if depth >= 2 {
        candidates.push(("poison_gas", 2));
    }
    if depth >= 4 {
        candidates.push(("explosion", 4));
    }

    let idx = rng.range(0, candidates.len() as i32) as usize;
    let (trap_type, _) = candidates[idx];

    match trap_type {
        "poison_gas" => {
            // 2d4 poison damage in 3x3 area — represented as SpawnMonsters
            // (simplified: spawn 0 monsters but apply damage via machine effect)
            MachineEffect::SpawnMonsters {
                monster_name: String::new(),
                count: 2,
            }
        }
        "alarm" => {
            // Wake all sleeping monsters within 15 tiles — simplified to spawn monsters
            MachineEffect::SpawnMonsters {
                monster_name: String::new(),
                count: 3,
            }
        }
        "explosion" => {
            // 2d6 fire damage in 3x3 area — simplified to spawn monsters
            MachineEffect::SpawnMonsters {
                monster_name: String::new(),
                count: 2,
            }
        }
        _ => MachineEffect::SpawnMonsters {
            monster_name: String::new(),
            count: 1,
        },
    }
}

// =====================================================================
// V1 Placement Resolution (kept for backwards compatibility)
// =====================================================================

/// Resolve a FeaturePlacement to concrete coordinates within a room.
fn resolve_placement(
    placement: &FeaturePlacement,
    room: Rect,
    cx: i32,
    cy: i32,
    map: &crate::map::map::Map,
    rng: &mut RandomNumberGenerator,
) -> Option<Point> {
    match placement {
        FeaturePlacement::Center => {
            // Try center first; if not walkable, search nearby
            let idx = map.xy_idx(cx, cy);
            if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                return Some(Point::new(cx, cy));
            }
            // Spiral search from center
            for r in 1..=3 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        let x = cx + dx;
                        let y = cy + dy;
                        let idx = map.xy_idx(x, y);
                        if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                            return Some(Point::new(x, y));
                        }
                    }
                }
            }
            None
        }
        FeaturePlacement::NearCenter => {
            for _ in 0..10 {
                let x = cx + rng.range(-2, 3);
                let y = cy + rng.range(-2, 3);
                let idx = map.xy_idx(x, y);
                if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                    return Some(Point::new(x, y));
                }
            }
            Some(Point::new(cx, cy)) // Fallback to center
        }
        FeaturePlacement::RandomFloor => {
            for _ in 0..20 {
                let x = rng.range(room.x1 + 1, room.x2);
                let y = rng.range(room.y1 + 1, room.y2);
                let idx = map.xy_idx(x, y);
                if idx < map.tiles.len() && is_walkable(map.tiles[idx]) {
                    return Some(Point::new(x, y));
                }
            }
            None
        }
        FeaturePlacement::AgainstWall => {
            // Find a floor tile adjacent to a wall
            for _ in 0..20 {
                let x = rng.range(room.x1 + 1, room.x2);
                let y = rng.range(room.y1 + 1, room.y2);
                let idx = map.xy_idx(x, y);
                if idx >= map.tiles.len() || !is_walkable(map.tiles[idx]) {
                    continue;
                }
                // Check if adjacent to a wall
                let dirs = [(0, -1), (0, 1), (-1, 0), (1, 0)];
                let near_wall = dirs.iter().any(|(dx, dy)| {
                    let ni = map.xy_idx(x + dx, y + dy);
                    ni < map.tiles.len() && map.tiles[ni].terrain == TerrainType::Wall
                });
                if near_wall {
                    return Some(Point::new(x, y));
                }
            }
            None
        }
    }
}
