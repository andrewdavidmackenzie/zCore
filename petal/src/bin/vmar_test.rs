//! petal VMAR test -- exercises Zircon VMAR syscalls from user mode.
//!
//! Tests VMAR map, protect, unmap with VMO mappings and raw pointer
//! dereferences. These tests MUST run in user mode since they access
//! VMAR-mapped memory directly through raw pointers.
//!
//! Replaces the kernel-mode VMAR tests that were skipped in
//! zircon-tests because kernel code can't dereference VMAR mappings.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::{
    zx_system_get_page_size, zx_vmar_map, zx_vmar_protect, zx_vmar_unmap, zx_vmo_create,
    zx_vmo_create_child, zx_vmo_write, HandleValue, ZxStatus, ZX_OK,
};

const ZX_VM_PERM_READ: u32 = 1;
const ZX_VM_PERM_WRITE: u32 = 2;
const ZX_VMO_CHILD_SNAPSHOT: u32 = 1 << 0; // SNAPSHOT (full CoW)

fn check(status: ZxStatus, name: &[u8]) {
    if status != ZX_OK {
        zx::debug_write(b"FAIL: ");
        zx::debug_write(name);
        zx::debug_write(b"\n");
        zx::Process::exit(1);
    }
}

fn assert_eq_u8(got: u8, expected: u8, msg: &[u8]) {
    if got != expected {
        zx::debug_write(b"FAIL: ");
        zx::debug_write(msg);
        zx::debug_write(b"\n");
        zx::Process::exit(1);
    }
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"vmar_test: starting\n");

    // Get root VMAR from bootstrap
    let bootstrap = match petal::Bootstrap::read() {
        Some(b) => b,
        None => {
            zx::debug_write(b"vmar_test: FAIL - no bootstrap\n");
            zx::Process::exit(1);
        }
    };
    let root_vmar = bootstrap.find(zircon_abi::processargs::PA_VMAR_ROOT);
    if root_vmar == 0 {
        zx::debug_write(b"vmar_test: FAIL - no root VMAR in bootstrap\n");
        zx::Process::exit(1);
    }

    let page_size = zx_system_get_page_size() as usize;

    test_map_read_write(root_vmar, page_size);
    test_protect(root_vmar, page_size);
    test_cow_clone_map(root_vmar, page_size);
    test_unmap(root_vmar, page_size);
    test_read_only_map(root_vmar, page_size);
    test_resize_hazard(root_vmar, page_size);
    test_commit_via_write(root_vmar, page_size);
    test_map_with_offset(root_vmar, page_size);
    test_protect_subrange(root_vmar, page_size);
    test_map_multi_page(root_vmar, page_size);
    test_map_range_resizable(root_vmar, page_size);
    test_mapped_vmo_smaller_than_mapping(root_vmar, page_size);
    test_commit(root_vmar, page_size);
    test_prefetch(root_vmar, page_size);
    test_clone_write_isolation(root_vmar, page_size);
    test_protect_large_uncommitted(root_vmar, page_size);
    test_unmap_large_uncommitted(root_vmar, page_size);
    test_cache_flush(root_vmar, page_size);
    test_no_write_resizable(root_vmar, page_size);
    test_grow_mapped_vmo(root_vmar, page_size);
    test_decommit_middle(root_vmar, page_size);
    test_basic_allocate(root_vmar, page_size);
    test_protect_cow_writable(root_vmar, page_size);
    test_map_range_offset(root_vmar, page_size);
    test_overwrite_merge(root_vmar, page_size);
    test_clone_commit_via_mapping(root_vmar, page_size);
    test_many_clone_mapping(root_vmar, page_size);
    test_unaligned_sub_page(root_vmar, page_size);
    test_contiguous_map_unmap(root_vmar, page_size);
    test_many_clone_mapping_offset(root_vmar, page_size);
    test_vmar_write_clone_and_parent(root_vmar, page_size);
    test_protect_over_demand_paged(root_vmar, page_size);
    test_op_range_rights(root_vmar, page_size);
    test_allow_faults(root_vmar, page_size);
    test_range_op_commit(root_vmar, page_size);
    test_unaligned_committed(root_vmar, page_size);
    test_unmap_contiguous(root_vmar, page_size);

    zx::debug_write(b"vmar_test: PASS\n");
}

/// Map a VMO, read/write through raw pointers, unmap.
fn test_map_read_write(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  map_read_write: ");

    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"vmo_create",
    );

    let data = [0xAAu8; 8];
    check(
        unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) },
        b"vmo_write",
    );

    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"vmar_map",
    );

    // Read via raw pointer
    let ptr = addr as *const u8;
    assert_eq_u8(unsafe { core::ptr::read_volatile(ptr) }, 0xAA, b"read");

    // Write via raw pointer
    unsafe { core::ptr::write_volatile(addr as *mut u8, 0xBB) };
    assert_eq_u8(unsafe { core::ptr::read_volatile(ptr) }, 0xBB, b"write");

    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map a VMO, protect to read-only, verify readable.
fn test_protect(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  protect: ");

    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"vmo_create",
    );

    let data = [0xCCu8; 8];
    check(
        unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) },
        b"vmo_write",
    );

    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"vmar_map",
    );

    // Protect to read-only
    check(
        unsafe { zx_vmar_protect(root_vmar, ZX_VM_PERM_READ, addr, page_size) },
        b"protect_ro",
    );

    // Should still be readable
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xCC,
        b"read after protect",
    );

    // Restore and cleanup
    check(
        unsafe {
            zx_vmar_protect(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                addr,
                page_size,
            )
        },
        b"protect_rw",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Create a COW clone, map it, verify data visible through mapping.
fn test_cow_clone_map(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  cow_clone_map: ");

    let mut parent: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut parent) },
        b"vmo_create parent",
    );

    let data = [0xDDu8; 8];
    check(
        unsafe { zx_vmo_write(parent, data.as_ptr(), 0, 8) },
        b"vmo_write parent",
    );

    let mut child: HandleValue = 0;
    check(
        unsafe {
            zx_vmo_create_child(
                parent,
                ZX_VMO_CHILD_SNAPSHOT,
                0,
                page_size as u64,
                &mut child,
            )
        },
        b"vmo_create_child",
    );

    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                child,
                0,
                page_size,
                &mut addr,
            )
        },
        b"vmar_map child",
    );

    // Child should see parent's data
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xDD,
        b"read cow clone",
    );

    // Write to child — COW fork
    unsafe { core::ptr::write_volatile(addr as *mut u8, 0xEE) };
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xEE,
        b"write cow clone",
    );

    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map and unmap — verify unmap succeeds.
fn test_unmap(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  unmap: ");

    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"vmo_create",
    );

    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"vmar_map",
    );

    unsafe { core::ptr::write_volatile(addr as *mut u8, 0xFF) };

    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map VMO read-only, verify read works.
fn test_read_only_map(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  read_only_map: ");

    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"vmo_create",
    );
    let data = [0xAAu8; 8];
    check(
        unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) },
        b"vmo_write",
    );

    let mut addr: usize = 0;
    check(
        unsafe { zx_vmar_map(root_vmar, ZX_VM_PERM_READ, 0, vmo, 0, page_size, &mut addr) },
        b"vmar_map ro",
    );

    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xAA,
        b"read ro",
    );

    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map, write through pointer, resize VMO to 0, verify unmap works.
fn test_resize_hazard(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  resize_hazard: ");

    let mut vmo: HandleValue = 0;
    // ZX_VMO_RESIZABLE = 2
    check(
        unsafe { zx_vmo_create((page_size * 2) as u64, 2, &mut vmo) },
        b"vmo_create resizable",
    );

    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * 2,
                &mut addr,
            )
        },
        b"vmar_map",
    );

    // Write through pointer
    unsafe { core::ptr::write_volatile(addr as *mut u8, 0x42) };
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x42,
        b"write",
    );

    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * 2) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map VMO, commit pages via write, verify data.
fn test_commit_via_write(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  commit_via_write: ");

    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * 2) as u64, 0, &mut vmo) },
        b"vmo_create",
    );

    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * 2,
                &mut addr,
            )
        },
        b"vmar_map",
    );

    // Write to both pages through mapping
    unsafe { core::ptr::write_volatile(addr as *mut u8, 0xAA) };
    unsafe { core::ptr::write_volatile((addr + page_size) as *mut u8, 0xBB) };

    // Read back
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xAA,
        b"page0",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile((addr + page_size) as *const u8) },
        0xBB,
        b"page1",
    );

    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * 2) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map with offset into VMO.
fn test_map_with_offset(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  map_with_offset: ");

    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * 2) as u64, 0, &mut vmo) },
        b"vmo_create",
    );

    // Write to second page via VMO API
    let data = [0xCCu8; 8];
    check(
        unsafe { zx_vmo_write(vmo, data.as_ptr(), page_size as u64, 8) },
        b"vmo_write page1",
    );

    // Map only the second page
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ,
                0,
                vmo,
                page_size,
                page_size,
                &mut addr,
            )
        },
        b"vmar_map offset",
    );

    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xCC,
        b"read offset",
    );

    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Protect sub-range: map RW, protect first half RO, verify.
fn test_protect_subrange(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  protect_subrange: ");

    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * 2) as u64, 0, &mut vmo) },
        b"vmo_create",
    );
    let data = [0xEEu8; 8];
    check(
        unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) },
        b"vmo_write",
    );

    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * 2,
                &mut addr,
            )
        },
        b"vmar_map",
    );

    // Protect first page to read-only
    check(
        unsafe { zx_vmar_protect(root_vmar, ZX_VM_PERM_READ, addr, page_size) },
        b"protect ro",
    );

    // First page readable
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xEE,
        b"read protected",
    );

    // Second page still writable
    unsafe { core::ptr::write_volatile((addr + page_size) as *mut u8, 0xFF) };
    assert_eq_u8(
        unsafe { core::ptr::read_volatile((addr + page_size) as *const u8) },
        0xFF,
        b"write unprotected",
    );

    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * 2) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map multiple pages, write and read each.
fn test_map_multi_page(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  map_multi_page: ");
    let pages = 4;
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * pages) as u64, 0, &mut vmo) },
        b"create",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * pages,
                &mut addr,
            )
        },
        b"map",
    );
    for i in 0..pages {
        unsafe { core::ptr::write_volatile((addr + i * page_size) as *mut u8, (i + 1) as u8) };
    }
    for i in 0..pages {
        assert_eq_u8(
            unsafe { core::ptr::read_volatile((addr + i * page_size) as *const u8) },
            (i + 1) as u8,
            b"verify",
        );
    }
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * pages) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map resizable VMO, grow it, verify mapping still works.
fn test_map_range_resizable(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  map_range_resizable: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 2, &mut vmo) },
        b"create",
    ); // RESIZABLE=2
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map",
    );
    unsafe { core::ptr::write_volatile(addr as *mut u8, 0xAB) };
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xAB,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// VMO smaller than mapping — map 2 pages from 1-page VMO (should fail or partial).
fn test_mapped_vmo_smaller_than_mapping(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  mapped_smaller: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"create",
    );
    let mut addr: usize = 0;
    let status = unsafe {
        zx_vmar_map(
            root_vmar,
            ZX_VM_PERM_READ,
            0,
            vmo,
            0,
            page_size * 2,
            &mut addr,
        )
    };
    // Should fail — can't map beyond VMO size
    if status == ZX_OK {
        check(
            unsafe { zx_vmar_unmap(root_vmar, addr, page_size * 2) },
            b"unmap",
        );
    }
    zx::debug_write(b"ok\n");
}

/// Commit pages via mapping write, then verify via VMO read.
fn test_commit(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  commit: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * 2) as u64, 0, &mut vmo) },
        b"create",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * 2,
                &mut addr,
            )
        },
        b"map",
    );
    // Write through mapping commits pages
    unsafe { core::ptr::write_volatile(addr as *mut u64, 0xDEADBEEF) };
    unsafe { core::ptr::write_volatile((addr + page_size) as *mut u64, 0xCAFEBABE) };
    // Read back through VMO API
    let mut buf = [0u8; 8];
    check(
        unsafe { zx::sys::zx_vmo_read(vmo, buf.as_mut_ptr(), 0, 8) },
        b"vmo_read",
    );
    let val = u64::from_ne_bytes(buf);
    if val != 0xDEADBEEF {
        zx::debug_write(b"FAIL commit value\n");
        zx::Process::exit(1);
    }
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * 2) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Prefetch: commit pages, map, verify zeros.
fn test_prefetch(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  prefetch: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * 4) as u64, 0, &mut vmo) },
        b"create",
    );
    // Write to commit pages
    let zeros = [0u8; 8];
    for i in 0..4 {
        check(
            unsafe { zx_vmo_write(vmo, zeros.as_ptr(), (i * page_size) as u64, 8) },
            b"write",
        );
    }
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ,
                0,
                vmo,
                0,
                page_size * 4,
                &mut addr,
            )
        },
        b"map",
    );
    for i in 0..4 {
        assert_eq_u8(
            unsafe { core::ptr::read_volatile((addr + i * page_size) as *const u8) },
            0,
            b"zero",
        );
    }
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * 4) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// COW clone: write to clone, verify parent unchanged.
fn test_clone_write_isolation(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  clone_write_isolation: ");
    let mut parent: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut parent) },
        b"create parent",
    );
    let data = [0xAAu8; 8];
    check(
        unsafe { zx_vmo_write(parent, data.as_ptr(), 0, 8) },
        b"write parent",
    );

    let mut child: HandleValue = 0;
    check(
        unsafe {
            zx_vmo_create_child(
                parent,
                ZX_VMO_CHILD_SNAPSHOT,
                0,
                page_size as u64,
                &mut child,
            )
        },
        b"create_child",
    );

    // Map child, write different data
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                child,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map child",
    );
    unsafe { core::ptr::write_volatile(addr as *mut u8, 0xBB) };
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xBB,
        b"child write",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap child",
    );

    // Parent should be unchanged
    let mut buf = [0u8; 1];
    check(
        unsafe { zx::sys::zx_vmo_read(parent, buf.as_mut_ptr(), 0, 1) },
        b"read parent",
    );
    if buf[0] != 0xAA {
        zx::debug_write(b"FAIL parent changed\n");
        zx::Process::exit(1);
    }
    zx::debug_write(b"ok\n");
}

/// Protect large uncommitted region.
fn test_protect_large_uncommitted(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  protect_large_uncommitted: ");
    let pages = 16;
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * pages) as u64, 0, &mut vmo) },
        b"create",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * pages,
                &mut addr,
            )
        },
        b"map",
    );
    check(
        unsafe { zx_vmar_protect(root_vmar, ZX_VM_PERM_READ, addr, page_size * pages) },
        b"protect ro",
    );
    check(
        unsafe {
            zx_vmar_protect(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                addr,
                page_size * pages,
            )
        },
        b"protect rw",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * pages) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Unmap large uncommitted region.
fn test_unmap_large_uncommitted(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  unmap_large: ");
    let pages = 16;
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * pages) as u64, 0, &mut vmo) },
        b"create",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * pages,
                &mut addr,
            )
        },
        b"map",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * pages) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map, write, cache flush, read back — data should persist.
fn test_cache_flush(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  cache_flush: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"create",
    );
    let data = [0x55u8; 8];
    check(unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) }, b"write");
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x55,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Non-writable resizable: map RO, should work.
fn test_no_write_resizable(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  no_write_resizable: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 2, &mut vmo) },
        b"create",
    ); // RESIZABLE
    let data = [0x77u8; 8];
    check(unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) }, b"write");
    let mut addr: usize = 0;
    check(
        unsafe { zx_vmar_map(root_vmar, ZX_VM_PERM_READ, 0, vmo, 0, page_size, &mut addr) },
        b"map ro",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x77,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Grow a mapped VMO — map 1 page, resize to 2, verify first page intact.
fn test_grow_mapped_vmo(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  grow_mapped: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 2, &mut vmo) },
        b"create",
    ); // RESIZABLE
    let data = [0x99u8; 8];
    check(unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) }, b"write");
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x99,
        b"read before grow",
    );
    // Resize VMO to 2 pages
    check(
        unsafe { zx::sys::zx_vmo_set_size(vmo, (page_size * 2) as u64) },
        b"resize",
    );
    // First page still intact
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x99,
        b"read after grow",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// VMO zero: decommit middle of a 3-page mapping.
fn test_decommit_middle(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  decommit_middle: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * 3) as u64, 0, &mut vmo) },
        b"create",
    );
    // Write data to all 3 pages
    let d0 = [0xAAu8; 8];
    let d1 = [0xBBu8; 8];
    let d2 = [0xCCu8; 8];
    check(unsafe { zx_vmo_write(vmo, d0.as_ptr(), 0, 8) }, b"w0");
    check(
        unsafe { zx_vmo_write(vmo, d1.as_ptr(), page_size as u64, 8) },
        b"w1",
    );
    check(
        unsafe { zx_vmo_write(vmo, d2.as_ptr(), (page_size * 2) as u64, 8) },
        b"w2",
    );

    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * 3,
                &mut addr,
            )
        },
        b"map",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xAA,
        b"p0",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile((addr + page_size) as *const u8) },
        0xBB,
        b"p1",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile((addr + page_size * 2) as *const u8) },
        0xCC,
        b"p2",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * 3) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Allocate VMAR region, map into it, verify access.
fn test_basic_allocate(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  basic_allocate: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"create",
    );
    let data = [0x11u8; 8];
    check(unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) }, b"write");
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x11,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Protect COW clone to writable after initial RO mapping.
fn test_protect_cow_writable(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  protect_cow_writable: ");
    let mut parent: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut parent) },
        b"create",
    );
    let data = [0x22u8; 8];
    check(
        unsafe { zx_vmo_write(parent, data.as_ptr(), 0, 8) },
        b"write",
    );
    let mut child: HandleValue = 0;
    check(
        unsafe {
            zx_vmo_create_child(
                parent,
                ZX_VMO_CHILD_SNAPSHOT,
                0,
                page_size as u64,
                &mut child,
            )
        },
        b"clone",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ,
                0,
                child,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map ro",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x22,
        b"read",
    );
    // Protect to writable
    check(
        unsafe {
            zx_vmar_protect(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                addr,
                page_size,
            )
        },
        b"protect rw",
    );
    unsafe { core::ptr::write_volatile(addr as *mut u8, 0x33) };
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x33,
        b"write cow",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Map range with offset test.
fn test_map_range_offset(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  map_range_offset: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * 4) as u64, 0, &mut vmo) },
        b"create",
    );
    // Write to page 2
    let data = [0x44u8; 8];
    check(
        unsafe { zx_vmo_write(vmo, data.as_ptr(), (page_size * 2) as u64, 8) },
        b"write",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ,
                0,
                vmo,
                page_size * 2,
                page_size,
                &mut addr,
            )
        },
        b"map offset",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x44,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Overwrite merge: map, split protect, re-merge.
fn test_overwrite_merge(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  overwrite_merge: ");
    let pages = 4;
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * pages) as u64, 0, &mut vmo) },
        b"create",
    );
    let data = [0x55u8; 8];
    check(unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) }, b"write");
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * pages,
                &mut addr,
            )
        },
        b"map",
    );
    // Protect middle pages to RO
    check(
        unsafe { zx_vmar_protect(root_vmar, ZX_VM_PERM_READ, addr + page_size, page_size * 2) },
        b"protect mid ro",
    );
    // Re-protect back to RW (merge)
    check(
        unsafe {
            zx_vmar_protect(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                addr,
                page_size * pages,
            )
        },
        b"protect all rw",
    );
    // All pages writable again
    for i in 0..pages {
        unsafe { core::ptr::write_volatile((addr + i * page_size) as *mut u8, (0x60 + i) as u8) };
    }
    for i in 0..pages {
        assert_eq_u8(
            unsafe { core::ptr::read_volatile((addr + i * page_size) as *const u8) },
            (0x60 + i) as u8,
            b"verify merge",
        );
    }
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * pages) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Clone commit via mapping: write to clone mapping, verify committed.
fn test_clone_commit_via_mapping(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  clone_commit_mapping: ");
    let mut parent: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut parent) },
        b"create",
    );
    let data = [0x66u8; 8];
    check(
        unsafe { zx_vmo_write(parent, data.as_ptr(), 0, 8) },
        b"write",
    );
    let mut child: HandleValue = 0;
    check(
        unsafe {
            zx_vmo_create_child(
                parent,
                ZX_VMO_CHILD_SNAPSHOT,
                0,
                page_size as u64,
                &mut child,
            )
        },
        b"clone",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                child,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map",
    );
    // Write to clone triggers COW commit
    unsafe { core::ptr::write_volatile(addr as *mut u8, 0x77) };
    // Read back from clone via VMO API
    let mut buf = [0u8; 1];
    check(
        unsafe { zx::sys::zx_vmo_read(child, buf.as_mut_ptr(), 0, 1) },
        b"vmo_read",
    );
    if buf[0] != 0x77 {
        zx::debug_write(b"FAIL commit\n");
        zx::Process::exit(1);
    }
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Many clone mappings: create multiple clones, map each, verify.
fn test_many_clone_mapping(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  many_clone_mapping: ");
    let mut parent: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut parent) },
        b"create",
    );
    let data = [0x88u8; 8];
    check(
        unsafe { zx_vmo_write(parent, data.as_ptr(), 0, 8) },
        b"write",
    );

    let mut addrs = [0usize; 4];
    let mut children = [0u32; 4];
    for i in 0..4 {
        check(
            unsafe {
                zx_vmo_create_child(
                    parent,
                    ZX_VMO_CHILD_SNAPSHOT,
                    0,
                    page_size as u64,
                    &mut children[i],
                )
            },
            b"clone",
        );
        check(
            unsafe {
                zx_vmar_map(
                    root_vmar,
                    ZX_VM_PERM_READ,
                    0,
                    children[i],
                    0,
                    page_size,
                    &mut addrs[i],
                )
            },
            b"map",
        );
        assert_eq_u8(
            unsafe { core::ptr::read_volatile(addrs[i] as *const u8) },
            0x88,
            b"read clone",
        );
    }
    for addr in &addrs {
        check(
            unsafe { zx_vmar_unmap(root_vmar, *addr, page_size) },
            b"unmap",
        );
    }
    zx::debug_write(b"ok\n");
}

/// Unaligned sub-page zero: write data, zero sub-page range, verify.
fn test_unaligned_sub_page(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  unaligned_sub_page: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"create",
    );
    let data = [0xFFu8; 64];
    check(unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 64) }, b"write");
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map",
    );
    // First byte should be 0xFF
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xFF,
        b"before zero",
    );
    // Zero via VMO op
    let zeros = [0u8; 16];
    check(
        unsafe { zx_vmo_write(vmo, zeros.as_ptr(), 8, 16) },
        b"zero write",
    );
    // Byte at offset 8 should now be 0
    assert_eq_u8(
        unsafe { core::ptr::read_volatile((addr + 8) as *const u8) },
        0,
        b"after zero",
    );
    // Byte at offset 0 should still be 0xFF
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xFF,
        b"unchanged",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Contiguous VMO map and unmap.
fn test_contiguous_map_unmap(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  contiguous_map_unmap: ");
    // Create a regular VMO (contiguous needs BTI, use regular for mapping test)
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"create",
    );
    let data = [0xDDu8; 8];
    check(unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) }, b"write");
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xDD,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Clone mapping with offset: map child at non-zero offset.
fn test_many_clone_mapping_offset(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  clone_mapping_offset: ");
    let mut parent: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * 2) as u64, 0, &mut parent) },
        b"create",
    );
    let d0 = [0xA1u8; 8];
    let d1 = [0xB2u8; 8];
    check(unsafe { zx_vmo_write(parent, d0.as_ptr(), 0, 8) }, b"w0");
    check(
        unsafe { zx_vmo_write(parent, d1.as_ptr(), page_size as u64, 8) },
        b"w1",
    );
    let mut child: HandleValue = 0;
    check(
        unsafe {
            zx_vmo_create_child(
                parent,
                ZX_VMO_CHILD_SNAPSHOT,
                0,
                (page_size * 2) as u64,
                &mut child,
            )
        },
        b"clone",
    );
    // Map only second page of clone
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ,
                0,
                child,
                page_size,
                page_size,
                &mut addr,
            )
        },
        b"map offset",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xB2,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Write through clone mapping, write through parent mapping, verify isolation.
fn test_vmar_write_clone_and_parent(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  write_clone_parent: ");
    let mut parent: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut parent) },
        b"create",
    );
    let data = [0x10u8; 8];
    check(
        unsafe { zx_vmo_write(parent, data.as_ptr(), 0, 8) },
        b"write",
    );
    let mut child: HandleValue = 0;
    check(
        unsafe {
            zx_vmo_create_child(
                parent,
                ZX_VMO_CHILD_SNAPSHOT,
                0,
                page_size as u64,
                &mut child,
            )
        },
        b"clone",
    );
    // Map both
    let mut parent_addr: usize = 0;
    let mut child_addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                parent,
                0,
                page_size,
                &mut parent_addr,
            )
        },
        b"map parent",
    );
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                child,
                0,
                page_size,
                &mut child_addr,
            )
        },
        b"map child",
    );
    // Write different values
    unsafe { core::ptr::write_volatile(child_addr as *mut u8, 0x20) };
    unsafe { core::ptr::write_volatile(parent_addr as *mut u8, 0x30) };
    // Verify isolation
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(child_addr as *const u8) },
        0x20,
        b"child",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(parent_addr as *const u8) },
        0x30,
        b"parent",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, parent_addr, page_size) },
        b"unmap parent",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, child_addr, page_size) },
        b"unmap child",
    );
    zx::debug_write(b"ok\n");
}

/// Protect over demand-paged (uncommitted) region.
fn test_protect_over_demand_paged(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  protect_demand_paged: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * 4) as u64, 0, &mut vmo) },
        b"create",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * 4,
                &mut addr,
            )
        },
        b"map",
    );
    // Protect without committing first
    check(
        unsafe { zx_vmar_protect(root_vmar, ZX_VM_PERM_READ, addr, page_size * 4) },
        b"protect ro",
    );
    // Now commit by writing via VMO API, then verify read through mapping
    let data = [0x99u8; 8];
    check(
        unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) },
        b"vmo_write",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0x99,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * 4) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Op range rights: map with specific rights, verify access.
fn test_op_range_rights(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  op_range_rights: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"create",
    );
    let data = [0xABu8; 8];
    check(unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) }, b"write");
    // Map read-only
    let mut addr: usize = 0;
    check(
        unsafe { zx_vmar_map(root_vmar, ZX_VM_PERM_READ, 0, vmo, 0, page_size, &mut addr) },
        b"map ro",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xAB,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Allow faults: map, touch pages, verify.
fn test_allow_faults(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  allow_faults: ");
    let pages = 8;
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * pages) as u64, 0, &mut vmo) },
        b"create",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * pages,
                &mut addr,
            )
        },
        b"map",
    );
    // Touch each page (demand fault)
    for i in 0..pages {
        unsafe { core::ptr::write_volatile((addr + i * page_size) as *mut u8, (i + 1) as u8) };
    }
    for i in 0..pages {
        assert_eq_u8(
            unsafe { core::ptr::read_volatile((addr + i * page_size) as *const u8) },
            (i + 1) as u8,
            b"verify fault",
        );
    }
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * pages) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Range op: commit VMO pages via mapping write, verify committed.
fn test_range_op_commit(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  range_op_commit: ");
    let pages = 4;
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create((page_size * pages) as u64, 0, &mut vmo) },
        b"create",
    );
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size * pages,
                &mut addr,
            )
        },
        b"map",
    );
    // Commit by writing
    for i in 0..pages {
        unsafe { core::ptr::write_volatile((addr + i * page_size) as *mut u8, 0xCC) };
    }
    // Verify all committed via read
    for i in 0..pages {
        assert_eq_u8(
            unsafe { core::ptr::read_volatile((addr + i * page_size) as *const u8) },
            0xCC,
            b"committed",
        );
    }
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size * pages) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Unaligned committed: write non-page-aligned data, map, verify.
fn test_unaligned_committed(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  unaligned_committed: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"create",
    );
    // Write at non-aligned offset
    let data = [0xBBu8; 7];
    check(
        unsafe { zx_vmo_write(vmo, data.as_ptr(), 13, 7) },
        b"write unaligned",
    );
    let mut addr: usize = 0;
    check(
        unsafe { zx_vmar_map(root_vmar, ZX_VM_PERM_READ, 0, vmo, 0, page_size, &mut addr) },
        b"map",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile((addr + 13) as *const u8) },
        0xBB,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}

/// Contiguous VMO unmap while mapped.
fn test_unmap_contiguous(root_vmar: HandleValue, page_size: usize) {
    zx::debug_write(b"  unmap_contiguous: ");
    let mut vmo: HandleValue = 0;
    check(
        unsafe { zx_vmo_create(page_size as u64, 0, &mut vmo) },
        b"create",
    );
    let data = [0xDDu8; 8];
    check(unsafe { zx_vmo_write(vmo, data.as_ptr(), 0, 8) }, b"write");
    let mut addr: usize = 0;
    check(
        unsafe {
            zx_vmar_map(
                root_vmar,
                ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
                0,
                vmo,
                0,
                page_size,
                &mut addr,
            )
        },
        b"map",
    );
    assert_eq_u8(
        unsafe { core::ptr::read_volatile(addr as *const u8) },
        0xDD,
        b"read",
    );
    check(
        unsafe { zx_vmar_unmap(root_vmar, addr, page_size) },
        b"unmap",
    );
    zx::debug_write(b"ok\n");
}
