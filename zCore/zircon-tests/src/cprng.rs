//! CPRNG (Cryptographically-secure Pseudo Random Number Generator) tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/cprng tests.
//! Tests for zx_cprng_draw and zx_cprng_add_entropy syscalls.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};

///
/// Draw random bytes and verify at least some are non-zero (with
/// overwhelming probability). Tests zx_cprng_draw basic functionality.
#[test]
fn draw_non_zero() {
    let _ctx = TestContext::new();
    let mut buf = [0u8; 32];
    hal_impl::rand::fill_random(&mut buf);
    assert!(
        buf.iter().any(|&b| b != 0),
        "random buffer should contain at least one non-zero byte"
    );
}

///
/// Draw two buffers and verify they differ (with overwhelming
/// probability). Tests randomness quality.
#[test]
fn draw_different() {
    let _ctx = TestContext::new();
    let mut buf1 = [0u8; 32];
    let mut buf2 = [0u8; 32];
    hal_impl::rand::fill_random(&mut buf1);
    hal_impl::rand::fill_random(&mut buf2);
    assert_ne!(
        buf1, buf2,
        "two random buffers should differ (with overwhelming probability)"
    );
}

///
/// Draw zero bytes -- should succeed without error (no-op).
#[test]
fn draw_zero_length() {
    let _ctx = TestContext::new();
    let mut buf = [0u8; 0];
    // Should not panic.
    hal_impl::rand::fill_random(&mut buf);
}

///
/// Draw the maximum allowed number of bytes (ZX_CPRNG_DRAW_MAX_LEN = 256).
/// Should succeed.
#[test]
fn draw_max_length() {
    let _ctx = TestContext::new();
    let mut buf = [0u8; 256];
    hal_impl::rand::fill_random(&mut buf);
    // With 256 random bytes, at least some should be non-zero.
    assert!(
        buf.iter().any(|&b| b != 0),
        "256 random bytes should contain at least one non-zero byte"
    );
}

///
/// Draw more than the maximum allowed bytes. Should return
/// INVALID_ARGS.
#[test]
fn draw_exceeds_max_length() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // ZX_CPRNG_DRAW_MAX_LEN = 256; requesting 257 should fail.
    let mut buf = [0u8; 257];
    let out = UserOutPtr::from(buf.as_mut_ptr() as usize);
    let result = sc.sys_cprng_draw_once(out, 257);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::INVALID_ARGS),
        "drawing > 256 bytes should return INVALID_ARGS"
    );
}

///
/// Draw with a null buffer pointer. Should return an error.
#[test]
fn draw_null_buffer() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Null pointer with non-zero length should fail.
    let out = UserOutPtr::from(0usize);
    let result = sc.sys_cprng_draw_once(out, 16);
    assert!(
        result.is_err(),
        "drawing to null buffer with len > 0 should fail"
    );
}

///
/// Add entropy from a valid buffer. Should succeed (no-op from
/// kernel perspective -- just mixes into the pool).
#[test]
fn add_entropy_valid() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let entropy = [0x42u8; 32];
    let inp = UserInPtr::from(entropy.as_ptr() as usize);
    let result = sc.sys_cprng_add_entropy(inp, entropy.len());
    assert!(
        result.is_ok(),
        "adding valid entropy should succeed, got {:?}",
        result
    );
}

///
/// Add entropy with a null buffer. Should return an error.
#[test]
fn add_entropy_null_buffer() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Null pointer with non-zero length should fail.
    let inp = UserInPtr::from(0usize);
    let result = sc.sys_cprng_add_entropy(inp, 16);
    assert!(
        result.is_err(),
        "adding entropy from null buffer with len > 0 should fail"
    );
}

///
/// Add zero-length entropy. Should succeed (no-op).
#[test]
fn add_entropy_zero_length() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Zero length is a no-op and should succeed regardless of pointer.
    let inp = UserInPtr::from(0usize);
    let result = sc.sys_cprng_add_entropy(inp, 0);
    assert!(
        result.is_ok(),
        "adding zero-length entropy should succeed, got {:?}",
        result
    );
}

///
/// Add entropy without root resource.
/// Note: Unlike Fuchsia, zCore's sys_cprng_add_entropy does not require
/// a root resource. This test verifies the call succeeds.
#[test]
fn add_entropy_no_resource() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // In zCore, add_entropy does not require a root resource,
    // so this should succeed with valid input.
    let entropy = [0xABu8; 64];
    let inp = UserInPtr::from(entropy.as_ptr() as usize);
    let result = sc.sys_cprng_add_entropy(inp, entropy.len());
    assert!(
        result.is_ok(),
        "add_entropy should succeed without root resource in zCore, got {:?}",
        result
    );
}
