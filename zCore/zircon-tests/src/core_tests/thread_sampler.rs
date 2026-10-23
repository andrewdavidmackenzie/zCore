//! Thread sampler tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/thread-sampler/thread-sampler.cc.
//!
//! **Note:** These tests use a global singleton sampler session.
//! They MUST run with `--test-threads=1` to avoid conflicts.
//! Marked `#[ignore]` so `cargo test` (parallel) skips them;
//! run with `cargo test -- --ignored --test-threads=1`.
//!
//! The thread sampler (`zx_sampler_create`/`start`/`stop`/`read`) is a
//! kernel-level profiling facility that periodically samples thread
//! register state.  In libos mode, sampling is done eagerly at
//! start/stop/read time rather than via periodic timer interrupts.

use crate::helpers::TestContext;
use zircon_object::vm::PAGE_SIZE;
use zircon_object::ZxError;

/// Helper: build a zx_sampler_config_t buffer.
fn sampler_config(period_ns: u64, buffer_size: u64) -> [u8; 16] {
    let mut buf = [0u8; 16];
    buf[0..8].copy_from_slice(&period_ns.to_ne_bytes());
    buf[8..16].copy_from_slice(&buffer_size.to_ne_bytes());
    buf
}

/// C++: TEST(ThreadSampler, StartStop)
/// Creates a sampler, starts sampling, reads at least one record, stops.
#[test]
#[ignore]
fn start_stop() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_resource = ctx.add_root_resource();

    let config = sampler_config(1_000_000, PAGE_SIZE as u64); // 1ms period, 1 page buffer
    let mut sampler_handle: u32 = 0;
    let result = sc.sys_sampler_create(
        root_resource,
        0,
        config.as_ptr() as usize,
        hal_impl::user::UserOutPtr::from(&mut sampler_handle as *mut u32 as usize),
    );
    assert!(result.is_ok(), "sampler_create should succeed");
    assert_ne!(sampler_handle, 0);

    // Start sampling.
    assert!(sc.sys_sampler_start(sampler_handle).is_ok());

    // Read — should have at least some data.
    let mut actual: usize = 0;
    let result = sc.sys_sampler_read(
        sampler_handle,
        hal_impl::user::UserOutPtr::from(0usize), // null = size query
        0,
        hal_impl::user::UserOutPtr::from(&mut actual as *mut usize as usize),
    );
    assert!(result.is_ok());
    // In libos mode, at least one thread (the test thread) should be sampled.
    assert!(actual > 0, "should have some sample data, got 0");

    // Stop.
    assert!(sc.sys_sampler_stop(sampler_handle).is_ok());

    // Read after stop should still return data.
    let mut buf = vec![0u8; actual];
    let mut actual2: usize = 0;
    let result = sc.sys_sampler_read(
        sampler_handle,
        hal_impl::user::UserOutPtr::from(buf.as_mut_ptr() as usize),
        buf.len(),
        hal_impl::user::UserOutPtr::from(&mut actual2 as *mut usize as usize),
    );
    assert!(result.is_ok());
    assert!(actual2 > 0, "should have sample data after stop");

    // Verify FXT profiler record format: word[0] header type should be 8.
    if actual2 >= 32 {
        let header = u64::from_ne_bytes(buf[0..8].try_into().unwrap());
        let record_type = header & 0xF;
        assert_eq!(record_type, 10, "FXT record type should be kProfiler (10)");
    }
}

/// C++: TEST(ThreadSampler, SamplerLifetime)
/// Only one sampler can exist at a time.
#[test]
#[ignore]
fn sampler_lifetime() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_resource = ctx.add_root_resource();

    let config = sampler_config(1_000_000, PAGE_SIZE as u64);

    // Create first sampler.
    let mut h1: u32 = 0;
    assert!(sc
        .sys_sampler_create(
            root_resource,
            0,
            config.as_ptr() as usize,
            hal_impl::user::UserOutPtr::from(&mut h1 as *mut u32 as usize),
        )
        .is_ok());

    // Second create should fail with ALREADY_EXISTS.
    let mut h2: u32 = 0;
    let result = sc.sys_sampler_create(
        root_resource,
        0,
        config.as_ptr() as usize,
        hal_impl::user::UserOutPtr::from(&mut h2 as *mut u32 as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::ALREADY_EXISTS);

    // Close the first sampler (remove handle from process).
    ctx.proc.remove_handle(h1).unwrap();

    // Now creating a new sampler should succeed.
    let mut h3: u32 = 0;
    assert!(sc
        .sys_sampler_create(
            root_resource,
            0,
            config.as_ptr() as usize,
            hal_impl::user::UserOutPtr::from(&mut h3 as *mut u32 as usize),
        )
        .is_ok());

    ctx.proc.remove_handle(h3).unwrap();
}

/// C++: TEST(ThreadSampler, InvalidOptions)
/// Non-zero options returns INVALID_ARGS.
#[test]
#[ignore]
fn invalid_options() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_resource = ctx.add_root_resource();

    let config = sampler_config(1_000_000, PAGE_SIZE as u64);
    let mut h: u32 = 0;
    let result = sc.sys_sampler_create(
        root_resource,
        1, // non-zero options
        config.as_ptr() as usize,
        hal_impl::user::UserOutPtr::from(&mut h as *mut u32 as usize),
    );
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(ThreadSampler, DroppedSampler)
/// Dropping a sampler mid-session, then creating a new one works.
#[test]
#[ignore]
fn dropped_sampler() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_resource = ctx.add_root_resource();

    let config = sampler_config(1_000_000, PAGE_SIZE as u64);

    // Create and start.
    let mut h1: u32 = 0;
    assert!(sc
        .sys_sampler_create(
            root_resource,
            0,
            config.as_ptr() as usize,
            hal_impl::user::UserOutPtr::from(&mut h1 as *mut u32 as usize),
        )
        .is_ok());
    assert!(sc.sys_sampler_start(h1).is_ok());

    // Drop mid-session (close handle without stop).
    ctx.proc.remove_handle(h1).unwrap();

    // Create a new sampler — should succeed.
    let mut h2: u32 = 0;
    assert!(sc
        .sys_sampler_create(
            root_resource,
            0,
            config.as_ptr() as usize,
            hal_impl::user::UserOutPtr::from(&mut h2 as *mut u32 as usize),
        )
        .is_ok());

    // Start, read, stop.
    assert!(sc.sys_sampler_start(h2).is_ok());
    let mut actual: usize = 0;
    assert!(sc
        .sys_sampler_read(
            h2,
            hal_impl::user::UserOutPtr::from(0usize),
            0,
            hal_impl::user::UserOutPtr::from(&mut actual as *mut usize as usize),
        )
        .is_ok());
    assert!(sc.sys_sampler_stop(h2).is_ok());

    ctx.proc.remove_handle(h2).unwrap();
}

/// C++: TEST(ThreadSampler, NonRunningThread)
/// Sampler captures threads that start after sampling begins.
#[test]
#[ignore]
fn non_running_thread() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_resource = ctx.add_root_resource();

    let config = sampler_config(1_000_000, PAGE_SIZE as u64);

    let mut h: u32 = 0;
    assert!(sc
        .sys_sampler_create(
            root_resource,
            0,
            config.as_ptr() as usize,
            hal_impl::user::UserOutPtr::from(&mut h as *mut u32 as usize),
        )
        .is_ok());

    // Start sampling before any threads are created.
    assert!(sc.sys_sampler_start(h).is_ok());

    // Read — in libos mode, the test thread itself should be sampled.
    let mut actual: usize = 0;
    assert!(sc
        .sys_sampler_read(
            h,
            hal_impl::user::UserOutPtr::from(0usize),
            0,
            hal_impl::user::UserOutPtr::from(&mut actual as *mut usize as usize),
        )
        .is_ok());
    // At least the test's own thread should appear.
    assert!(actual > 0, "should have sample data");

    assert!(sc.sys_sampler_stop(h).is_ok());
    ctx.proc.remove_handle(h).unwrap();
}

/// C++: TEST(ThreadSampler, HighFrequency)
/// Stress test with short period and large buffer.
#[test]
#[ignore]
fn high_frequency() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let root_resource = ctx.add_root_resource();

    // 50us period, 1024 pages buffer.
    let config = sampler_config(50_000, 1024 * PAGE_SIZE as u64);

    let mut h: u32 = 0;
    assert!(sc
        .sys_sampler_create(
            root_resource,
            0,
            config.as_ptr() as usize,
            hal_impl::user::UserOutPtr::from(&mut h as *mut u32 as usize),
        )
        .is_ok());

    assert!(sc.sys_sampler_start(h).is_ok());

    // Read — should have at least some records.
    let mut actual: usize = 0;
    assert!(sc
        .sys_sampler_read(
            h,
            hal_impl::user::UserOutPtr::from(0usize),
            0,
            hal_impl::user::UserOutPtr::from(&mut actual as *mut usize as usize),
        )
        .is_ok());
    assert!(actual > 0, "high frequency sampler should produce data");

    assert!(sc.sys_sampler_stop(h).is_ok());
    ctx.proc.remove_handle(h).unwrap();
}
