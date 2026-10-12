//! Memory mapping tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/memory-mapping tests.
//! Memory mapping operations (mmap-style) require real VMAR mappings
//! and user-space page faults. Not supported in libos test mode.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(MemoryMappingTest, BasicMapping)
///
/// Map a VMO into the process VMAR and verify the virtual address
/// is valid. Requires real VMAR mapping infrastructure.
#[should_panic(expected = "memory mapping needs real VMAR")]
#[test]
fn basic_mapping() {
    let _ctx = TestContext::new();
    panic!("memory mapping needs real VMAR");
}

/// C++: TEST(MemoryMappingTest, ReadMapping)
///
/// Map a VMO read-only and read through the mapping. Verifies data
/// written via VMO is visible through the mapping. Requires real VMAR.
#[should_panic(expected = "memory mapping needs real VMAR")]
#[test]
fn read_mapping() {
    let _ctx = TestContext::new();
    panic!("memory mapping needs real VMAR");
}

/// C++: TEST(MemoryMappingTest, WriteMapping)
///
/// Map a VMO read-write and write through the mapping. Verifies
/// data is visible via VMO read. Requires real VMAR mapping.
#[should_panic(expected = "memory mapping needs real VMAR")]
#[test]
fn write_mapping() {
    let _ctx = TestContext::new();
    panic!("memory mapping needs real VMAR");
}

/// C++: TEST(MemoryMappingTest, UnmapMapping)
///
/// Unmap a previously mapped region. Accessing the unmapped address
/// should cause a fault. Requires real VMAR mapping.
#[should_panic(expected = "memory mapping needs real VMAR")]
#[test]
fn unmap_mapping() {
    let _ctx = TestContext::new();
    panic!("memory mapping needs real VMAR");
}

/// C++: TEST(MemoryMappingTest, ProtectMapping)
///
/// Change the protection of a mapped region (e.g., from RW to RO).
/// Writing to a protected-RO mapping should fault. Requires real VMAR.
#[should_panic(expected = "memory mapping needs real VMAR")]
#[test]
fn protect_mapping() {
    let _ctx = TestContext::new();
    panic!("memory mapping needs real VMAR");
}

/// C++: TEST(MemoryMappingTest, SharedMapping)
///
/// Map the same VMO into two VMAR regions. Writes through one
/// mapping should be visible through the other. Requires real VMAR.
#[should_panic(expected = "memory mapping needs real VMAR")]
#[test]
fn shared_mapping() {
    let _ctx = TestContext::new();
    panic!("memory mapping needs real VMAR");
}

/// C++: TEST(MemoryMappingTest, PrivateMapping)
///
/// Create a private (COW) mapping of a VMO. Writes through the
/// mapping should not affect the original VMO. Requires real VMAR.
#[should_panic(expected = "memory mapping needs real VMAR")]
#[test]
fn private_mapping() {
    let _ctx = TestContext::new();
    panic!("memory mapping needs real VMAR");
}

/// C++: TEST(MemoryMappingTest, MappingPermissions)
///
/// Verify that mapping permission flags (READ, WRITE, EXECUTE) are
/// enforced correctly. Accessing without permission should fault.
/// Requires real VMAR mapping infrastructure.
#[should_panic(expected = "memory mapping needs real VMAR")]
#[test]
fn mapping_permissions() {
    let _ctx = TestContext::new();
    panic!("memory mapping needs real VMAR");
}
