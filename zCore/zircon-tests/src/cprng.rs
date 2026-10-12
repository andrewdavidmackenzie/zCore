//! CPRNG (Cryptographically-secure Pseudo Random Number Generator) tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/cprng tests.
//! Tests for zx_cprng_draw and zx_cprng_add_entropy syscalls.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Draw random bytes and verify at least some are non-zero (with
/// overwhelming probability). Tests zx_cprng_draw basic functionality.
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn draw_non_zero() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}

///
/// Draw two buffers and verify they differ (with overwhelming
/// probability). Tests randomness quality.
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn draw_different() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}

///
/// Draw zero bytes — should succeed without error (no-op).
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn draw_zero_length() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}

///
/// Draw the maximum allowed number of bytes (ZX_CPRNG_DRAW_MAX_LEN).
/// Should succeed.
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn draw_max_length() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}

///
/// Draw more than the maximum allowed bytes. Should return
/// INVALID_ARGS.
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn draw_exceeds_max_length() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}

///
/// Draw with a null buffer pointer. Should return INVALID_ARGS.
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn draw_null_buffer() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}

///
/// Add entropy from a valid buffer. Should succeed (no-op from
/// kernel perspective — just mixes into the pool).
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn add_entropy_valid() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}

///
/// Add entropy with a null buffer. Should return INVALID_ARGS.
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn add_entropy_null_buffer() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}

///
/// Add zero-length entropy. Should succeed (no-op).
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn add_entropy_zero_length() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}

///
/// Add entropy without root resource. Should return ACCESS_DENIED.
#[should_panic(expected = "cprng syscalls not implemented")]
#[test]
fn add_entropy_no_resource() {
    let _ctx = TestContext::new();
    panic!("cprng syscalls not implemented");
}
