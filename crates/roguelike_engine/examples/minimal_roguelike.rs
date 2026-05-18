//! Minimal headless roguelike demonstrating the engine's core systems.
//!
//! Run with: `cargo run --example minimal_roguelike`
//!
//! This example generates a BrogueLike dungeon, spawns a player and a
//! squad of goblins, wires up the faction matrix, and runs 10 simulated
//! turns through the engine's turn scheduler -- printing state to stdout
//! each step.

use bevy::prelude::*;
use roguelike_engine::prelude::{self as engine, *};

/// Marker for the player entity.
#[derive(Component)]
struct Player;

/// Marker for monster entities.
#[derive(Component)]
struct Monster;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins((
            CombatPlugin,
            FovPlugin,
            SquadPlugin,
            StatusEffectPlugin,
            AbilityPlugin,
        ))
        .init_resource::<TurnManager>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (update_squad_target, simulation_step).chain(),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut turn_manager: ResMut<TurnManager>,
) {
    println!("=== Roguelike Engine Demo ===\n");

    // ------------------------------------------------------------------
    // Generate dungeon
    // ------------------------------------------------------------------
    let ctx = EngineBuilderMap::with_seed(1, 40, 25, "Demo Floor", 42);
    let mut chain = BuilderChain::new(ctx);
    chain.add(BrogueLikeBuilder::dungeon(
        1,
        40,
        25,
        FloorProfile {
            cavern_weight: 20,
            target_rooms: 6,
            ..Default::default()
        },
    ));
    chain.add(StartPointBuilder::new());
    chain.add(DiagonalCuller::new());
    chain.add(FinishDoors::new());
    chain.add(DistantExit::new());
    chain.build_map();
    let finished = chain.finish();
    let map = finished.map;
    let start = finished
        .starting_position
        .unwrap_or(Position { x: 5, y: 5 });

    println!("Map generated: {}x{}", map.width, map.height);
    println!("Player start: ({}, {})\n", start.x, start.y);

    // ------------------------------------------------------------------
    // Set up factions (programmatic, no RON asset needed)
    // ------------------------------------------------------------------
    let factions = FactionMatrix::from_entries(&[(
        "player".into(),
        "monsters".into(),
        Relation::Hostile,
    )]);
    commands.insert_resource(factions);

    // ------------------------------------------------------------------
    // Spawn player
    // ------------------------------------------------------------------
    let player = commands
        .spawn((
            Player,
            engine::Name("Hero".to_string()),
            Position {
                x: start.x,
                y: start.y,
            },
            Viewshed::new(8),
            Health {
                current: 30,
                max: 30,
            },
            Collider,
            FovRevealsMap,
            Faction(FactionKind::new("player")),
        ))
        .id();
    turn_manager.add_entity(player);
    println!("  Spawned Hero at ({}, {})", start.x, start.y);

    // ------------------------------------------------------------------
    // Find walkable positions far from the player for monsters
    // ------------------------------------------------------------------
    let mut monster_positions = Vec::new();
    for y in 1..map.height - 1 {
        for x in 1..map.width - 1 {
            let idx = map.xy_idx(x, y);
            if is_walkable(map.tiles[idx])
                && manhattan_distance(x, y, start.x, start.y) > 5
                && monster_positions.len() < 4
            {
                // Spread monsters apart
                if monster_positions
                    .iter()
                    .all(|&(mx, my): &(i32, i32)| manhattan_distance(x, y, mx, my) > 3)
                {
                    monster_positions.push((x, y));
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Spawn monsters in a squad
    // ------------------------------------------------------------------
    let squad_id = SquadId(1);
    let monster_names = [
        "Goblin Scout",
        "Goblin Guard",
        "Goblin Shaman",
        "Goblin Chief",
    ];
    let mut first = true;
    for (i, &(mx, my)) in monster_positions.iter().enumerate() {
        let name = monster_names.get(i).copied().unwrap_or("Goblin");
        let mut entity_cmds = commands.spawn((
            Monster,
            engine::Name(name.to_string()),
            Position { x: mx, y: my },
            Viewshed::new(6),
            Health {
                current: 8,
                max: 8,
            },
            Collider,
            MonsterAI::default(),
            squad_id,
            SquadConfig::default(),
            Morale::default(),
            Faction(FactionKind::new("monsters")),
        ));

        if first {
            entity_cmds.insert(SquadLeader);
            first = false;
        }

        let id = entity_cmds.id();
        turn_manager.add_entity(id);
        println!("  Spawned {} at ({}, {})", name, mx, my);
    }

    // Insert map as resource (after we are done reading it for positions)
    commands.insert_resource(map);

    println!("\nSimulation starting...\n");
}

/// Feed the player position into SquadTarget so squad alerting works.
fn update_squad_target(
    player_query: Query<&Position, With<Player>>,
    mut squad_target: ResMut<SquadTarget>,
) {
    if let Ok(pos) = player_query.single() {
        squad_target.position = Some(bracket_lib::prelude::Point::new(pos.x, pos.y));
    }
}

/// Simple simulation: dequeue turns and print state each step.
fn simulation_step(
    mut turn_manager: ResMut<TurnManager>,
    player_query: Query<Entity, With<Player>>,
    all_query: Query<(
        Entity,
        &engine::Name,
        &Position,
        &Health,
        Option<&Player>,
        Option<&MonsterAI>,
    )>,
    mut step_count: Local<u32>,
) {
    if *step_count > 10 {
        return;
    }
    if *step_count == 10 {
        println!("\n=== Simulation complete (10 turns) ===");
        println!("\nFinal state:");
        for (_, name, pos, health, is_player, ai) in all_query.iter() {
            let role = if is_player.is_some() {
                "PLAYER"
            } else {
                "NPC"
            };
            let ai_state = ai.map(|a| a.display_state()).unwrap_or("");
            println!(
                "  [{}] {} at ({},{}) HP:{}/{} {}",
                role, name.0, pos.x, pos.y, health.current, health.max, ai_state
            );
        }
        *step_count += 1;
        std::process::exit(0);
    }

    // Advance time to next scheduled entity
    if let Some(next_time) = turn_manager.peek_time() {
        turn_manager.current_time = next_time;
    } else {
        return;
    }

    let player_entity = player_query.single().ok();
    let is_player_fn = |e: Entity| -> bool { player_entity.is_some_and(|p| p == e) };

    let outcome = dequeue_next_batch_pure(&mut turn_manager, is_player_fn);

    match outcome {
        DequeueOutcome::PlayerReady(entity) => {
            if let Ok((_, name, pos, health, _, _)) = all_query.get(entity) {
                println!(
                    "[Turn {}] Player '{}' at ({},{}) HP:{}/{}",
                    *step_count, name.0, pos.x, pos.y, health.current, health.max
                );
            }
            // Re-insert player (simulating a basic action)
            let next_time =
                compute_reinsert_time(turn_manager.current_time, BASE_ACTION_COST, 1.0);
            turn_manager.insert_at(entity, next_time);
            *step_count += 1;
        }
        DequeueOutcome::NpcBatch(npcs) => {
            for entity in &npcs {
                if let Ok((_, name, pos, health, _, ai)) = all_query.get(*entity) {
                    let state = ai.map(|a| a.display_state()).unwrap_or("");
                    println!(
                        "[Turn {}] NPC '{}' at ({},{}) HP:{}/{} {}",
                        *step_count, name.0, pos.x, pos.y, health.current, health.max, state
                    );
                }
                // Re-insert NPC
                let next_time =
                    compute_reinsert_time(turn_manager.current_time, BASE_ACTION_COST, 1.0);
                turn_manager.insert_at(*entity, next_time);
            }
            *step_count += 1;
        }
        DequeueOutcome::Empty => {}
    }
}
