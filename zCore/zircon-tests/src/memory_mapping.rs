//! Memory mapping tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/memory-mapping tests.
//! Memory mapping operations (mmap-style) require real VMAR mappings
//! and user-space page faults. Not supported in libos test mode.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Map a VMO into the process VMAR and verify the virtual address
/// is valid. Requires real VMAR mapping infrastructure.
#[test]
#[ignore = "memory mapping needs real VMAR with page fault handling"]
fn basic_mapping() {
    // Requires real VMAR mapping infrastructure.
}

///
/// Map a VMO read-only and read through the mapping. Verifies data
/// written via VMO is visible through the mapping. Requires real VMAR.
#[test]
#[ignore = "memory mapping needs real VMAR with page fault handling"]
fn read_mapping() {
    // Requires real VMAR mapping infrastructure.
}

///
/// Map a VMO read-write and write through the mapping. Verifies
/// data is visible via VMO read. Requires real VMAR mapping.
#[test]
#[ignore = "memory mapping needs real VMAR with page fault handling"]
fn write_mapping() {
    // Requires real VMAR mapping infrastructure.
}

///
/// Unmap a previously mapped region. Accessing the unmapped address
/// should cause a fault. Requires real VMAR mapping.
#[test]
#[ignore = "memory mapping needs real VMAR with page fault handling"]
fn unmap_mapping() {
    // Requires real VMAR mapping infrastructure.
}

///
/// Change the protection of a mapped region (e.g., from RW to RO).
/// Writing to a protected-RO mapping should fault. Requires real VMAR.
#[test]
#[ignore = "memory mapping needs real VMAR with page fault handling"]
fn protect_mapping() {
    // Requires real VMAR mapping infrastructure.
}

///
/// Map the same VMO into two VMAR regions. Writes through one
/// mapping should be visible through the other. Requires real VMAR.
#[test]
#[ignore = "memory mapping needs real VMAR with page fault handling"]
fn shared_mapping() {
    // Requires real VMAR mapping infrastructure.
}

///
/// Create a private (COW) mapping of a VMO. Writes through the
/// mapping should not affect the original VMO. Requires real VMAR.
#[test]
#[ignore = "memory mapping needs real VMAR with page fault handling"]
fn private_mapping() {
    // Requires real VMAR mapping infrastructure.
}

///
/// Verify that mapping permission flags (READ, WRITE, EXECUTE) are
/// enforced correctly. Accessing without permission should fault.
/// Requires real VMAR mapping infrastructure.
#[test]
#[ignore = "memory mapping needs real VMAR with page fault handling"]
fn mapping_permissions() {
    // Requires real VMAR mapping infrastructure.
}

/// C++: TEST(MemoryMappingTest, AddressSpaceLimitsTest)
#[test]
#[ignore = "requires mmap/VMAR mapping infrastructure"]
fn address_space_limits_test() {
    todo!("requires mmap/VMAR mapping infrastructure");
}

/// C++: TEST(MemoryMappingTest, MmapFlagsTest)
#[test]
#[ignore = "requires mmap/VMAR mapping infrastructure"]
fn mmap_flags_test() {
    todo!("requires mmap/VMAR mapping infrastructure");
}

/// C++: TEST(MemoryMappingTest, MmapLenTest)
#[test]
#[ignore = "requires mmap/VMAR mapping infrastructure"]
fn mmap_len_test() {
    todo!("requires mmap/VMAR mapping infrastructure");
}

/// C++: TEST(MemoryMappingTest, MmapOffsetTest)
#[test]
#[ignore = "requires mmap/VMAR mapping infrastructure"]
fn mmap_offset_test() {
    todo!("requires mmap/VMAR mapping infrastructure");
}

/// C++: TEST(MemoryMappingTest, MmapProtExecTest)
#[test]
#[ignore = "requires mmap/VMAR mapping infrastructure"]
fn mmap_prot_exec_test() {
    todo!("requires mmap/VMAR mapping infrastructure");
}

/// C++: TEST(MemoryMappingTest, MmapProtTest)
#[test]
#[ignore = "requires mmap/VMAR mapping infrastructure"]
fn mmap_prot_test() {
    todo!("requires mmap/VMAR mapping infrastructure");
}

/// C++: TEST(MemoryMappingTest, MmapZerofilledTest)
#[test]
#[ignore = "requires mmap/VMAR mapping infrastructure"]
fn mmap_zerofilled_test() {
    todo!("requires mmap/VMAR mapping infrastructure");
}

/// C++: TEST(MemoryMappingTest, MprotectTest)
#[test]
#[ignore = "requires mmap/VMAR mapping infrastructure"]
fn mprotect_test() {
    todo!("requires mmap/VMAR mapping infrastructure");
}
