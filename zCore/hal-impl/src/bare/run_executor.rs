//! Bare-metal executor: runs futures via the custom executor with
//! interrupt-driven scheduling.

use alloc::sync::Arc;
use core::future::Future;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static EXIT_CODE: AtomicI32 = AtomicI32::new(0);

/// Run the executor loop until the wrapped future completes.
///
/// Enables the timer and interrupts, then loops running the executor
/// until the future completes, sleeping between iterations via
/// wait-for-interrupt.
///
/// Returns only on the CPU whose own future completed. When another
/// CPU's future completes (global shutdown — e.g. the init process
/// exited on the primary core while this core runs a never-completing
/// service loop), this CPU parks instead: only the completing CPU
/// reports the result and resets the machine, so a secondary core
/// can never cut off the completion report (or reset first).
///
/// The completion flag is per-call (not the global `EXIT_CODE`)
/// because `run_until_idle` must otherwise keep looping while this
/// CPU still has parked tasks: it spins inside the executor's idle
/// hlt until every task is gone, and a task whose wake-up was lost
/// (an issue #834 zombie) parks forever, hanging shutdown.
pub fn run_executor<F: Future<Output = i32> + Send + 'static>(future: F) -> i32 {
    let exited = Arc::new(AtomicBool::new(false));
    let exited_flag = exited.clone();
    // Spawn at the same priority as Zircon threads (16) so the
    // test runner isn't starved by spawned thread tasks.
    executor::spawn_with_priority(
        async move {
            let code = future.await;
            EXIT_CODE.store(code, Ordering::Relaxed);
            exited_flag.store(true, Ordering::Release);
            // Regain control on every CPU: run_until_idle loops would
            // otherwise spin inside their executors' idle hlt while
            // parked tasks remain (issue #834 zombie tasks).
            executor::request_shutdown();
        },
        16,
    );

    crate::timer::timer_enable();
    log::info!("executor run!");
    crate::interrupt::intr_on();

    loop {
        executor::run_until_idle();
        // Deferred-wake drain (#523/#829): timer callbacks defer their
        // wakes; drain them here — this level iterates even when the
        // system is fully idle (the Executor::run coroutine parks via
        // sched_yield once every task is parked, so its in-loop drain
        // stops; without this level's drain, deferred wakes waited for
        // the next executor activation — sleep-poll loops like
        // sys_debug_exec's degraded from ~1ms to executor-resume
        // latency, and fully-idle systems crawled at timer-tick rate).
        crate::timer::drain_deferred_timer_callbacks();
        if exited.load(Ordering::Acquire) {
            return EXIT_CODE.load(Ordering::Relaxed);
        }
        if executor::shutdown_requested() {
            // Another CPU's future completed and this one's never
            // will — park rather than return, so only the completing
            // CPU reports and resets.
            loop {
                crate::interrupt::wait_for_interrupt();
            }
        }
        crate::interrupt::wait_for_interrupt();
    }
}
