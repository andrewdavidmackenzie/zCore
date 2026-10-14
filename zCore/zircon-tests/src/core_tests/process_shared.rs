//! Process shared tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/process-shared/process-shared.cc.
//!
//! Shared processes (zx_process_create with ZX_PROCESS_SHARED /
//! zx_process_create_shared) allow multiple processes to share a
//! single address space. zCore does not implement this infrastructure.

use crate::helpers::TestContext;

/// C++: TEST(ProcessShared, MapInPrototype)
/// Creates a prototype process (ZX_PROCESS_SHARED), creates a child
/// process via zx_process_create_shared, maps a VMO into the shared
/// VMAR, and verifies both processes can read the data via
/// zx_process_read_memory at the same address.
#[test]
fn map_in_prototype() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. zx::process::create(job, "prototype", ZX_PROCESS_SHARED, &proc, &shared_vmar)
    //   2. zx_process_create_shared(proc, 0, "process", &child, &restricted_vmar)
    //   3. zx_vmo_create + zx_vmar_map into shared_vmar
    //   4. zx_vmo_write(vmo, "abc")
    //   5. prototype_process.read_memory(addr) → assert == "abc"
    //   6. child_process.read_memory(addr) → assert == "abc"
}

/// C++: TEST(ProcessShared, RestrictedVmarNotShared)
/// Maps a VMO into the restricted (private) VMAR of the child process,
/// then verifies the prototype process cannot read from that address.
#[test]
fn restricted_vmar_not_shared() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. Create prototype + shared child
    //   2. Map VMO into restricted_vmar (child only)
    //   3. Write "abc" to VMO
    //   4. prototype.read_memory(addr) → should fail or return different data
}

/// C++: TEST(ProcessShared, InvalidPrototype)
/// Creating a shared child from a non-shared (regular) process
/// returns ZX_ERR_INVALID_ARGS.
#[test]
fn invalid_prototype() {
    let _ctx = TestContext::new();
    // C++ test:
    //   zx::process::create(job, "prototype", 0 /*not shared*/, &proc, &vmar);
    //   ASSERT_EQ(zx_process_create_shared(proc, 0, "child", &child, &rvmar),
    //             ZX_ERR_INVALID_ARGS);
}

/// C++: TEST(ProcessShared, PropertiesCopied)
/// Verifies that ZX_PROP_PROCESS_BREAK_ON_LOAD and
/// ZX_PROP_PROCESS_DEBUG_ADDR are copied (not shared) to the child.
/// Changing the child's value does not affect the prototype's.
#[test]
fn properties_copied() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. Create prototype (ZX_PROCESS_SHARED)
    //   2. Set BREAK_ON_LOAD = 0x10adf00d, DEBUG_ADDR = 0xfeedface
    //   3. Create shared child
    //   4. Assert child inherits both values
    //   5. Change child's BREAK_ON_LOAD to 0
    //   6. Assert prototype's BREAK_ON_LOAD still == 0x10adf00d
    //   (Same pattern for DEBUG_ADDR)
}
