//! Bare-metal executor: runs futures via the custom executor with
//! interrupt-driven scheduling.

use core::future::Future;
use core::sync::atomic::{AtomicI32, Ordering};

static EXIT_CODE: AtomicI32 = AtomicI32::new(0);

/// Run the executor loop until all tasks complete.
///
/// Enables the timer and interrupts, then loops running the executor
/// until idle, sleeping between iterations via wait-for-interrupt.
pub fn run_executor<F: Future<Output = i32> + Send + 'static>(future: F) -> i32 {
    // Spawn the future into the executor.
    executor::spawn(async move {
        let code = future.await;
        EXIT_CODE.store(code, Ordering::Relaxed);
    });

    crate::timer::timer_enable();
    log::info!("executor run!");
    crate::interrupt::intr_on();

    loop {
        let has_task = executor::run_until_idle();
        if !has_task {
            return EXIT_CODE.load(Ordering::Relaxed);
        }
        crate::interrupt::wait_for_interrupt();
    }
}
