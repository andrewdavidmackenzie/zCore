//! Additional zCore-specific assert tests.
//!
//! These are NOT 1:1 ports of C++ core-tests. They verify that
//! Rust assert macros work correctly with kernel error types.

use crate::helpers::TestContext;
use zircon_object::ZxError;

/// Verify ZX_OK equivalent (Ok(())).
#[test]
fn assert_ok() {
    let _ctx = TestContext::new();
    let result: Result<(), ZxError> = Ok(());
    assert!(result.is_ok(), "Ok should be ok");
}

/// Verify error values are distinct from Ok.
#[test]
fn assert_not_ok() {
    let _ctx = TestContext::new();
    let result: Result<(), ZxError> = Err(ZxError::BAD_HANDLE);
    assert!(result.is_err(), "Err should be err");
    assert_eq!(result.err().unwrap(), ZxError::BAD_HANDLE);
}

/// Various error values have correct identity.
#[test]
fn assert_error_values() {
    let _ctx = TestContext::new();
    assert_ne!(ZxError::BAD_HANDLE, ZxError::INVALID_ARGS);
    assert_ne!(ZxError::NOT_FOUND, ZxError::PEER_CLOSED);
    assert_ne!(ZxError::SHOULD_WAIT, ZxError::BAD_STATE);
}

/// Error codes should be matchable.
#[test]
fn assert_in_range() {
    let _ctx = TestContext::new();
    let err = ZxError::OUT_OF_RANGE;
    match err {
        ZxError::OUT_OF_RANGE => {} // expected
        _ => panic!("should match OUT_OF_RANGE"),
    }
}

/// Error values have debug representations.
#[test]
fn assert_debug_string() {
    let _ctx = TestContext::new();
    let err = ZxError::BAD_HANDLE;
    let debug = format!("{:?}", err);
    assert!(!debug.is_empty(), "debug format should not be empty");
}

/// Error equality comparison works.
#[test]
fn assert_eq() {
    let _ctx = TestContext::new();
    let e1 = ZxError::BAD_HANDLE;
    let e2 = ZxError::BAD_HANDLE;
    assert_eq!(e1, e2, "same error should be equal");
}

/// Error inequality comparison works.
#[test]
fn assert_neq() {
    let _ctx = TestContext::new();
    let e1 = ZxError::BAD_HANDLE;
    let e2 = ZxError::INVALID_ARGS;
    assert_ne!(e1, e2, "different errors should not be equal");
}

/// Error values can be copied and compared.
#[test]
fn assert_copy() {
    let _ctx = TestContext::new();
    let err = ZxError::PEER_CLOSED;
    let copy = err;
    assert_eq!(err, copy, "copied error should be equal to original");
}
