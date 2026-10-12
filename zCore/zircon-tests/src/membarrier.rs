//! Membarrier tests.
//!
//! Ports of Fuchsia's membarrier test suite. Tests
//! memory barrier synchronization across processes.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(MembarrierSyncProcessData, Basic)
#[test]
#[ignore = "requires membarrier infrastructure"]
fn membarrier_data_basic() {
    todo!("requires membarrier infrastructure");
}

/// C++: TEST(MembarrierSyncProcessInsn, Basic)
#[test]
#[ignore = "requires membarrier infrastructure"]
fn membarrier_insn_basic() {
    todo!("requires membarrier infrastructure");
}
