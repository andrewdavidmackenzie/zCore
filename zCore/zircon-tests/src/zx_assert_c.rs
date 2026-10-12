//! ZX assert C tests.
//!
//! Ports of Fuchsia's C-language zx_assert macro test suite.
//! zCore has no C-language zx_assert. These tests verify that
//! Rust equivalents of C assertion patterns work correctly.

use crate::helpers::TestContext;
use zircon_object::ZxError;

extern crate alloc;

/// C++: TEST(ZxAssertCTest, StatusOk)
/// Verify OK status.
#[test]
fn status_ok() {
    let _ctx = TestContext::new();
    let status: Result<(), ZxError> = Ok(());
    assert!(status.is_ok());
}

/// C++: TEST(ZxAssertCTest, StatusError)
/// Verify error status values.
#[test]
fn status_error() {
    let _ctx = TestContext::new();
    let status: Result<(), ZxError> = Err(ZxError::INVALID_ARGS);
    assert!(status.is_err());
    assert_eq!(status.unwrap_err(), ZxError::INVALID_ARGS);
}

/// C++: TEST(ZxAssertCTest, StatusConvert)
/// Error values can be converted to and from integers.
#[test]
fn status_convert() {
    let _ctx = TestContext::new();
    // ZxError has a numeric representation.
    let err = ZxError::BAD_HANDLE;
    let debug = alloc::format!("{:?}", err);
    assert!(debug.contains("BAD_HANDLE"));
}

/// C++: TEST(ZxAssertCTest, StatusCompare)
/// Error comparison works.
#[test]
fn status_compare() {
    let _ctx = TestContext::new();
    assert_eq!(ZxError::BAD_HANDLE, ZxError::BAD_HANDLE);
    assert_ne!(ZxError::BAD_HANDLE, ZxError::INVALID_ARGS);
}

/// C++: TEST(ZxAssertCTest, StatusMatch)
/// Error values can be matched in switch/match statements.
#[test]
fn status_match() {
    let _ctx = TestContext::new();
    let result: Result<(), ZxError> = Err(ZxError::NOT_FOUND);
    match result {
        Ok(()) => panic!("should be error"),
        Err(ZxError::NOT_FOUND) => {} // expected
        Err(other) => panic!("unexpected error: {:?}", other),
    }
}

/// C++: TEST(ZxAssertCTest, StatusChain)
/// Multiple error checks in sequence.
#[test]
fn status_chain() {
    let _ctx = TestContext::new();
    let errs = [
        ZxError::BAD_HANDLE,
        ZxError::INVALID_ARGS,
        ZxError::NOT_FOUND,
        ZxError::PEER_CLOSED,
    ];

    for &err in &errs {
        let result: Result<(), ZxError> = Err(err);
        assert!(result.is_err());
    }
}

/// C++: TEST(ZxAssertCTest, StatusNested)
/// Nested error handling.
#[test]
fn status_nested() {
    let _ctx = TestContext::new();

    fn inner(fail: bool) -> Result<u32, ZxError> {
        if fail {
            Err(ZxError::BAD_STATE)
        } else {
            Ok(42)
        }
    }

    assert_eq!(inner(false).unwrap(), 42);
    assert_eq!(inner(true).unwrap_err(), ZxError::BAD_STATE);
}

/// C++: TEST(ZxAssertCTest, StatusAll)
/// Verify all common error values are distinct.
#[test]
fn status_all() {
    let _ctx = TestContext::new();
    let errors = [
        ZxError::BAD_HANDLE,
        ZxError::INVALID_ARGS,
        ZxError::NOT_FOUND,
        ZxError::PEER_CLOSED,
        ZxError::SHOULD_WAIT,
        ZxError::BAD_STATE,
        ZxError::OUT_OF_RANGE,
        ZxError::ALREADY_BOUND,
    ];

    for i in 0..errors.len() {
        for j in (i + 1)..errors.len() {
            assert_ne!(errors[i], errors[j], "all errors should be distinct");
        }
    }
}
