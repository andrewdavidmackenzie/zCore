//! Process debug tests.
//!
//! Ports of Fuchsia's process debug test suite. Tests
//! zx_process_read_memory and zx_process_write_memory.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ProcessDebugTest, ReadMemoryAtInvalidOffsetReturnsErrorNotFound)
#[test]
#[ignore = "requires process debug read/write memory"]
fn process_debug_read_memory_at_invalid_offset_returns_error_not_found() {
    todo!("requires process debug read/write memory");
}

/// C++: TEST(ProcessDebugTest, ReadMemoryAtOffsetIsOk)
#[test]
#[ignore = "requires process debug read/write memory"]
fn process_debug_read_memory_at_offset_is_ok() {
    todo!("requires process debug read/write memory");
}

/// C++: TEST(ProcessDebugTest, WriteAtInvalidOffsetReturnsErrorNotFound)
#[test]
#[ignore = "requires process debug read/write memory"]
fn process_debug_write_at_invalid_offset_returns_error_not_found() {
    todo!("requires process debug read/write memory");
}

/// C++: TEST(ProcessDebugTest, WriteMemoryAtOffsetIsOk)
#[test]
#[ignore = "requires process debug read/write memory"]
fn process_debug_write_memory_at_offset_is_ok() {
    todo!("requires process debug read/write memory");
}

/// C++: TEST(ProcessDebugUtilsTest, XorShiftIsOk)
#[test]
#[ignore = "requires process debug utilities"]
fn process_debug_utils_xor_shift_is_ok() {
    todo!("requires process debug utilities");
}

/// C++: TEST(ProcessDebugVDSO, WriteToVdsoAddressReturnsAccessDenied)
#[test]
#[ignore = "requires process debug vDSO protection"]
fn process_debug_vdso_write_to_vdso_address_returns_access_denied() {
    todo!("requires process debug vDSO protection");
}
