//! Thread spawning.

use alloc::sync::Arc;
use async_std::task_local;
use core::{any::Any, cell::RefCell, future::Future};

task_local! {
    static CURRENT_THREAD: RefCell<Option<Arc<dyn Any + Send + Sync>>> = RefCell::new(None);
}

hal_fn_impl! {
    impl mod crate::hal_fn::thread {
        fn spawn(future: impl Future<Output = ()> + Send + 'static) {
            async_std::task::spawn(future);
        }

        fn set_current_thread(thread: Option<Arc<dyn Any + Send + Sync>>) {
            CURRENT_THREAD.with(|t| *t.borrow_mut() = thread);
        }

        fn get_current_thread() -> Option<Arc<dyn Any + Send + Sync>> {
            CURRENT_THREAD.try_with(|t| {
                t.borrow().as_ref().cloned()
            }).unwrap_or(None)
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
