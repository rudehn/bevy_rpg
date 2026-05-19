//! Turn scheduling primitives.
//!
//! This module owns the engine's turn scheduling core: the [`TurnManager`]
//! resource (a priority-queue of entities keyed by scheduled time), the pure
//! dequeue logic in [`dequeue_next_batch_pure`], and the
//! [`compute_reinsert_time`] helper used by action cost computation.
//!
//! The full Bevy plugin (`TurnOrderPlugin`, `TurnState`, `ProcessingPhase`,
//! `CombatReactionSet`) still lives in the game crate for now; those pieces
//! depend on game-side player identification and on the game's UI log. When
//! the plugin is eventually migrated, it will consume these primitives via
//! `roguelike_engine::turn::*`.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy::prelude::*;

/// Emitted when a full turn cycle has ended. Subscribers (tile promotion,
/// fire/gas/water sims, status-effect ticks, etc.) listen for this to run
/// their per-turn updates.
#[derive(Message)]
pub struct TurnEndEvent;

/// A single entry in the turn queue, ordered first by scheduled time and then
/// by insertion order (to break ties deterministically, preserving FIFO
/// semantics for equal times).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TurnEntry {
    time: u32,
    insertion_order: u64,
    entity: Entity,
}

impl Ord for TurnEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time
            .cmp(&other.time)
            .then(self.insertion_order.cmp(&other.insertion_order))
    }
}

impl PartialOrd for TurnEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Resource holding the turn queue for the engine's turn scheduler.
///
/// Internally uses a min-heap (`BinaryHeap<Reverse<TurnEntry>>`) so that
/// `peek` / `pop` always yield the earliest scheduled entity in O(log n)
/// time. An `insertion_counter` guarantees stable FIFO ordering for entities
/// scheduled at the same tick.
#[derive(Resource, Default)]
pub struct TurnManager {
    /// Min-heap of turn entries. Wrapped in `Reverse` so `BinaryHeap`
    /// (which is a max-heap) yields the *smallest* `(time, insertion_order)`
    /// first.
    turn_queue: BinaryHeap<Reverse<TurnEntry>>,
    /// Monotonically increasing counter used to break ties in scheduled time
    /// so that entities inserted first are dequeued first.
    insertion_counter: u64,
    /// Global clock. Advances as actors are dequeued.
    pub current_time: u32,
}

impl TurnManager {
    /// Insert `entity` at the current global time.
    pub fn add_entity(&mut self, entity: Entity) {
        let order = self.insertion_counter;
        self.insertion_counter += 1;
        self.turn_queue.push(Reverse(TurnEntry {
            time: self.current_time,
            insertion_order: order,
            entity,
        }));
    }

    /// Insert an entity at a specific scheduled time.
    pub fn insert_at(&mut self, entity: Entity, time: u32) {
        let order = self.insertion_counter;
        self.insertion_counter += 1;
        self.turn_queue.push(Reverse(TurnEntry {
            time,
            insertion_order: order,
            entity,
        }));
    }

    /// No-op retained for backward compatibility.
    ///
    /// With the old `Vec`-based queue, callers needed to explicitly re-sort
    /// after bulk mutations. The `BinaryHeap` is always in heap order, so
    /// this method does nothing.
    pub fn sort_queue(&mut self) {
        // Heap is always sorted — nothing to do.
    }

    /// Peek at the next scheduled time without removing anything. Returns
    /// `None` if the queue is empty.
    pub fn peek_time(&self) -> Option<u32> {
        self.turn_queue.peek().map(|Reverse(entry)| entry.time)
    }

    /// Returns the number of entities currently in the turn queue.
    pub fn len(&self) -> usize {
        self.turn_queue.len()
    }

    /// Returns `true` if the turn queue contains no entities.
    pub fn is_empty(&self) -> bool {
        self.turn_queue.is_empty()
    }

    /// Returns `true` if the given entity is anywhere in the turn queue.
    pub fn contains(&self, entity: Entity) -> bool {
        self.turn_queue.iter().any(|Reverse(e)| e.entity == entity)
    }

    /// Remove all occurrences of `entity` from the turn queue.
    pub fn remove_entity(&mut self, entity: Entity) {
        let old = std::mem::take(&mut self.turn_queue);
        self.turn_queue = old.into_iter().filter(|Reverse(e)| e.entity != entity).collect();
    }
}

/// Compute the re-insertion time for an entity after completing an action.
///
/// `base_cost` is the raw action cost, `delay` is the speed multiplier
/// (e.g. from a speed component). Returns `current_time + round(base_cost * delay)`.
pub fn compute_reinsert_time(current_time: u32, base_cost: u32, delay: f32) -> u32 {
    let cost = (base_cost as f32 * delay).round() as u32;
    current_time + cost
}

/// Outcome of a pure dequeue operation (no ECS side-effects).
#[derive(Debug, PartialEq, Eq)]
pub enum DequeueOutcome {
    /// The player entity is next to act.
    PlayerReady(Entity),
    /// A batch of NPC entities is ready to act.
    NpcBatch(Vec<Entity>),
    /// The queue is empty or no actors are scheduled at the current time.
    Empty,
}

/// Maximum number of NPCs that can act in a single batch before yielding.
///
/// Games can treat this as advisory. The constant is exposed so games can
/// tune their per-frame cap; the default protects against infinite loops
/// when many NPCs share a scheduled time.
pub const MAX_NPC_BATCH: u32 = 16;

/// Pure dequeue logic: determines which entities should act next without
/// touching ECS state.
///
/// - `is_player` is a closure that returns `true` if an entity is the player.
///   The caller normally wraps a Bevy `Query<Entity, With<Player>>` check.
/// - Dequeued entities are physically removed from `turn_manager.turn_queue`.
/// - If the player's scheduled time is tied with NPCs, NPCs go first in
///   insertion order; the player waits until the next dequeue cycle.
/// - The batch is capped at [`MAX_NPC_BATCH`].
pub fn dequeue_next_batch_pure(
    turn_manager: &mut TurnManager,
    is_player: impl Fn(Entity) -> bool,
) -> DequeueOutcome {
    let mut npc_batch: Vec<Entity> = Vec::new();

    while let Some(Reverse(entry)) = turn_manager.turn_queue.peek() {
        if entry.time > turn_manager.current_time {
            break;
        }

        if is_player(entry.entity) {
            if !npc_batch.is_empty() {
                break;
            }
            let Reverse(entry) = turn_manager.turn_queue.pop().unwrap();
            return DequeueOutcome::PlayerReady(entry.entity);
        }

        if npc_batch.len() as u32 >= MAX_NPC_BATCH {
            break;
        }

        let Reverse(entry) = turn_manager.turn_queue.pop().unwrap();
        npc_batch.push(entry.entity);
    }

    if !npc_batch.is_empty() {
        DequeueOutcome::NpcBatch(npc_batch)
    } else {
        DequeueOutcome::Empty
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn entity(n: u32) -> Entity {
        Entity::from_raw_u32(n).expect("valid test entity index")
    }

    /// Build a TurnManager with a given current_time and pre-sorted queue entries.
    /// Entries are inserted in order so insertion_order reflects their position
    /// in the slice, preserving FIFO semantics for equal times.
    fn make_tm(current_time: u32, entries: &[(Entity, u32)]) -> TurnManager {
        let mut tm = TurnManager {
            turn_queue: BinaryHeap::new(),
            insertion_counter: 0,
            current_time,
        };
        for &(entity, time) in entries {
            tm.insert_at(entity, time);
        }
        tm
    }

    // -----------------------------------------------------------------------
    // TurnManager basic operations
    // -----------------------------------------------------------------------

    #[test]
    fn add_entity_inserts_at_current_time() {
        let mut tm = TurnManager::default();
        tm.current_time = 50;
        tm.add_entity(entity(1));
        assert_eq!(tm.len(), 1);
        assert_eq!(tm.peek_time(), Some(50));
        assert!(tm.contains(entity(1)));
    }

    #[test]
    fn insert_at_maintains_sorted_order() {
        let mut tm = TurnManager::default();
        tm.insert_at(entity(1), 300);
        tm.insert_at(entity(2), 100);
        tm.insert_at(entity(3), 200);

        // Drain the heap and collect times in dequeue order.
        let mut times = Vec::new();
        while let Some(Reverse(entry)) = tm.turn_queue.pop() {
            times.push(entry.time);
        }
        assert_eq!(times, vec![100, 200, 300]);
    }

    #[test]
    fn sort_queue_stable_for_equal_times() {
        // Entities inserted in order at the same time should keep that order
        // when dequeued (guaranteed by insertion_order tiebreaker).
        let mut tm = TurnManager::default();
        tm.current_time = 100;
        tm.insert_at(entity(1), 100);
        tm.insert_at(entity(2), 100);
        tm.insert_at(entity(3), 100);
        tm.sort_queue(); // no-op, but kept for API compat

        let mut entities = Vec::new();
        while let Some(Reverse(entry)) = tm.turn_queue.pop() {
            entities.push(entry.entity);
        }
        assert_eq!(entities, vec![entity(1), entity(2), entity(3)]);
    }

    #[test]
    fn peek_time_returns_lowest() {
        let tm = make_tm(0, &[(entity(1), 50), (entity(2), 100)]);
        assert_eq!(tm.peek_time(), Some(50));
    }

    #[test]
    fn peek_time_empty_returns_none() {
        let tm = TurnManager::default();
        assert_eq!(tm.peek_time(), None);
    }

    // -----------------------------------------------------------------------
    // compute_reinsert_time
    // -----------------------------------------------------------------------

    #[test]
    fn reinsert_time_default_speed() {
        // delay=1.0, base_cost=100 -> reinsert at current_time+100
        assert_eq!(compute_reinsert_time(0, 100, 1.0), 100);
    }

    #[test]
    fn reinsert_time_slow_entity() {
        // delay=1.5, base_cost=100 -> 150
        assert_eq!(compute_reinsert_time(0, 100, 1.5), 150);
    }

    #[test]
    fn reinsert_time_fast_entity() {
        // delay=0.5, base_cost=100 -> 50
        assert_eq!(compute_reinsert_time(0, 100, 0.5), 50);
    }

    #[test]
    fn reinsert_time_rounds_correctly() {
        // delay=0.33, base_cost=100 -> 33.0 -> 33
        assert_eq!(compute_reinsert_time(0, 100, 0.33), 33);
        // delay=0.335, base_cost=100 -> 33.5 -> 34 (rounds up at .5)
        assert_eq!(compute_reinsert_time(0, 100, 0.335), 34);
    }

    #[test]
    fn reinsert_time_with_nonzero_current_time() {
        assert_eq!(compute_reinsert_time(500, 100, 1.0), 600);
        assert_eq!(compute_reinsert_time(500, 100, 2.0), 700);
    }

    #[test]
    fn reinsert_time_zero_cost_free_action() {
        // base_cost=0 means no time passes regardless of delay
        assert_eq!(compute_reinsert_time(100, 0, 1.0), 100);
        assert_eq!(compute_reinsert_time(100, 0, 3.0), 100);
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure -- empty queue
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_empty_queue_returns_empty() {
        let mut tm = TurnManager::default();
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::Empty);
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure -- single entity
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_single_npc() {
        let npc = entity(1);
        let mut tm = make_tm(0, &[(npc, 0)]);
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc]));
        assert!(tm.is_empty());
    }

    #[test]
    fn dequeue_single_player() {
        let player = entity(1);
        let mut tm = make_tm(0, &[(player, 0)]);
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));
        assert!(tm.is_empty());
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure -- player comes first
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_player_first_returns_player_ready() {
        let player = entity(1);
        let npc_a = entity(2);
        let mut tm = make_tm(100, &[(player, 100), (npc_a, 100)]);

        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));
        // NPC should remain in queue
        assert_eq!(tm.len(), 1);
        assert!(tm.contains(npc_a));
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure -- NPCs batch before player
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_npcs_batch_before_player_at_same_time() {
        let npc_a = entity(1);
        let npc_b = entity(2);
        let player = entity(3);

        // NPCs are in queue before the player (insertion order), all at time 0.
        let mut tm = make_tm(0, &[(npc_a, 0), (npc_b, 0), (player, 0)]);

        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc_a, npc_b]));
        // Player is still in the queue, not yet dequeued.
        assert_eq!(tm.len(), 1);
        assert!(tm.contains(player));
    }

    #[test]
    fn dequeue_npcs_before_player_different_times() {
        let npc = entity(1);
        let player = entity(2);

        // NPC at time 50, player at time 100, current_time=100.
        // NPC's time < current_time, so it acts. Player at exactly current_time.
        let mut tm = make_tm(100, &[(npc, 50), (player, 100)]);

        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc]));
        // Player deferred to next dequeue cycle.
        assert_eq!(tm.len(), 1);
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure -- NPC batch limit (MAX_NPC_BATCH)
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_respects_max_npc_batch() {
        let mut entries: Vec<(Entity, u32)> = Vec::new();
        // 20 NPCs all at time 0
        for i in 0..20 {
            entries.push((entity(i), 0));
        }
        let mut tm = make_tm(0, &entries);

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        match result {
            DequeueOutcome::NpcBatch(batch) => {
                assert_eq!(batch.len(), MAX_NPC_BATCH as usize);
            }
            other => panic!("Expected NpcBatch, got {:?}", other),
        }
        // Remaining 4 NPCs still in queue
        assert_eq!(tm.len(), 4);
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure -- future entities not dequeued
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_skips_future_entities() {
        let npc = entity(1);
        let mut tm = make_tm(0, &[(npc, 100)]);

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::Empty);
        // Entity remains in queue
        assert_eq!(tm.len(), 1);
    }

    #[test]
    fn dequeue_takes_ready_entities_leaves_future() {
        let npc_ready = entity(1);
        let npc_future = entity(2);
        let mut tm = make_tm(100, &[(npc_ready, 100), (npc_future, 200)]);

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc_ready]));
        assert_eq!(tm.len(), 1);
        // The remaining entry is the future NPC.
        assert!(tm.contains(npc_future));
        assert_eq!(tm.peek_time(), Some(200));
    }

    // -----------------------------------------------------------------------
    // dequeue_next_batch_pure -- entities at or below current_time all dequeue
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_entities_at_and_below_current_time() {
        let npc_a = entity(1);
        let npc_b = entity(2);
        let npc_c = entity(3);
        // current_time=100. Entity at 50, 100, 150.
        let mut tm = make_tm(100, &[(npc_a, 50), (npc_b, 100), (npc_c, 150)]);

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc_a, npc_b]));
        assert_eq!(tm.len(), 1);
        assert!(tm.contains(npc_c));
        assert_eq!(tm.peek_time(), Some(150));
    }

    // -----------------------------------------------------------------------
    // Full turn cycle simulation
    // -----------------------------------------------------------------------

    #[test]
    fn full_turn_cycle_player_and_npc() {
        let player = entity(1);
        let npc = entity(2);

        // Both start at time 0.
        let mut tm = make_tm(0, &[(player, 0), (npc, 0)]);

        // Advance time to the first scheduled actor.
        tm.current_time = tm.peek_time().unwrap();

        // Player is first (lower insertion_order, same time).
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));

        // Simulate player action: re-insert at current_time + 100 (base cost, delay=1.0)
        let player_reinsert = compute_reinsert_time(tm.current_time, 100, 1.0);
        tm.insert_at(player, player_reinsert);

        // Now dequeue again -- NPC should be next.
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc]));

        // Simulate NPC action: re-insert with delay=1.5 (slow monster)
        let npc_reinsert = compute_reinsert_time(tm.current_time, 100, 1.5);
        tm.insert_at(npc, npc_reinsert);

        // Advance to next cycle.
        tm.sort_queue();
        tm.current_time = tm.peek_time().unwrap();

        // Player at 100, NPC at 150 -> player goes first.
        assert_eq!(tm.current_time, 100);
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));
    }

    #[test]
    fn fast_entity_acts_more_often() {
        let fast = entity(1);
        let slow = entity(2);

        let mut tm = make_tm(0, &[(fast, 0), (slow, 0)]);
        tm.current_time = 0;

        // Dequeue both NPCs.
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![fast, slow]));

        // Re-insert: fast at delay=0.5 (cost=50), slow at delay=2.0 (cost=200).
        tm.insert_at(fast, compute_reinsert_time(0, 100, 0.5));
        tm.insert_at(slow, compute_reinsert_time(0, 100, 2.0));

        // Advance to next.
        tm.current_time = tm.peek_time().unwrap();
        assert_eq!(tm.current_time, 50);

        // Only fast is ready.
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![fast]));

        // Re-insert fast again.
        tm.insert_at(fast, compute_reinsert_time(50, 100, 0.5));

        // Advance.
        tm.current_time = tm.peek_time().unwrap();
        assert_eq!(tm.current_time, 100);

        // Only fast again (slow at 200).
        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![fast]));
    }

    // -----------------------------------------------------------------------
    // Free action: re-insertion at same time
    // -----------------------------------------------------------------------

    #[test]
    fn free_action_reinserts_at_current_time() {
        let player = entity(1);
        let npc = entity(2);

        let mut tm = make_tm(100, &[(npc, 200)]);

        // Simulate a free action: player goes back in at current_time.
        tm.insert_at(player, tm.current_time);

        // Player (time=100) is before NPC (time=200).
        assert_eq!(tm.peek_time(), Some(100));
        assert!(tm.contains(player));
        // Player acts before the NPC at 200.
        tm.current_time = tm.peek_time().unwrap();
        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::PlayerReady(player));
    }

    // -----------------------------------------------------------------------
    // Dedup: entity already in queue should not be double-inserted
    // (mirrors the check in resolve_turn_end)
    // -----------------------------------------------------------------------

    #[test]
    fn dedup_prevents_double_insertion() {
        let ent = entity(1);
        let mut tm = make_tm(0, &[(ent, 100)]);

        // Simulate the dedup check from resolve_turn_end.
        let already_present = tm.contains(ent);
        assert!(already_present);

        // Should NOT insert again.
        if !already_present {
            tm.insert_at(ent, 200);
        }
        assert_eq!(tm.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Edge: identical scheduled times -- stable ordering preserved
    // -----------------------------------------------------------------------

    #[test]
    fn stable_order_with_identical_times() {
        let a = entity(10);
        let b = entity(20);
        let c = entity(30);

        let mut tm = make_tm(100, &[(a, 100), (b, 100), (c, 100)]);
        tm.sort_queue(); // no-op, kept for API compat

        let result = dequeue_next_batch_pure(&mut tm, |_| false);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![a, b, c]));
    }

    // -----------------------------------------------------------------------
    // Edge: player scheduled after NPCs -- NPCs batch, player waits
    // -----------------------------------------------------------------------

    #[test]
    fn player_scheduled_later_waits() {
        let player = entity(1);
        let npc = entity(2);

        let mut tm = make_tm(100, &[(npc, 100), (player, 200)]);

        let result = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(result, DequeueOutcome::NpcBatch(vec![npc]));
        // Player still in queue at future time.
        assert_eq!(tm.len(), 1);
        assert!(tm.contains(player));
        assert_eq!(tm.peek_time(), Some(200));
    }

    // -----------------------------------------------------------------------
    // Mixed scenario: multiple dequeue rounds
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_dequeue_rounds() {
        let player = entity(1);
        let npc_a = entity(2);
        let npc_b = entity(3);

        // NPCs at 0, player at 50.
        let mut tm = make_tm(0, &[(npc_a, 0), (npc_b, 0), (player, 50)]);

        // Round 1: NPCs batch.
        let r1 = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(r1, DequeueOutcome::NpcBatch(vec![npc_a, npc_b]));

        // Re-insert NPCs at time 100.
        tm.insert_at(npc_a, 100);
        tm.insert_at(npc_b, 100);

        // Round 2: nothing at current_time=0 anymore, empty.
        let r2 = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(r2, DequeueOutcome::Empty);

        // Advance time to next actor (player at 50).
        tm.current_time = tm.peek_time().unwrap();
        assert_eq!(tm.current_time, 50);

        // Round 3: player is ready.
        let r3 = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(r3, DequeueOutcome::PlayerReady(player));

        // Re-insert player at 150.
        tm.insert_at(player, 150);

        // Advance.
        tm.current_time = tm.peek_time().unwrap();
        assert_eq!(tm.current_time, 100);

        // Round 4: both NPCs batch.
        let r4 = dequeue_next_batch_pure(&mut tm, |e| e == player);
        assert_eq!(r4, DequeueOutcome::NpcBatch(vec![npc_a, npc_b]));
    }
}
