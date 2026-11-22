//! Thread spawning.
//!
//! Uses plain OS threads instead of async-std's task system. The
//! async-std executor's thread pool outlives the test binary's
//! static teardown (~1/3 of runs crash in ConcurrentQueue<TimerOp>::pop
//! or similar after the test summary — issue #873, same class as
//! #852's zircon-object variant). A plain OS thread avoids that
//! entirely: the OS reclaims everything at process exit.

use alloc::sync::Arc;
use core::{any::Any, cell::RefCell, future::Future};

thread_local! {
    static CURRENT_THREAD: RefCell<Option<Arc<dyn Any + Send + Sync>>> =
        RefCell::new(None);
}

hal_fn_impl! {
    impl mod crate::hal_fn::thread {
        fn spawn(future: impl Future<Output = ()> + Send + 'static) {
            // Spawn on an OS thread; the future is driven to
            // completion by the minimal block_on below. This avoids
            // async-std's thread pool entirely (#873).
            std::thread::spawn(move || {
                super::run_executor::block_on(future);
            });
        }

        fn set_current_thread(thread: Option<Arc<dyn Any + Send + Sync>>) {
            CURRENT_THREAD.with(|t| *t.borrow_mut() = thread);
        }

        fn get_current_thread() -> Option<Arc<dyn Any + Send + Sync>> {
            CURRENT_THREAD.with(|t| t.borrow().clone())
        }
    }
}

/// Whether the current thread is servicing a syscall entered from user
/// mode. In libos mode syscalls are invoked directly from the host
/// process (no trap handler), so this is always false — matching the
/// bare-metal behavior for in-kernel callers: user-buffer copies fall
/// back to the libos VMAR validation instead of the user/kernel
/// address-space split.
pub fn user_syscall_enter() {}

/// Leave the user-syscall region (no-op in libos; see
/// [`user_syscall_enter`]).
pub fn user_syscall_leave() {}

/// Whether the current CPU is inside a user-mode syscall (always false
/// in libos mode).
pub fn in_user_syscall() -> bool {
    false
}
