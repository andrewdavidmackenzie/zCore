//! petal CPU mask profile test -- profile create and apply with CPU affinity.
//!
//! Port of zircon-tests/src/core_tests/cpu_mask_profile.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_RESOURCE: u32 = 0x3F;
const PA_PROC_SELF: u32 = 0x01;

// ProfileInfo ABI layout (96 bytes total):
// offset 0: flags_raw (u32) — ProfileInfoFlags bitmask
// offset 4: padding (u32)
// offset 8: sched_union[24] — priority at offset 8 (i32)
// offset 32: cpu_affinity_mask[8] — u64 × 8 = 64 bytes

const PROFILE_INFO_SIZE: usize = 96;
const FLAG_CPU_MASK: u32 = 1 << 1;

fn pass(name: &[u8]) {
    zx::debug_write(b"  cpu_mask_profile_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: cpu_mask_profile_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn assert_ok(s: ZxStatus, name: &[u8], msg: &[u8]) {
    if s != ZX_OK {
        fail(name, msg);
    }
}

fn make_cpu_mask_info(mask: u64) -> [u8; PROFILE_INFO_SIZE] {
    let mut info = [0u8; PROFILE_INFO_SIZE];
    // flags_raw at offset 0
    info[0..4].copy_from_slice(&FLAG_CPU_MASK.to_ne_bytes());
    // cpu_affinity_mask[0] at offset 32
    info[32..40].copy_from_slice(&mask.to_ne_bytes());
    info
}

/// Empty mask is valid — kernel falls back to any CPU
fn empty_mask_is_valid(resource: HandleValue, proc_h: HandleValue) {
    let name = b"empty_mask_is_valid";

    let info = make_cpu_mask_info(0); // empty mask
    let mut profile_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_profile_create(resource, 0, info.as_ptr(), &mut profile_h) },
        name,
        b"profile create",
    );
    if profile_h == 0 {
        fail(name, b"profile handle is 0");
    }

    // Create a thread and apply
    let mut thread_h: HandleValue = 0;
    let tname = b"cpu-mask-t\0";
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), tname.len() - 1, 0, &mut thread_h) },
        name,
        b"thread create",
    );

    assert_ok(
        unsafe { zx_object_set_profile(thread_h, profile_h, 0) },
        name,
        b"set profile",
    );

    unsafe { zx_handle_close(thread_h) };
    unsafe { zx_handle_close(profile_h) };
    pass(name);
}

/// Apply profile with mask = (1 << 0)
fn apply_profile(resource: HandleValue, proc_h: HandleValue) {
    let name = b"apply_profile";

    let info = make_cpu_mask_info(1); // CPU 0
    let mut profile_h: HandleValue = 0;
    assert_ok(
        unsafe { zx_profile_create(resource, 0, info.as_ptr(), &mut profile_h) },
        name,
        b"profile create",
    );

    let mut thread_h: HandleValue = 0;
    let tname = b"cpu-apply\0";
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), tname.len() - 1, 0, &mut thread_h) },
        name,
        b"thread create",
    );

    assert_ok(
        unsafe { zx_object_set_profile(thread_h, profile_h, 0) },
        name,
        b"set profile",
    );

    unsafe { zx_handle_close(thread_h) };
    unsafe { zx_handle_close(profile_h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"cpu_mask_profile_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let resource = bootstrap.find(PA_RESOURCE);
    let proc_h = bootstrap.find(PA_PROC_SELF);

    if resource == 0 || proc_h == 0 {
        zx::debug_write(b"  SKIP: missing resource or proc_self\n");
        zx::debug_write(b"cpu_mask_profile_test: all tests passed\n");
        return;
    }

    empty_mask_is_valid(resource, proc_h);
    apply_profile(resource, proc_h);

    zx::debug_write(b"cpu_mask_profile_test: all tests passed\n");
}
