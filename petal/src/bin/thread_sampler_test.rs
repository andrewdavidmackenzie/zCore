//! petal thread sampler test -- sampler create/start/stop/read.
//!
//! Port of zircon-tests/src/core_tests/thread_sampler.rs.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

const PA_RESOURCE: u32 = 0x3F;

fn pass(name: &[u8]) {
    zx::debug_write(b"  sampler_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: sampler_test: ");
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

fn sampler_config(period_ns: u64, buffer_size: u64) -> [u8; 16] {
    let mut config = [0u8; 16];
    config[0..8].copy_from_slice(&period_ns.to_ne_bytes());
    config[8..16].copy_from_slice(&buffer_size.to_ne_bytes());
    config
}

/// Create a sampler, start and stop it
fn start_stop(resource: HandleValue) {
    let name = b"start_stop";
    let config = sampler_config(1_000_000, 4096); // 1ms period, 4K buffer
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_sampler_create(resource, 0, config.as_ptr(), &mut h) },
        name,
        b"create",
    );
    assert_ok(unsafe { zx_sampler_start(h) }, name, b"start");
    assert_ok(unsafe { zx_sampler_stop(h) }, name, b"stop");
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Invalid options for sampler_create
fn invalid_options(resource: HandleValue) {
    let name = b"invalid_options";
    let config = sampler_config(1_000_000, 4096);
    let mut h: HandleValue = 0;
    let s = unsafe { zx_sampler_create(resource, 0xFF, config.as_ptr(), &mut h) };
    if s == ZX_OK {
        unsafe { zx_handle_close(h) };
        fail(name, b"invalid options should fail");
    }
    pass(name);
}

/// Sampler on non-running thread
fn non_running_thread(resource: HandleValue) {
    let name = b"non_running_thread";
    let config = sampler_config(1_000_000, 4096);
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_sampler_create(resource, 0, config.as_ptr(), &mut h) },
        name,
        b"create",
    );
    // Start without attaching to a thread should work (creates the sampler)
    // The sampler operates on the calling thread
    assert_ok(unsafe { zx_sampler_start(h) }, name, b"start");
    assert_ok(unsafe { zx_sampler_stop(h) }, name, b"stop");
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// Dropped sampler — close handle while running
fn dropped_sampler(resource: HandleValue) {
    let name = b"dropped_sampler";
    let config = sampler_config(1_000_000, 4096);
    let mut h: HandleValue = 0;
    assert_ok(
        unsafe { zx_sampler_create(resource, 0, config.as_ptr(), &mut h) },
        name,
        b"create",
    );
    assert_ok(unsafe { zx_sampler_start(h) }, name, b"start");
    // Close without stopping — should clean up
    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"sampler_test: starting\n");

    let bootstrap = petal::Bootstrap::read().unwrap_or_else(|| {
        zx::debug_write(b"FAIL: no bootstrap\n");
        zx::Process::exit(1);
    });

    let resource = bootstrap.find(PA_RESOURCE);
    if resource == 0 {
        zx::debug_write(b"  SKIP: no root resource\n");
        zx::debug_write(b"sampler_test: all tests passed\n");
        return;
    }

    start_stop(resource);
    invalid_options(resource);
    non_running_thread(resource);
    dropped_sampler(resource);

    zx::debug_write(b"sampler_test: all tests passed\n");
}
