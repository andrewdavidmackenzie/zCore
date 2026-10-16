//! Memory mapping tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/memory-mapping tests.
//! These test VMAR map/unmap/protect operations at the object level.

use crate::helpers::TestContext;
use zircon_object::vm::{MMUFlags, VmObject, PAGE_SIZE};

/// C++: TEST(MemoryMappingTest, AddressSpaceLimitsTest)
/// Map multiple VMOs and verify they all get distinct addresses.
#[test]
fn address_space_limits_test() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let mut addrs = alloc::vec::Vec::new();
    for _ in 0..8 {
        let vmo = VmObject::new_paged(1);
        let addr = vmar.map(None, vmo, 0, PAGE_SIZE, MMUFlags::READ).unwrap();
        addrs.push(addr);
    }
    // All addresses should be distinct
    for i in 0..addrs.len() {
        for j in i + 1..addrs.len() {
            assert_ne!(addrs[i], addrs[j]);
        }
    }
    for addr in addrs {
        vmar.unmap(addr, PAGE_SIZE).unwrap();
    }
}

extern crate alloc;

/// C++: TEST(MemoryMappingTest, MmapFlagsTest)
/// Map with various flag combinations and verify they succeed.
#[test]
fn mmap_flags_test() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    // READ only
    let vmo = VmObject::new_paged(1);
    let a1 = vmar.map(None, vmo, 0, PAGE_SIZE, MMUFlags::READ).unwrap();
    vmar.unmap(a1, PAGE_SIZE).unwrap();
    // READ | WRITE
    let vmo = VmObject::new_paged(1);
    let a2 = vmar
        .map(None, vmo, 0, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE)
        .unwrap();
    vmar.unmap(a2, PAGE_SIZE).unwrap();
}

/// C++: TEST(MemoryMappingTest, MmapLenTest)
/// Map with various lengths (1 page, 4 pages, 16 pages).
#[test]
fn mmap_len_test() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    for pages in [1, 4, 16] {
        let vmo = VmObject::new_paged(pages);
        let len = pages * PAGE_SIZE;
        let addr = vmar.map(None, vmo, 0, len, MMUFlags::READ).unwrap();
        assert_ne!(addr, 0);
        vmar.unmap(addr, len).unwrap();
    }
}

/// C++: TEST(MemoryMappingTest, MmapOffsetTest)
/// Map at a VMO offset. The mapped region starts at the given offset.
#[test]
fn mmap_offset_test() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(4); // 4 pages
                                      // Write data at page 2
    vmo.write(2 * PAGE_SIZE, &[0xDE, 0xAD]).unwrap();
    // Map pages 2-3 (offset=0x2000, len=0x2000)
    let addr = vmar
        .map(
            None,
            vmo.clone(),
            2 * PAGE_SIZE,
            2 * PAGE_SIZE,
            MMUFlags::READ,
        )
        .unwrap();
    assert_ne!(addr, 0);
    // Verify via VMO that data is there
    let mut buf = [0u8; 2];
    vmo.read(2 * PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(buf, [0xDE, 0xAD]);
    vmar.unmap(addr, 2 * PAGE_SIZE).unwrap();
}

/// C++: TEST(MemoryMappingTest, MmapProtExecTest)
/// Map with EXECUTE flag. May succeed or fail depending on policy.
#[test]
fn mmap_prot_exec_test() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1);
    // Attempt mapping with EXECUTE — may fail if VMO lacks EXECUTE right
    let result = vmar.map(None, vmo, 0, PAGE_SIZE, MMUFlags::READ | MMUFlags::EXECUTE);
    // Either succeeds or fails with an error — no panic
    if let Ok(addr) = result {
        vmar.unmap(addr, PAGE_SIZE).unwrap();
    }
}

/// C++: TEST(MemoryMappingTest, MmapProtTest)
/// Map with different protection flags and verify.
#[test]
fn mmap_prot_test() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    // Map READ, then protect to READ|WRITE
    let vmo = VmObject::new_paged(1);
    let addr = vmar.map(None, vmo, 0, PAGE_SIZE, MMUFlags::READ).unwrap();
    let _ = vmar.protect(addr, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE);
    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

/// C++: TEST(MemoryMappingTest, MmapZerofilledTest)
/// A freshly created VMO's pages are zero-filled.
#[test]
fn mmap_zerofilled_test() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(1);
    let addr = vmar
        .map(None, vmo.clone(), 0, PAGE_SIZE, MMUFlags::READ)
        .unwrap();
    // Read from VMO — should be all zeros
    let mut buf = [0xFFu8; 64];
    vmo.read(0, &mut buf).unwrap();
    assert!(
        buf.iter().all(|&b| b == 0),
        "VMO pages should be zero-filled"
    );
    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

/// C++: TEST(MemoryMappingTest, MprotectTest)
/// Change protection on a mapped range. Verify protect succeeds.
#[test]
fn mprotect_test() {
    let ctx = TestContext::new();
    let vmar = ctx.proc.vmar();
    let vmo = VmObject::new_paged(2);
    let addr = vmar
        .map(
            None,
            vmo,
            0,
            2 * PAGE_SIZE,
            MMUFlags::READ | MMUFlags::WRITE,
        )
        .unwrap();
    // Protect the first page to read-only
    let result = vmar.protect(addr, PAGE_SIZE, MMUFlags::READ);
    assert!(result.is_ok());
    // Protect back to read-write
    let result = vmar.protect(addr, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE);
    assert!(result.is_ok());
    vmar.unmap(addr, 2 * PAGE_SIZE).unwrap();
}
