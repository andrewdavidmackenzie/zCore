//! Strict-priority round-robin scheduler with priority-dependent quanta.
//!
//! Tasks are grouped by priority level (0-31, higher = more important).
//! `pick_next()` always returns the highest-priority ready task.
//! Within the same priority level, tasks are served round-robin.
//!
//! Time slice quanta vary by priority (set in `thread.rs` via the
//! Fuchsia weight table). This controls how long a thread runs before
//! yielding to same-priority peers, but does NOT provide cross-priority
//! proportional sharing — higher-priority threads always preempt lower.

use crate::task_collection::{Key, MAX_PRIORITY};
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

/// Look up a task's CURRENT scheduling priority. The key's priority
/// bits encode the spawn priority and can be stale after
/// `change_priority`; this reads the live mapping. Returns
/// `MAX_PRIORITY` for unknown keys (treated as non-migratable).
pub fn current_priority(state: &SchedState, key: Key) -> usize {
    state
        .task_priorities
        .get(&key)
        .copied()
        .unwrap_or(MAX_PRIORITY)
}

/// Called when a task is woken (notified).
pub fn on_task_notified(state: &mut SchedState, key: Key) {
    // Only enqueue tasks that are known to the scheduler. After a task
    // is stolen (on_task_removed), stale notifications from the old
    // waker page may arrive — ignore them.
    let p = match state.task_priorities.get(&key).copied() {
        Some(p) => p,
        None => return,
    };
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

/// Select the lowest-priority ready task (for work stealing).
/// Stealing the lowest-priority task minimizes disruption to the
/// source CPU — high-priority system tasks stay where they are.
pub fn pick_lowest(state: &mut SchedState) -> Option<Key> {
    if state.ready_bitmap == 0 {
        return None;
    }
    // Lowest set bit = lowest priority with ready tasks.
    let p = state.ready_bitmap.trailing_zeros() as usize;
    let key = state.queues[p].pop_front()?;
    if state.queues[p].is_empty() {
        state.ready_bitmap &= !(1 << p);
    }
    Some(key)
}

/// Stub: set a task as deadline (no-op in priority scheduler — deadline
/// tasks are just priority 31).
pub fn set_deadline(_state: &mut SchedState, _key: Key, _relative_deadline_ns: u64) {}

/// Stub: set a task as fair (no-op in priority scheduler).
pub fn set_fair(_state: &mut SchedState, _key: Key) {}

/// #862/#829 diagnostics: human-readable scheduler state for the
/// stuck-task dumper.
pub fn debug_summary(state: &SchedState) -> alloc::string::String {
    use alloc::format;
    let mut s = format!("ready_bitmap={:#x}", state.ready_bitmap);
    for p in 0..32 {
        if !state.queues[p].is_empty() {
            s += &format!(" q[{}]={:?}", p, state.queues[p]);
        }
    }
    let mut keys: alloc::vec::Vec<Key> = state.task_priorities.keys().copied().collect();
    keys.sort_unstable();
    s += &format!(" known_keys={:?}", keys);
    s
}
