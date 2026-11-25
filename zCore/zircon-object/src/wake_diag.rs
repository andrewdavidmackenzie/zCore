//! #862 wake-storm diagnostics.
//!
//! Reproduced hangs show a task's waker slot receiving tens of
//! thousands of wakes per second (one slot reached 1.85M wakes while
//! the timer only expired 90k times — so the storm is event-source
//! driven, not timer driven) before the system goes quiescent and
//! hangs forever. This module names the kernel path that fires the
//! wakers: every fire passes through `log_wake_fire()`, which logs
//! its caller every 50,000 calls — during a storm the caller of the
//! storming path dominates the samples.

use core::panic::Location;
use core::sync::atomic::{AtomicU64, Ordering};

/// Log (every 50k calls) the caller that fires a waker.
#[track_caller]
pub fn log_wake_fire() {
    static FIRES: AtomicU64 = AtomicU64::new(0);
    let n = FIRES.fetch_add(1, Ordering::Relaxed) + 1;
    if n.is_multiple_of(50_000) {
        error!("[wake-fire-storm] fire #{} from {}", n, Location::caller());
    }
}
