//! Bare-metal executor: runs futures via the custom executor with
//! interrupt-driven scheduling.

use core::future::Future;

/// Run the executor loop until all tasks complete.
///
/// Enables the timer and interrupts, then loops running the executor
/// until idle, sleeping between iterations via wait-for-interrupt.
pub fn run_executor<F: Future<Output = i32> + Send + 'static>(future: F) -> i32 {
    // Spawn the future into the executor.
    executor::spawn(async move {
        let code = future.await;
        // Store exit code for retrieval after executor stops.
        // For bare-metal, we loop forever -- reset is called by the caller.
        unsafe {
            EXIT_CODE = code;
        }
    });

    crate::timer::timer_enable();
    log::info!("executor run!");
    crate::interrupt::intr_on();

    loop {
        let has_task = executor::run_until_idle();
        if !has_task {
            return unsafe { EXIT_CODE };
        }
        crate::interrupt::wait_for_interrupt();
    }
}

static mut EXIT_CODE: i32 = 0;
