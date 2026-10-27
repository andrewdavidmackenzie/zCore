//! Event, EventPair, Timer, and Object signal syscall-level tests.

use crate::helpers::TestContext;
use hal_impl::user::UserOutPtr;
use zircon_object::object::HandleValue;

// -- Remaining EventPair faithful 1:1 ports via syscall dispatch --

/// Helper: create eventpair via syscall and return handle values.
pub(crate) fn syscall_eventpair_create(ctx: &TestContext) -> (HandleValue, HandleValue) {
    let sc = ctx.syscall();
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    sc.sys_eventpair_create(
        0,
        UserOutPtr::from(&mut h0 as *mut _ as usize),
        UserOutPtr::from(&mut h1 as *mut _ as usize),
    )
    .unwrap();
    (h0, h1)
}
