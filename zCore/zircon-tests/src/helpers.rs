//! Test helper: creates a minimal Zircon process/thread context
//! for dispatching syscalls in libos mode.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::{Handle, Rights};
use zircon_object::task::{CurrentThread, Job, Process, Thread};

extern crate alloc;

type HandleValue = u32;

/// One-time HAL initialization for libos mode.
static INIT: std::sync::Once = std::sync::Once::new();

pub fn ensure_hal_init() {
    INIT.call_once(|| {
        hal_impl::init();
    });
}

/// Thread function placeholder for the Syscall struct.
/// In test mode, we never actually spawn user threads through this.
fn dummy_thread_fn(_thread: CurrentThread) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async {})
}

/// A test context providing a Zircon process and current thread.
pub struct TestContext {
    pub proc: Arc<Process>,
    pub current: CurrentThread,
}

impl TestContext {
    /// Create a minimal Zircon process/thread context for testing.
    pub fn new() -> Self {
        ensure_hal_init();
        let job = Job::root();
        let proc = Process::create(&job, "test-proc").unwrap();
        let thread = Thread::create(&proc, "test-thread").unwrap();
        let current = CurrentThread::new_for_test(thread);
        TestContext { proc, current }
    }

    /// Construct a Syscall dispatcher for this context.
    pub fn syscall(&self) -> zircon_syscall::Syscall<'_> {
        zircon_syscall::Syscall {
            thread: &self.current,
            thread_fn: dummy_thread_fn,
        }
    }

    /// Create a root resource and add it to the process handle table.
    ///
    /// Returns the handle value. The root resource validates as any
    /// resource kind and bypasses range checks, matching the behavior
    /// of the root resource created during Zircon boot.
    pub fn add_root_resource(&self) -> HandleValue {
        let resource = Resource::create(
            "root",
            ResourceKind::ROOT,
            0,
            0x1_0000_0000, // 4 GiB range, same as boot path
            ResourceFlags::empty(),
        );
        self.proc
            .add_handle(Handle::new(resource, Rights::DEFAULT_RESOURCE))
    }
}
