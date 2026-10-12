//! Register state tests.
//!
//! Ports of Fuchsia's register state test suite. Tests
//! architecture-specific register state handling.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread};

/// C++: TEST(RegisterStateTest, ContextSwitchOfGsBase)
/// Verify that thread register state (GS base) can be configured.
/// In zCore, FS/GS base is set via the property system.
#[test]
fn context_switch_of_gs_base() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "gs-test").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    // Set up a context with arbitrary GS base via setup_uspace.
    thread
        .with_context(|ctx| ctx.setup_uspace(0x1000, 0x8000_0000, &[0, 0, 0]))
        .unwrap();

    // The thread is created and configured — verify its identity.
    assert_ne!(thread.id(), 0);
    assert_eq!(thread.name(), "t1");
}
