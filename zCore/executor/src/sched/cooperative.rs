//! Cooperative FIFO scheduler — wraps the current bitmap-scan behavior.
//!
//! Tasks are selected in the order they were notified (FIFO by bitmap
//! position within each waker page). Priority is ignored. This exactly
//! reproduces the pre-refactor generator behavior.

use crate::task_collection::Key;
use alloc::collections::VecDeque;

/// Per-CPU scheduler state.
pub struct SchedState {
    ready: VecDeque<Key>,
}

/// Create a new per-CPU scheduler state.
pub fn new(_cpu_id: u8) -> SchedState {
    SchedState {
        ready: VecDeque::new(),
    }
}

/// Called when a task is spawned.
pub fn on_task_added(_state: &mut SchedState, _key: Key, _priority: usize) {}

/// Called when a task is removed.
pub fn on_task_removed(state: &mut SchedState, key: Key) {
    state.ready.retain(|&k| k != key);
}

/// Called when a task is woken (notified).
pub fn on_task_notified(state: &mut SchedState, key: Key) {
    // Avoid duplicates — a task can be notified while already in the queue.
    if !state.ready.contains(&key) {
        state.ready.push_back(key);
    }
}

/// Select the next task to run.
pub fn pick_next(state: &mut SchedState) -> Option<Key> {
    state.ready.pop_front()
}

/// Called when the current task voluntarily yields.
pub fn on_yield(state: &mut SchedState, key: Key) {
    // Re-add to back of queue for round-robin.
    if !state.ready.contains(&key) {
        state.ready.push_back(key);
    }
}

/// Called on timer tick. Returns true if the task should be preempted.
pub fn should_preempt(_state: &mut SchedState, _current_key: Key) -> bool {
    true // Always preempt on timer (current behavior).
}

/// Try to steal a task for another CPU.
pub fn steal_task(state: &mut SchedState) -> Option<Key> {
    state.ready.pop_front()
}
