//! ZX assert C and C++ tests.
//!
//! Faithful port of Fuchsia `zircon/system/utest/core/assert/assert.c`
//! and `zircon/system/utest/core/assert/assert_cpp.cc`.
//!
//! The C++ tests verify `ZX_ASSERT`, `ZX_ASSERT_MSG`, `ZX_DEBUG_ASSERT`,
//! and `ZX_DEBUG_ASSERT_MSG` macros. They use a `noinline` `EchoValue`
//! function and test both passing predicates (should not crash) and
//! failing predicates (should crash via `ASSERT_DEATH`).
//!
//! In Rust, the equivalent mechanisms are:
//! - `assert!` / `assert_eq!` for `ZX_ASSERT`
//! - `debug_assert!` / `debug_assert_eq!` for `ZX_DEBUG_ASSERT`
//! - `std::panic::catch_unwind` for `ASSERT_DEATH`

extern crate alloc;

/// C++ `EchoValue`: a noinline identity function used to prevent
/// the compiler from optimizing away the assert predicate.
#[inline(never)]
fn echo_value(val: u32) -> u32 {
    // Use a black_box-style trick to prevent optimization.
    // The C++ version uses inline asm to launder the value.
    let mut v = val;
    // Safety: this is a no-op that prevents the compiler from
    // constant-folding the return value.
    unsafe {
        core::ptr::read_volatile(&v as *const u32);
    }
    v = val; // Ensure `v` is not optimized away
    v
}

// ── ZxAssertCTest suite ────────────────────────────────────────────

/// C++: TEST(ZxAssertCTest, ZxAssertValidPredicatePasses)
#[test]
fn zx_assert_c_valid_predicate_passes() {
    let expected: u32 = line!();
    assert_eq!(echo_value(expected), expected);
}

/// C++: TEST(ZxAssertCTest, ZxAssertMsgValidPredicatePasses)
#[test]
fn zx_assert_c_msg_valid_predicate_passes() {
    let expected: u32 = line!();
    let val = echo_value(expected);
    assert_eq!(val, expected, "Expected {}, Got {}; ", expected, val);
}

/// C++: TEST(ZxAssertCTest, ZxDebugAssertValidPredicatePasses)
#[test]
fn zx_assert_c_debug_valid_predicate_passes() {
    let expected: u32 = line!();
    debug_assert_eq!(echo_value(expected), expected);
}

/// C++: TEST(ZxAssertCTest, ZxDebugAssertMsgValidPredicatePasses)
#[test]
fn zx_assert_c_debug_msg_valid_predicate_passes() {
    let expected: u32 = line!();
    let val = echo_value(expected);
    debug_assert_eq!(val, expected, "Expected {}, Got {}; ", expected, val);
}

/// C++: TEST(ZxAssertCTest, ZxAssertInvalidPredicateFails)
#[test]
fn zx_assert_c_invalid_predicate_fails() {
    let expected: u32 = line!();
    let actual: u32 = expected + 1;
    let result = std::panic::catch_unwind(|| {
        assert_eq!(echo_value(actual), expected);
    });
    assert!(result.is_err(), "assert! with false predicate should panic");
}

/// C++: TEST(ZxAssertCTest, ZxAssertMsgInvalidPredicateFails)
#[test]
fn zx_assert_c_msg_invalid_predicate_fails() {
    let expected: u32 = line!();
    let actual: u32 = expected + 1;
    let result = std::panic::catch_unwind(|| {
        let val = echo_value(actual);
        assert_eq!(val, expected, "Expected {}, Got {}; ", expected, val);
    });
    assert!(
        result.is_err(),
        "assert! with message and false predicate should panic"
    );
}

/// C++: TEST(ZxAssertCTest, ZxDebugAssertInvalidPredicateFails)
#[test]
fn zx_assert_c_debug_invalid_predicate_fails() {
    let expected: u32 = line!();
    let actual: u32 = expected + 1;

    let result = std::panic::catch_unwind(|| {
        debug_assert_eq!(echo_value(actual), expected);
    });

    if cfg!(debug_assertions) {
        assert!(
            result.is_err(),
            "debug_assert! with false predicate should panic in debug mode"
        );
    } else {
        assert!(
            result.is_ok(),
            "debug_assert! with false predicate should be no-op in release mode"
        );
    }
}

/// C++: TEST(ZxAssertCTest, ZxDebugAssertMsgInvalidPredicateFails)
#[test]
fn zx_assert_c_debug_msg_invalid_predicate_fails() {
    let expected: u32 = line!();
    let actual: u32 = expected + 1;

    let result = std::panic::catch_unwind(|| {
        let val = echo_value(actual);
        debug_assert_eq!(val, expected, "Expected {}, Got {}; ", expected, val);
    });

    if cfg!(debug_assertions) {
        assert!(
            result.is_err(),
            "debug_assert! with message and false predicate should panic in debug mode"
        );
    } else {
        assert!(
            result.is_ok(),
            "debug_assert! with message and false predicate should be no-op in release mode"
        );
    }
}

// ── ZxAssertCppTest suite ──────────────────────────────────────────

/// C++: TEST(ZxAssertCppTest, ZxAssertValidPredicatePasses)
#[test]
fn zx_assert_cpp_valid_predicate_passes() {
    let expected: u32 = line!();
    assert_eq!(echo_value(expected), expected);
}

/// C++: TEST(ZxAssertCppTest, ZxAssertMsgValidPredicatePasses)
#[test]
fn zx_assert_cpp_msg_valid_predicate_passes() {
    let expected: u32 = line!();
    let val = echo_value(expected);
    assert_eq!(val, expected, "Expected {}, Got {}; ", expected, val);
}

/// C++: TEST(ZxAssertCppTest, ZxDebugAssertValidPredicatePasses)
#[test]
fn zx_assert_cpp_debug_valid_predicate_passes() {
    let expected: u32 = line!();
    debug_assert_eq!(echo_value(expected), expected);
}

/// C++: TEST(ZxAssertCppTest, ZxDebugAssertMsgValidPredicatePasses)
#[test]
fn zx_assert_cpp_debug_msg_valid_predicate_passes() {
    let expected: u32 = line!();
    let val = echo_value(expected);
    debug_assert_eq!(val, expected, "Expected {}, Got {}; ", expected, val);
}

/// C++: TEST(ZxAssertCppTest, ZxAssertInvalidPredicateFails)
#[test]
fn zx_assert_cpp_invalid_predicate_fails() {
    let expected: u32 = line!();
    let actual: u32 = expected + 1;
    let result = std::panic::catch_unwind(|| {
        assert_eq!(echo_value(actual), expected);
    });
    assert!(result.is_err(), "assert! with false predicate should panic");
}

/// C++: TEST(ZxAssertCppTest, ZxAssertMsgInvalidPredicateFails)
#[test]
fn zx_assert_cpp_msg_invalid_predicate_fails() {
    let expected: u32 = line!();
    let actual: u32 = expected + 1;
    let result = std::panic::catch_unwind(|| {
        let val = echo_value(actual);
        assert_eq!(val, expected, "Expected {}, Got {}; ", expected, val);
    });
    assert!(
        result.is_err(),
        "assert! with message and false predicate should panic"
    );
}

/// C++: TEST(ZxAssertCppTest, ZxDebugAssertInvalidPredicateFails)
#[test]
fn zx_assert_cpp_debug_invalid_predicate_fails() {
    let expected: u32 = line!();
    let actual: u32 = expected + 1;

    let result = std::panic::catch_unwind(|| {
        debug_assert_eq!(echo_value(actual), expected);
    });

    if cfg!(debug_assertions) {
        assert!(
            result.is_err(),
            "debug_assert! with false predicate should panic in debug mode"
        );
    } else {
        assert!(
            result.is_ok(),
            "debug_assert! with false predicate should be no-op in release mode"
        );
    }
}

/// C++: TEST(ZxAssertCppTest, ZxDebugAssertMsgInvalidPredicateFails)
#[test]
fn zx_assert_cpp_debug_msg_invalid_predicate_fails() {
    let expected: u32 = line!();
    let actual: u32 = expected + 1;

    let result = std::panic::catch_unwind(|| {
        let val = echo_value(actual);
        debug_assert_eq!(val, expected, "Expected {}, Got {}; ", expected, val);
    });

    if cfg!(debug_assertions) {
        assert!(
            result.is_err(),
            "debug_assert! with message and false predicate should panic in debug mode"
        );
    } else {
        assert!(
            result.is_ok(),
            "debug_assert! with message and false predicate should be no-op in release mode"
        );
    }
}
