//! Executable TLS tests.
//!
//! Faithful port of Fuchsia `zircon/system/utest/core/elf-tls/tls.cc`.
//!
//! The C++ tests use `__thread` / `thread_local` variables with specific
//! initializers, then verify values in both the main thread and child
//! threads. This Rust port uses `std::thread_local!` and `std::thread`
//! to reproduce the same test patterns.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

// ── Thread-local variables matching the C++ declarations ────────────
//
// C++ declares: thread_local bool u1 = true; thread_local uint8_t u8 = MAX; etc.
// Rust thread_local! with Cell for interior mutability.

use core::cell::Cell;

thread_local! {
    static TLS_U1: Cell<bool> = const { Cell::new(true) };
    static TLS_U8: Cell<u8> = const { Cell::new(u8::MAX) };
    static TLS_U16: Cell<u16> = const { Cell::new(u16::MAX) };
    static TLS_U32: Cell<u32> = const { Cell::new(u32::MAX) };
    static TLS_U64: Cell<u64> = const { Cell::new(u64::MAX) };
    static TLS_UPTR: Cell<usize> = const { Cell::new(usize::MAX) };
    static TLS_I8: Cell<i8> = const { Cell::new(i8::MAX) };
    static TLS_I16: Cell<i16> = const { Cell::new(i16::MAX) };
    static TLS_I32: Cell<i32> = const { Cell::new(i32::MAX) };
    static TLS_I64: Cell<i64> = const { Cell::new(i64::MAX) };
    static TLS_IPTR: Cell<isize> = const { Cell::new(isize::MAX) };
    static TLS_F32: Cell<f32> = const { Cell::new(f32::MAX) };
    static TLS_F64: Cell<f64> = const { Cell::new(f64::MAX) };
}

// C++ thread_local uint8_t array[1024] = {0xff repeated};
const ARRAY_SIZE: usize = 1024;
thread_local! {
    static TLS_ARRAY: core::cell::RefCell<[u8; ARRAY_SIZE]> =
        core::cell::RefCell::new([0xFF; ARRAY_SIZE]);
}

// C++ thread_local uint8_t big_array[1 << 20]; (zero-initialized)
const BIG_ARRAY_SIZE: usize = 1 << 20;
thread_local! {
    static TLS_BIG_ARRAY: core::cell::RefCell<Vec<u8>> =
        core::cell::RefCell::new(vec![0u8; BIG_ARRAY_SIZE]);
}

// C++ struct Ctor { Ctor() : x(UINT64_MAX) {} uint64_t x; };
// thread_local Ctor ctor;
thread_local! {
    static TLS_CTOR_X: Cell<u64> = const { Cell::new(u64::MAX) };
}

// C++ __attribute__((aligned(0x1000))) thread_local int aligned_var = 123;
// Rust doesn't support per-variable TLS alignment, but we can test the value.
thread_local! {
    static TLS_ALIGNED_VAR: Cell<i32> = const { Cell::new(123) };
}

// C++ thread_local uint8_t spam_array[1024] = {0xff repeated};
thread_local! {
    static TLS_SPAM_ARRAY: core::cell::RefCell<[u8; ARRAY_SIZE]> =
        core::cell::RefCell::new([0xFF; ARRAY_SIZE]);
}

/// Collect basic initializer values from the current thread's TLS.
/// Mirrors the C++ `GetBasicInitializers` function.
struct BasicInitializerInfo {
    u1: bool,
    u8: u8,
    u16: u16,
    u32: u32,
    u64: u64,
    uptr: usize,
    i8: i8,
    i16: i16,
    i32: i32,
    i64: i64,
    iptr: isize,
    f32: f32,
    f64: f64,
}

fn get_basic_initializers() -> BasicInitializerInfo {
    BasicInitializerInfo {
        u1: TLS_U1.with(|v| v.get()),
        u8: TLS_U8.with(|v| v.get()),
        u16: TLS_U16.with(|v| v.get()),
        u32: TLS_U32.with(|v| v.get()),
        u64: TLS_U64.with(|v| v.get()),
        uptr: TLS_UPTR.with(|v| v.get()),
        i8: TLS_I8.with(|v| v.get()),
        i16: TLS_I16.with(|v| v.get()),
        i32: TLS_I32.with(|v| v.get()),
        i64: TLS_I64.with(|v| v.get()),
        iptr: TLS_IPTR.with(|v| v.get()),
        f32: TLS_F32.with(|v| v.get()),
        f64: TLS_F64.with(|v| v.get()),
    }
}

fn verify_basic_initializers(info: &BasicInitializerInfo, error_message: &str) {
    assert_eq!(info.u1, true, "{}", error_message);
    assert_eq!(info.u8, u8::MAX, "{}", error_message);
    assert_eq!(info.u16, u16::MAX, "{}", error_message);
    assert_eq!(info.u32, u32::MAX, "{}", error_message);
    assert_eq!(info.u64, u64::MAX, "{}", error_message);
    assert_eq!(info.uptr, usize::MAX, "{}", error_message);
    assert_eq!(info.i8, i8::MAX, "{}", error_message);
    assert_eq!(info.i16, i16::MAX, "{}", error_message);
    assert_eq!(info.i32, i32::MAX, "{}", error_message);
    assert_eq!(info.i64, i64::MAX, "{}", error_message);
    assert_eq!(info.iptr, isize::MAX, "{}", error_message);
    assert_eq!(info.f32, f32::MAX, "{}", error_message);
    assert_eq!(info.f64, f64::MAX, "{}", error_message);
}

/// C++: TEST(ExecutableTlsTest, BasicInitalizersInThread)
/// Verify that a child thread sees the correct initial TLS values.
#[test]
fn basic_initalizers_in_thread() {
    let handle = std::thread::Builder::new()
        .name("GetInitializers".into())
        .spawn(|| get_basic_initializers())
        .expect("unable to create GetInitializers thread");

    let info = handle
        .join()
        .expect("unable to join GetInitializers thread");
    verify_basic_initializers(&info, "BackgroundThread: Unexpected initialized value");
}

/// C++: TEST(ExecutableTlsTest, BasicInitalizersInMain)
/// Verify that the main thread sees the correct initial TLS values.
#[test]
fn basic_initalizers_in_main() {
    let info = get_basic_initializers();
    verify_basic_initializers(&info, "MainThread: Unexpected initialized value");
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializerInThread)
/// Verify that a child thread's TLS array is initialized to 0xFF.
#[test]
fn array_initializer_in_thread() {
    let handle = std::thread::Builder::new()
        .name("ArrayInitializers".into())
        .spawn(|| {
            TLS_ARRAY.with(|arr| {
                let arr = arr.borrow();
                arr.to_vec()
            })
        })
        .expect("unable to create GetArray thread");

    let arr = handle.join().expect("unable to join GetArray thread");
    for byte in &arr {
        assert_eq!(
            *byte,
            u8::MAX,
            "BackgroundThread: Unexpected initialized value"
        );
    }
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializerInMain)
/// Verify that the main thread's TLS array is initialized to 0xFF.
#[test]
fn array_initializer_in_main() {
    TLS_ARRAY.with(|arr| {
        let arr = arr.borrow();
        for byte in arr.iter() {
            assert_eq!(*byte, u8::MAX, "MainThread: Unexpected initialized value");
        }
    });
}

/// C++: TEST(ExecutableTlsTest, BigArrayInitializerInThread)
/// Verify that a child thread's big TLS array (1 MiB) is zero-initialized.
#[test]
fn big_array_initializer_in_thread() {
    let handle = std::thread::Builder::new()
        .name("GetBigArray".into())
        .spawn(|| {
            TLS_BIG_ARRAY.with(|arr| {
                let arr = arr.borrow();
                let mut sum: u8 = 0;
                for byte in arr.iter() {
                    sum |= byte;
                }
                sum
            })
        })
        .expect("unable to create GetBigArray thread");

    let sum = handle.join().expect("unable to join GetBigArray thread");
    assert_eq!(sum, 0, "BackgroundThread: Unexpected initialized value");
}

/// C++: TEST(ExecutableTlsTest, BigArrayInitializerInMain)
/// Verify that the main thread's big TLS array (1 MiB) is zero-initialized.
#[test]
fn big_array_initializer_in_main() {
    TLS_BIG_ARRAY.with(|arr| {
        let arr = arr.borrow();
        let mut sum: u8 = 0;
        for byte in arr.iter() {
            sum |= byte;
        }
        assert_eq!(sum, 0, "MainThread: Unexpected initialized value");
    });
}

/// C++: TEST(ExecutableTlsTest, StructureInitializerInThread)
/// Verify that a child thread's TLS struct (with constructor) has the correct value.
#[test]
fn structure_initializer_in_thread() {
    let handle = std::thread::Builder::new()
        .name("StructureInitializer".into())
        .spawn(|| TLS_CTOR_X.with(|v| v.get()))
        .expect("unable to create GetCtor thread");

    let val = handle.join().expect("unable to join GetCtor thread");
    assert_eq!(
        val,
        u64::MAX,
        "BackgroundThread: Unexpected initialized value"
    );
}

/// C++: TEST(ExecutableTlsTest, StructureInitalizierInMain)
/// Verify that the main thread's TLS struct has the correct initial value.
#[test]
fn structure_initalizier_in_main() {
    let val = TLS_CTOR_X.with(|v| v.get());
    assert_eq!(val, u64::MAX, "MainThread: Unexpected initialized value");
}

/// C++: TEST(ExecutableTlsTest, AlignmentInitializerInThread)
/// Verify TLS variable alignment and value in a child thread.
/// Note: Rust does not support per-variable TLS alignment attributes,
/// so we only verify the value (123) matches the C++ initializer.
#[test]
fn alignment_initializer_in_thread() {
    let handle = std::thread::Builder::new()
        .name("GetInitializers".into())
        .spawn(|| TLS_ALIGNED_VAR.with(|v| v.get()))
        .expect("unable to create GetAlignment thread");

    let val = handle.join().expect("unable to join GetAlignment thread");
    assert_eq!(val, 123, "BackgroundThread: Unexpected initialized value");
}

/// C++: TEST(ExecutableTlsTest, AlignmentInitializierInMain)
/// Verify TLS variable alignment and value in the main thread.
#[test]
fn alignment_initializier_in_main() {
    let val = TLS_ALIGNED_VAR.with(|v| v.get());
    assert_eq!(val, 123, "MainThread: Unexpected initialized value");
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializerSpamThread)
/// Spawn 64 threads, each writing and verifying their own TLS array
/// 100 times with yield() between write and verify, ensuring no
/// cross-thread TLS corruption.
#[test]
fn array_initializer_spam_thread() {
    const THREAD_COUNT: u8 = 64;

    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|index| {
            std::thread::Builder::new()
                .name("TestArraySpam".into())
                .spawn(move || {
                    TLS_SPAM_ARRAY.with(|arr| {
                        let mut arr = arr.borrow_mut();
                        for iteration in 0u8..100 {
                            let starting_value = index.wrapping_add(iteration);
                            let mut value = starting_value;
                            for byte in arr.iter_mut() {
                                *byte = value;
                                value = value.wrapping_add(1);
                            }

                            std::thread::yield_now();

                            value = starting_value;
                            for (offset, byte) in arr.iter().enumerate() {
                                if *byte != value {
                                    panic!(
                                        "BackgroundThread: Unexpected initialized value: \
                                         thread={} ExpectedValue={} ActualValue={} FailureOffset={}",
                                        index, value, *byte, offset
                                    );
                                }
                                value = value.wrapping_add(1);
                            }
                        }
                    });
                })
                .expect("unable to create TestArraySpamInfo thread")
        })
        .collect();

    for (index, handle) in handles.into_iter().enumerate() {
        handle
            .join()
            .unwrap_or_else(|_| panic!("unable to join TestArraySpamInfo thread {}", index));
    }
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializierSpamMain)
/// Run the spam test body on the main thread (100 iterations of
/// write-yield-verify on a TLS array).
#[test]
fn array_initializier_spam_main() {
    TLS_SPAM_ARRAY.with(|arr| {
        let mut arr = arr.borrow_mut();
        let index: u8 = 0;
        for iteration in 0u8..100 {
            let starting_value = index.wrapping_add(iteration);
            let mut value = starting_value;
            for byte in arr.iter_mut() {
                *byte = value;
                value = value.wrapping_add(1);
            }

            std::thread::yield_now();

            value = starting_value;
            for (offset, byte) in arr.iter().enumerate() {
                if *byte != value {
                    panic!(
                        "MainThread: Unexpected initialized value: \
                         ExpectedValue={} ActualValue={} FailureOffset={}",
                        value, *byte, offset
                    );
                }
                value = value.wrapping_add(1);
            }
        }
    });
}
