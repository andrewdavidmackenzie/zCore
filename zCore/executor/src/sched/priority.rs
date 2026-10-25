//! Priority round-robin scheduler with weight-based time slices.
//!
//! Tasks are grouped by priority level (0-31, higher = more important).
//! `pick_next()` always returns the highest-priority ready task.
//! Within the same priority level, tasks are served round-robin.
//!
//! Each priority level maps to a weight via an exponential curve
//! matching Fuchsia's fair scheduler: `weight = 1.1^(priority - 31)`.
//! The weight determines the thread's time slice quantum via
//! `weight_to_time_slice_ns()`.

use crate::task_collection::Key;
use alloc::collections::VecDeque;
use hashbrown::HashMap;

/// Per-CPU scheduler state.
pub struct SchedState {
    /// Bitmask of non-empty priority levels.
    ready_bitmap: u32,
    /// Per-priority FIFO queues of ready task keys.
    queues: [VecDeque<Key>; 32],
    /// Maps task key to its priority level.
    task_priorities: HashMap<Key, usize>,
}

/// Create a new per-CPU scheduler state.
pub fn new(_cpu_id: u8) -> SchedState {
    SchedState {
        ready_bitmap: 0,
        queues: core::array::from_fn(|_| VecDeque::new()),
        task_priorities: HashMap::new(),
    }
}

/// Called when a task is spawned at a given priority.
pub fn on_task_added(state: &mut SchedState, key: Key, priority: usize) {
    let p = priority.min(31);
    state.task_priorities.insert(key, p);
}

/// Called when a task is removed.
pub fn on_task_removed(state: &mut SchedState, key: Key) {
    if let Some(p) = state.task_priorities.remove(&key) {
        state.queues[p].retain(|&k| k != key);
        if state.queues[p].is_empty() {
            state.ready_bitmap &= !(1 << p);
        }
    }
}

/// Called when a task is woken (notified).
pub fn on_task_notified(state: &mut SchedState, key: Key) {
    let p = state.task_priorities.get(&key).copied().unwrap_or(4);
    if !state.queues[p].contains(&key) {
        state.queues[p].push_back(key);
        state.ready_bitmap |= 1 << p;
    }
}

/// Select the highest-priority ready task.
pub fn pick_next(state: &mut SchedState) -> Option<Key> {
    if state.ready_bitmap == 0 {
        return None;
    }
    // Highest set bit = highest priority with ready tasks.
    let p = 31 - state.ready_bitmap.leading_zeros() as usize;
    let key = state.queues[p].pop_front()?;
    if state.queues[p].is_empty() {
        state.ready_bitmap &= !(1 << p);
    }
    Some(key)
}

/// Called when the current task voluntarily yields.
pub fn on_yield(state: &mut SchedState, key: Key) {
    let p = state.task_priorities.get(&key).copied().unwrap_or(4);
    if !state.queues[p].contains(&key) {
        state.queues[p].push_back(key);
        state.ready_bitmap |= 1 << p;
    }
}

/// Called on timer tick. Returns true if the task should be preempted.
pub fn should_preempt(state: &mut SchedState, current_key: Key) -> bool {
    let current_p = state
        .task_priorities
        .get(&current_key)
        .copied()
        .unwrap_or(4);
    // Preempt if a higher-priority task is ready.
    let higher_mask = state.ready_bitmap & !((1 << (current_p + 1)) - 1);
    if higher_mask != 0 {
        return true;
    }
    // Always preempt on timer for round-robin within the same priority.
    true
}

/// Change a task's priority. Moves it between queues if it's currently
/// in a ready queue. Returns the old priority (or None if unknown).
pub fn change_priority(state: &mut SchedState, key: Key, new_priority: usize) -> Option<usize> {
    let new_p = new_priority.min(31);
    let old_p = state.task_priorities.insert(key, new_p)?;
    if old_p == new_p {
        return Some(old_p);
    }
    // If the task is in the old priority queue, move it to the new one.
    if let Some(pos) = state.queues[old_p].iter().position(|&k| k == key) {
        state.queues[old_p].remove(pos);
        if state.queues[old_p].is_empty() {
            state.ready_bitmap &= !(1 << old_p);
        }
        state.queues[new_p].push_back(key);
        state.ready_bitmap |= 1 << new_p;
    }
    Some(old_p)
}

/// Try to steal a task for another CPU (return lowest-priority ready task).
pub fn steal_task(state: &mut SchedState) -> Option<Key> {
    if state.ready_bitmap == 0 {
        return None;
    }
    // Steal lowest-priority task to minimize impact.
    let p = state.ready_bitmap.trailing_zeros() as usize;
    let key = state.queues[p].pop_front()?;
    if state.queues[p].is_empty() {
        state.ready_bitmap &= !(1 << p);
    }
    Some(key)
}
