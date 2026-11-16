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

extern crate petal;

use zx::sys::{
    zx_system_get_page_size, zx_vmar_map, zx_vmar_protect, zx_vmar_unmap, zx_vmo_create,
    zx_vmo_create_child, zx_vmo_write, HandleValue, ZxStatus,
};

const ZX_OK: ZxStatus = 0;
const ZX_VM_PERM_READ: u32 = 1;
const ZX_VM_PERM_WRITE: u32 = 2;
const ZX_VMO_CHILD_SNAPSHOT: u32 = 0;

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
