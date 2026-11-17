//! petal profile test -- profile create/apply syscalls.
//!
//! Port of UserPtr-using tests from zircon-tests/src/core_tests/profile.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_RESOURCE: u32 = 0x3F;
const PA_PROC_SELF: u32 = 0x01;

const FLAG_PRIORITY: u32 = 1 << 0;
const FLAG_CPU_MASK: u32 = 1 << 1;
const PROFILE_INFO_SIZE: usize = 96;

fn pass(name: &[u8]) {
    zx::debug_write(b"  profile_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: profile_test: ");
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

fn make_priority_info(priority: i32) -> [u8; PROFILE_INFO_SIZE] {
    let mut info = [0u8; PROFILE_INFO_SIZE];
    info[0..4].copy_from_slice(&FLAG_PRIORITY.to_ne_bytes());
    info[8..12].copy_from_slice(&priority.to_ne_bytes());
    info
}

/// Create profile with valid priority
fn create_valid_priority(resource: HandleValue) {
    let name = b"create_valid_priority";
    let info = make_priority_info(16); // DEFAULT priority
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_profile_create(resource, 0, info.as_ptr(), &mut h) },
        name,
        b"create",
    );
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Create profile with invalid priority
fn create_invalid_priority(resource: HandleValue) {
    let name = b"create_invalid_priority";
    // Priority -1 is invalid
    let info = make_priority_info(-1);
    let mut h: HandleValue = 0;
    let s = unsafe { zx_profile_create(resource, 0, info.as_ptr(), &mut h) };
    if s == ZX_OK {
        unsafe { zx_handle_close(h) };
        fail(name, b"priority -1 should fail");
    }

    // Priority 32 is out of range (0-31)
    let info = make_priority_info(32);
    let s = unsafe { zx_profile_create(resource, 0, info.as_ptr(), &mut h) };
    if s == ZX_OK {
        unsafe { zx_handle_close(h) };
        fail(name, b"priority 32 should fail");
    }

    pass(name);
}

/// Apply priority profile to a thread
fn apply_priority_profile(resource: HandleValue, proc_h: HandleValue) {
    let name = b"apply_priority_profile";
    let info = make_priority_info(16);
    let mut profile: HandleValue = 0;
    assert_ok(
        unsafe { zx_profile_create(resource, 0, info.as_ptr(), &mut profile) },
        name,
        b"create profile",
    );

    let mut thread: HandleValue = 0;
    let tname = b"profile-t\0";
    assert_ok(
        unsafe { zx_thread_create(proc_h, tname.as_ptr(), tname.len() - 1, 0, &mut thread) },
        name,
        b"create thread",
    );

    assert_ok(
        unsafe { zx_object_set_profile(thread, profile, 0) },
        name,
        b"set profile",
    );

    unsafe { zx_handle_close(thread) };
    unsafe { zx_handle_close(profile) };
    pass(name);
}

/// Bad handle for profile_create
fn create_bad_handle() {
    let name = b"create_bad_handle";
    let info = make_priority_info(16);
    let mut h: HandleValue = 0;
    let s = unsafe { zx_profile_create(0xDEAD, 0, info.as_ptr(), &mut h) };
    if s != ZX_ERR_BAD_HANDLE {
        fail(name, b"expected BAD_HANDLE");
    }
    pass(name);
}

/// Wrong type handle for profile_create
fn create_wrong_type() {
    let name = b"create_wrong_type";
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );

    let info = make_priority_info(16);
    let mut h: HandleValue = 0;
    let s = unsafe { zx_profile_create(event, 0, info.as_ptr(), &mut h) };
    if s == ZX_OK {
        unsafe { zx_handle_close(h) };
        fail(name, b"event should not work as resource");
    }

    unsafe { zx_handle_close(event) };
    pass(name);
}

/// Profile create with both PRIORITY and CPU_MASK flags
fn create_priority_and_cpu_mask(resource: HandleValue) {
    let name = b"create_priority_and_cpu_mask";
    let mut info = [0u8; PROFILE_INFO_SIZE];
    info[0..4].copy_from_slice(&(FLAG_PRIORITY | FLAG_CPU_MASK).to_ne_bytes());
    info[8..12].copy_from_slice(&16i32.to_ne_bytes()); // priority
    info[32..40].copy_from_slice(&1u64.to_ne_bytes()); // CPU 0

    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_profile_create(resource, 0, info.as_ptr(), &mut h) },
        name,
        b"create",
    );
    if h == 0 {
        fail(name, b"handle is 0");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Apply profile to wrong type (not a thread)
fn apply_wrong_type(resource: HandleValue) {
    let name = b"apply_wrong_type";
    let info = make_priority_info(16);
    let mut profile: HandleValue = 0;
    assert_ok(
        unsafe { zx_profile_create(resource, 0, info.as_ptr(), &mut profile) },
        name,
        b"create profile",
    );

    // Apply to an event (not a thread) — should fail
    let mut event: HandleValue = 0;
    assert_ok(
        unsafe { zx_event_create(0, &mut event) },
        name,
        b"event create",
    );

    let s = unsafe { zx_object_set_profile(event, profile, 0) };
    if s == ZX_OK {
        fail(name, b"apply to event should fail");
    }

    unsafe { zx_handle_close(event) };
    unsafe { zx_handle_close(profile) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"profile_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let resource = bootstrap.find(PA_RESOURCE);
    let proc_h = bootstrap.find(PA_PROC_SELF);

    if resource == 0 {
        zx::debug_write(b"  SKIP: no root resource\n");
        zx::debug_write(b"profile_test: all tests passed\n");
        return;
    }

    create_valid_priority(resource);
    create_invalid_priority(resource);
    create_bad_handle();
    create_wrong_type();
    create_priority_and_cpu_mask(resource);
    apply_wrong_type(resource);
    if proc_h != 0 {
        apply_priority_profile(resource, proc_h);
    }

    zx::debug_write(b"profile_test: all tests passed\n");
}
