use criterion::{black_box, criterion_group, criterion_main, Criterion};
use roguelike_engine::prelude::*;

// ---- Map Generation ----

fn bench_map_generation(c: &mut Criterion) {
    c.bench_function("map_gen_brogelike_80x60", |b| {
        b.iter(|| {
            let ctx = EngineBuilderMap::with_seed(1, 80, 60, "bench", black_box(42));
            let mut chain = BuilderChain::new(ctx);
            chain.add(BrogueLikeBuilder::dungeon(
                1,
                80,
                60,
                FloorProfile {
                    cavern_weight: 30,
                    target_rooms: 8,
                    ..Default::default()
                },
            ));
            chain.add(StartPointBuilder::new());
            chain.add(DiagonalCuller::new());
            chain.add(FinishDoors::new());
            chain.add(DistantExit::new());
            chain.build_map();
            chain.finish()
        })
    });
}

// ---- Turn Scheduling ----

fn bench_turn_scheduling(c: &mut Criterion) {
    use bevy::prelude::Entity;

    c.bench_function("turn_dequeue_100_entities", |b| {
        b.iter(|| {
            let mut tm = TurnManager::default();
            // Insert 100 entities
            for i in 0..100u32 {
                let entity = Entity::from_raw_u32(i).expect("valid test entity index");
                tm.add_entity(entity);
            }
            // Dequeue all
            let mut count = 0;
            loop {
                let result = dequeue_next_batch_pure(&mut tm, |_| false);
                match result {
                    DequeueOutcome::NpcBatch(batch) => count += batch.len(),
                    DequeueOutcome::Empty => break,
                    _ => {}
                }
            }
            black_box(count)
        })
    });

    c.bench_function("turn_insert_1000", |b| {
        b.iter(|| {
            let mut tm = TurnManager::default();
            for i in 0..1000u32 {
                let entity = Entity::from_raw_u32(i).expect("valid test entity index");
                let time = (i * 7) % 500; // Varied times
                tm.insert_at(entity, time);
            }
            black_box(tm.len())
        })
    });
}

// ---- Combat Math ----

fn bench_combat_math(c: &mut Criterion) {
    c.bench_function("combat_full_pipeline", |b| {
        b.iter(|| {
            let raw = black_box(20);
            let armor = black_box(5);
            let resist = black_box(30);
            let after_armor = compute_after_armor(raw, armor);
            let after_resist = apply_resistance(after_armor, resist);
            let final_dmg = apply_damage_multipliers(after_resist, true, false);
            black_box(final_dmg)
        })
    });

    c.bench_function("combat_damage_modifiers_5", |b| {
        let modifiers = vec![
            DamageModifier { multiplier: 1.5 },
            DamageModifier { multiplier: 0.8 },
            DamageModifier { multiplier: 1.2 },
            DamageModifier { multiplier: 0.9 },
            DamageModifier { multiplier: 1.1 },
        ];
        b.iter(|| black_box(apply_damage_modifiers(black_box(20), &modifiers)))
    });
}

// ---- GOAP Planning ----

fn bench_goap(c: &mut Criterion) {
    let actions = vec![
        ActionDef {
            name: "search",
            cost: 1,
            preconditions: vec![],
            effects: vec![(WorldStateProp::PlayerVisible, true)],
        },
        ActionDef {
            name: "engage",
            cost: 2,
            preconditions: vec![(WorldStateProp::PlayerVisible, true)],
            effects: vec![(WorldStateProp::AdjacentToThreat, true)],
        },
        ActionDef {
            name: "flee",
            cost: 1,
            preconditions: vec![(WorldStateProp::HpLow, true)],
            effects: vec![(WorldStateProp::HasEscapeRoute, true)],
        },
        ActionDef {
            name: "heal",
            cost: 3,
            preconditions: vec![(WorldStateProp::HpLow, true)],
            effects: vec![(WorldStateProp::HpLow, false)],
        },
        ActionDef {
            name: "alert_squad",
            cost: 1,
            preconditions: vec![(WorldStateProp::PlayerVisible, true)],
            effects: vec![(WorldStateProp::HostileNearby, true)],
        },
    ];

    let goals = vec![
        Goal {
            name: "attack",
            priority: 5,
            desired: vec![(WorldStateProp::AdjacentToThreat, true)],
        },
        Goal {
            name: "survive",
            priority: 10,
            desired: vec![(WorldStateProp::HasEscapeRoute, true)],
        },
    ];

    c.bench_function("goap_plan_5_actions_2_goals", |b| {
        let state = WorldState::default();
        b.iter(|| black_box(plan(black_box(&state), &goals, &actions)))
    });

    c.bench_function("goap_plan_hp_low", |b| {
        let mut state = WorldState::default();
        state.hp_low = true;
        b.iter(|| black_box(plan(black_box(&state), &goals, &actions)))
    });
}

// ---- Geometry ----

fn bench_geometry(c: &mut Criterion) {
    c.bench_function("geometry_aoe_radius_5", |b| {
        b.iter(|| black_box(tiles_in_aoe(black_box(40), black_box(30), 5)))
    });

    c.bench_function("geometry_manhattan_distance", |b| {
        b.iter(|| {
            black_box(manhattan_distance(
                black_box(10),
                black_box(20),
                black_box(50),
                black_box(60),
            ))
        })
    });
}

// ---- AI Decisions ----

fn bench_ai_decisions(c: &mut Criterion) {
    c.bench_function("ai_should_flee", |b| {
        b.iter(|| black_box(should_flee(black_box(3), black_box(10), black_box(0.3))))
    });

    c.bench_function("ai_flee_direction", |b| {
        b.iter(|| {
            black_box(flee_direction(
                black_box(5),
                black_box(5),
                black_box(8),
                black_box(3),
            ))
        })
    });
}

criterion_group!(
    benches,
    bench_map_generation,
    bench_turn_scheduling,
    bench_combat_math,
    bench_goap,
    bench_geometry,
    bench_ai_decisions,
);
criterion_main!(benches);
