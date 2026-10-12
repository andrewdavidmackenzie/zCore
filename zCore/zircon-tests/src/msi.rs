//! MSI (Message Signaled Interrupt) tests.
//!
//! Ports of Fuchsia's MSI test suite. Tests MSI/MSI-X allocation
//! and configuration.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(MsiTest, AllocateSyscall)
#[test]
#[ignore = "requires MSI interrupt infrastructure"]
fn allocate_syscall() {
    todo!("requires MSI interrupt infrastructure");
}

/// C++: TEST(MsiTest, AllocatedHandleRights)
#[test]
#[ignore = "requires MSI interrupt infrastructure"]
fn allocated_handle_rights() {
    todo!("requires MSI interrupt infrastructure");
}

/// C++: TEST(MsiTest, ContiguousNotSupported)
#[test]
#[ignore = "requires MSI interrupt infrastructure"]
fn contiguous_not_supported() {
    todo!("requires MSI interrupt infrastructure");
}

/// C++: TEST(MsiTest, CreateSyscallArgs)
#[test]
#[ignore = "requires MSI interrupt infrastructure"]
fn create_syscall_args() {
    todo!("requires MSI interrupt infrastructure");
}

/// C++: TEST(MsiTest, Msi)
#[test]
#[ignore = "requires MSI interrupt infrastructure"]
fn msi() {
    todo!("requires MSI interrupt infrastructure");
}

/// C++: TEST(MsiTest, Msix)
#[test]
#[ignore = "requires MSI interrupt infrastructure"]
fn msix() {
    todo!("requires MSI interrupt infrastructure");
}
