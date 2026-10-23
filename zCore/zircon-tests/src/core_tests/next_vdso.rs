//! Next vDSO tests.
//!
//! Ports of Fuchsia's next-vDSO test suite. Tests the next-generation
//! vDSO interface. zCore has no vDSO. This test verifies that
//! process VDSO base property returns 0 (no vDSO mapped).

use crate::helpers::TestContext;
use zircon_object::object::{Handle, Rights};

/// C++: TEST(NextVdsoTest, BasicTest)
/// Verify that the VDSO base address property is queryable.
/// In zCore test mode, no vDSO is mapped so base should be 0.
#[test]
fn basic_test() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(Handle::new(
        ctx.proc.clone(),
        Rights::INSPECT | Rights::GET_PROPERTY,
    ));

    let mut buf = [0u8; core::mem::size_of::<usize>()];
    let sc = ctx.syscall();
    sc.sys_object_get_property(proc_h, 6, buf.as_mut_ptr() as usize, buf.len())
        .unwrap();
    let vdso_base = usize::from_ne_bytes(buf);
    assert_eq!(vdso_base, 0, "no vDSO mapped in test mode");
}
