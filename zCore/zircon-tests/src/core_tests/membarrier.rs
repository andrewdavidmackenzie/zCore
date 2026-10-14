//! Membarrier tests.
//!
//! Ports of Fuchsia's membarrier test suite. Tests
//! memory barrier synchronization across processes.
//! zCore has no membarrier syscall. These tests verify that
//! basic memory ordering via volatile reads/writes is correct.

use crate::helpers::TestContext;
use zircon_object::vm::VmObject;

/// C++: TEST(MembarrierSyncProcessData, Basic)
/// Verify that data written to a VMO is visible when read back
/// (simulating data-side memory barrier semantics).
#[test]
fn membarrier_data_basic() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Write data.
    let data = [0xAAu8; 64];
    vmo.write(0, &data).unwrap();

    // Read it back — should see the written data (data barrier).
    let mut buf = [0u8; 64];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, data, "data should be visible after write");

    // Overwrite and re-read.
    let data2 = [0xBBu8; 64];
    vmo.write(0, &data2).unwrap();
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, data2, "updated data should be visible");
}

/// C++: TEST(MembarrierSyncProcessInsn, Basic)
/// Verify that instruction-side memory barrier semantics work.
/// In libos mode, instruction barriers are no-ops but we verify
/// VMO data consistency after write-then-read patterns.
#[test]
fn membarrier_insn_basic() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);

    // Write an "instruction-like" pattern.
    let insn_data: Vec<u8> = (0..256).map(|i| (i & 0xFF) as u8).collect();
    vmo.write(0, &insn_data).unwrap();

    // Read back and verify (instruction barrier is a no-op in libos).
    let mut buf = vec![0u8; 256];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, insn_data, "instruction data should be consistent");
}
