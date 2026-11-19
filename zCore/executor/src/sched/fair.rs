//! Weighted Fair Queue (WFQ) scheduler with deadline support.
//!
//! **Fair tasks** are ordered by virtual runtime — a measure of how much
//! CPU time they've consumed, scaled inversely by their weight. Higher-
//! weight tasks accumulate virtual runtime more slowly and therefore
//! get more real CPU time, achieving proportional sharing.
//!
//! **Deadline tasks** are ordered by relative deadline (shortest first).
//! They always preempt fair tasks, implementing earliest-deadline-first
//! (EDF) scheduling among deadline tasks.
//!
//! Selection is O(log n) via BTreeMap for both fair and deadline tasks.

use crate::task_collection::{Key, MAX_PRIORITY};
use alloc::collections::BTreeMap;
use hashbrown::HashMap;

/// Reference weight for virtual runtime scaling (priority 31).
const REFERENCE_WEIGHT: u64 = 1024;

/// Weight table: maps priority 0-31 to fixed-point weight.
/// `weight = round(1024 * 1.1^(priority - 31))`
/// Must match the table in zircon-object/src/task/thread.rs.
const WEIGHT_TABLE: [u64; 32] = [
    53, 59, 65, 71, 78, 86, 95, 104, 114, 126, 138, 152, 167, 184, 203, 223, 245, 270, 297, 326,
    359, 395, 434, 478, 525, 578, 636, 699, 769, 846, 931, 1024,
];

/// Scheduling class: fair (weighted) or deadline (EDF).
#[derive(Clone, Copy, PartialEq, Eq)]
enum SchedClass {
    /// Weighted fair queue — ordered by virtual runtime.
    Fair,
    /// Earliest deadline first — ordered by relative deadline.
    Deadline {
        /// Relative deadline in nanoseconds (shorter = more urgent).
        relative_deadline_ns: u64,
    },
}

/// Per-task scheduling state.
struct TaskState {
    /// Current priority level (0-31).
    priority: usize,
    /// Scheduling class (fair or deadline).
    sched_class: SchedClass,
    /// Virtual runtime (fair tasks only): accumulated weighted CPU usage.
    vruntime: u64,
    /// Whether the task is currently in a ready tree.
    in_ready_tree: bool,
}

/// Per-CPU scheduler state.
pub struct SchedState {
    /// Fair ready tasks ordered by (vruntime, key).
    fair_tree: BTreeMap<(u64, Key), Key>,
    /// Deadline ready tasks ordered by (relative_deadline, key).
    /// Deadline tasks always preempt fair tasks.
    deadline_tree: BTreeMap<(u64, Key), Key>,
    /// Per-task state.
    tasks: HashMap<Key, TaskState>,
    /// Minimum vruntime of any ready fair task.
    min_vruntime: u64,
}

/// Create a new per-CPU scheduler state.
pub fn new(_cpu_id: u8) -> SchedState {
    SchedState {
        fair_tree: BTreeMap::new(),
        deadline_tree: BTreeMap::new(),
        tasks: HashMap::new(),
        min_vruntime: 0,
    }
}

/// Called when a task is spawned at a given priority.
pub fn on_task_added(state: &mut SchedState, key: Key, priority: usize) {
    let p = priority.min(31);
    state.tasks.insert(
        key,
        TaskState {
            priority: p,
            sched_class: SchedClass::Fair,
            vruntime: state.min_vruntime,
            in_ready_tree: false,
        },
    );
}

/// Called when a task is removed (dead/dropped).
pub fn on_task_removed(state: &mut SchedState, key: Key) {
    if let Some(ts) = state.tasks.remove(&key) {
        if ts.in_ready_tree {
            match ts.sched_class {
                SchedClass::Fair => {
                    state.fair_tree.remove(&(ts.vruntime, key));
                }
                SchedClass::Deadline {
                    relative_deadline_ns,
                } => {
                    state.deadline_tree.remove(&(relative_deadline_ns, key));
                }
            }
        }
    }
}

/// Look up a task's CURRENT scheduling priority. The key's priority
/// bits encode the spawn priority and can be stale after
/// `change_priority`; this reads the live mapping. Returns
/// `MAX_PRIORITY` for unknown keys (treated as non-migratable).
pub fn current_priority(state: &SchedState, key: Key) -> usize {
    state
        .tasks
        .get(&key)
        .map(|ts| ts.priority)
        .unwrap_or(MAX_PRIORITY)
}

/// Called when a task is woken (notification fired).
pub fn on_task_notified(state: &mut SchedState, key: Key) {
    if let Some(ts) = state.tasks.get_mut(&key) {
        if !ts.in_ready_tree {
            match ts.sched_class {
                SchedClass::Fair => {
                    if ts.vruntime < state.min_vruntime {
                        ts.vruntime = state.min_vruntime;
                    }
                    state.fair_tree.insert((ts.vruntime, key), key);
                }
                SchedClass::Deadline {
                    relative_deadline_ns,
                } => {
                    state.deadline_tree.insert((relative_deadline_ns, key), key);
                }
            }
            ts.in_ready_tree = true;
        }
    } else {
        // Unknown task — assign default fair priority 4.
        let vruntime = state.min_vruntime;
        state.tasks.insert(
            key,
            TaskState {
                priority: 4,
                sched_class: SchedClass::Fair,
                vruntime,
                in_ready_tree: true,
            },
        );
        state.fair_tree.insert((vruntime, key), key);
    }
}

/// Select the next task to run.
/// Deadline tasks are always preferred over fair tasks (EDF).
/// Among fair tasks, the one with the lowest virtual runtime is chosen.
pub fn pick_next(state: &mut SchedState) -> Option<Key> {
    // Priority 1: deadline tasks (earliest deadline first)
    if let Some((&(deadline, key), _)) = state.deadline_tree.iter().next() {
        state.deadline_tree.remove(&(deadline, key));
        if let Some(ts) = state.tasks.get_mut(&key) {
            ts.in_ready_tree = false;
        }
        return Some(key);
    }

    // Priority 2: fair tasks (lowest virtual runtime)
    let (&(vruntime, key), _) = state.fair_tree.iter().next()?;
    state.fair_tree.remove(&(vruntime, key));

    if let Some(ts) = state.tasks.get_mut(&key) {
        ts.in_ready_tree = false;
        let weight = WEIGHT_TABLE[ts.priority];
        let increment = REFERENCE_WEIGHT * 1_000_000 / weight;
        ts.vruntime = ts.vruntime.saturating_add(increment);
    }

    // Update min_vruntime
    if let Some(&(next_min, _)) = state.fair_tree.keys().next() {
        state.min_vruntime = state.min_vruntime.max(next_min);
    } else {
        state.min_vruntime = state.min_vruntime.max(vruntime);
    }

    Some(key)
}

/// Re-queue a task that voluntarily yielded.
pub fn on_yield(state: &mut SchedState, key: Key) {
    if let Some(ts) = state.tasks.get_mut(&key) {
        if !ts.in_ready_tree {
            match ts.sched_class {
                SchedClass::Fair => {
                    state.fair_tree.insert((ts.vruntime, key), key);
                }
                SchedClass::Deadline {
                    relative_deadline_ns,
                } => {
                    state.deadline_tree.insert((relative_deadline_ns, key), key);
                }
            }
            ts.in_ready_tree = true;
        }
    }
}

/// Check if a higher-priority (lower-vruntime) task should preempt
/// the current task. Returns true if there's a ready task with lower
/// vruntime than the current task.
pub fn should_preempt(state: &mut SchedState, current_key: Key) -> bool {
    // A deadline task always preempts a fair task.
    if let Some(ts) = state.tasks.get(&current_key) {
        // If there's a ready deadline task and current is fair, preempt.
        if ts.sched_class == SchedClass::Fair && !state.deadline_tree.is_empty() {
            return true;
        }
        // Among fair tasks, preempt if a lower-vruntime task is ready.
        if let SchedClass::Fair = ts.sched_class {
            if let Some(&(min_vruntime, _)) = state.fair_tree.keys().next() {
                return min_vruntime < ts.vruntime;
            }
        }
        // Among deadline tasks, preempt if an earlier-deadline task is ready.
        if let SchedClass::Deadline {
            relative_deadline_ns,
        } = ts.sched_class
        {
            if let Some(&(min_deadline, _)) = state.deadline_tree.keys().next() {
                return min_deadline < relative_deadline_ns;
            }
        }
    }
    false
}

/// Change a task's priority. Returns the old priority.
pub fn change_priority(state: &mut SchedState, key: Key, new_priority: usize) -> Option<usize> {
    let new_p = new_priority.min(31);
    if let Some(ts) = state.tasks.get_mut(&key) {
        let old = ts.priority;
        if ts.in_ready_tree {
            // Remove from current tree
            match ts.sched_class {
                SchedClass::Fair => {
                    state.fair_tree.remove(&(ts.vruntime, key));
                }
                SchedClass::Deadline {
                    relative_deadline_ns,
                } => {
                    state.deadline_tree.remove(&(relative_deadline_ns, key));
                }
            }
            ts.priority = new_p;
            // Re-insert in appropriate tree
            match ts.sched_class {
                SchedClass::Fair => {
                    state.fair_tree.insert((ts.vruntime, key), key);
                }
                SchedClass::Deadline {
                    relative_deadline_ns,
                } => {
                    state.deadline_tree.insert((relative_deadline_ns, key), key);
                }
            }
        } else {
            ts.priority = new_p;
        }
        Some(old)
    } else {
        None
    }
}

/// Select the least-important ready task (for work stealing).
/// For the fair scheduler, this is the highest-vruntime fair task.
/// Deadline tasks are never stolen.
pub fn pick_lowest(state: &mut SchedState) -> Option<Key> {
    let (&(vruntime, key), _) = state.fair_tree.iter().next_back()?;
    state.fair_tree.remove(&(vruntime, key));
    if let Some(ts) = state.tasks.get_mut(&key) {
        ts.in_ready_tree = false;
    }
    Some(key)
}

/// Set a task's scheduling class to deadline with the given parameters.
/// Called when a thread has a deadline profile applied.
pub fn set_deadline(state: &mut SchedState, key: Key, relative_deadline_ns: u64) {
    if let Some(ts) = state.tasks.get_mut(&key) {
        // Remove from current tree if ready
        if ts.in_ready_tree {
            match ts.sched_class {
                SchedClass::Fair => {
                    state.fair_tree.remove(&(ts.vruntime, key));
                }
                SchedClass::Deadline {
                    relative_deadline_ns: old_dl,
                } => {
                    state.deadline_tree.remove(&(old_dl, key));
                }
            }
        }
        // Switch to deadline class
        ts.sched_class = SchedClass::Deadline {
            relative_deadline_ns,
        };
        // Re-insert if was ready
        if ts.in_ready_tree {
            state.deadline_tree.insert((relative_deadline_ns, key), key);
        }
    }
}

/// Set a task's scheduling class back to fair.
pub fn set_fair(state: &mut SchedState, key: Key) {
    if let Some(ts) = state.tasks.get_mut(&key) {
        if let SchedClass::Deadline {
            relative_deadline_ns,
        } = ts.sched_class
        {
            if ts.in_ready_tree {
                state.deadline_tree.remove(&(relative_deadline_ns, key));
            }
            ts.sched_class = SchedClass::Fair;
            ts.vruntime = state.min_vruntime;
            if ts.in_ready_tree {
                state.fair_tree.insert((ts.vruntime, key), key);
            }
        }
    }
}
