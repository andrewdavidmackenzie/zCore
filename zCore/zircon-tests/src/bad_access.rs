//! Bad access tests.
//!
//! Ports of Fuchsia's bad-access test suite. Tests that invalid
//! memory accesses are properly rejected by syscalls.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(BadAccessTest, ChannelReadHandle)
#[test]
#[ignore = "requires user-space memory fault testing"]
fn channel_read_handle() {
    todo!("requires user-space memory fault testing");
}

/// C++: TEST(BadAccessTest, InvalidMappedAddressFails)
#[test]
#[ignore = "requires user-space memory fault testing"]
fn invalid_mapped_address_fails() {
    todo!("requires user-space memory fault testing");
}

/// C++: TEST(BadAccessTest, KernelMappedAddressChannelWriteFails)
#[test]
#[ignore = "requires user-space memory fault testing"]
fn kernel_mapped_address_channel_write_fails() {
    todo!("requires user-space memory fault testing");
}

/// C++: TEST(BadAccessTest, NormalMappedAddressChannelWriteSucceeds)
#[test]
#[ignore = "requires user-space memory fault testing"]
fn normal_mapped_address_channel_write_succeeds() {
    todo!("requires user-space memory fault testing");
}

/// C++: TEST(BadAccessTest, SyscallNumTest)
#[test]
#[ignore = "requires user-space memory fault testing"]
fn syscall_num_test() {
    todo!("requires user-space memory fault testing");
}
