//! petal counter test -- exercises Zircon Counter syscalls from user mode.
//!
//! Ports of the syscall-layer tests from
//! `zircon-tests/src/core_tests/counter.rs`.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate alloc;
extern crate petal;

use zx::sys::*;

// Signal bits (from zircon-object Signal bitflags)
const ZX_COUNTER_NON_POSITIVE: u32 = 1 << 5; // __ZX_OBJECT_SIGNAL_5
const ZX_USER_SIGNAL_0: u32 = 1 << 24;

// Rights bits from Fuchsia ABI
const ZX_RIGHTS_DEFAULT_COUNTER: u32 = ZX_RIGHT_DUPLICATE
    | ZX_RIGHT_TRANSFER
    | ZX_RIGHT_READ
    | ZX_RIGHT_WRITE
    | ZX_RIGHT_SIGNAL
    | ZX_RIGHT_WAIT
    | ZX_RIGHT_INSPECT;

fn pass(name: &[u8]) {
    zx::debug_write(b"  counter_test: ");
    zx::debug_write(name);
    zx::debug_write(b" PASS\n");
}

fn fail(name: &[u8], msg: &[u8]) {
    zx::debug_write(b"FAIL: counter_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(b"\n");
    zx::Process::exit(1);
}

fn assert_ok(status: ZxStatus, name: &[u8], msg: &[u8]) {
    if status != ZX_OK {
        fail(name, msg);
    }
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        fail(name, msg);
    }
}

fn counter_create(options: u32) -> Result<HandleValue, ZxStatus> {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_counter_create(options as u64, &mut h) };
    if s == ZX_OK {
        Ok(h)
    } else {
        Err(s)
    }
}

fn counter_read(handle: HandleValue) -> Result<i64, ZxStatus> {
    let mut val: i64 = 0;
    let s = unsafe { zx_counter_read(handle, &mut val) };
    if s == ZX_OK {
        Ok(val)
    } else {
        Err(s)
    }
}

fn event_create() -> HandleValue {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut h) };
    if s != ZX_OK {
        fail(b"event_create", b"failed");
    }
    h
}

fn dup_with_rights(handle: HandleValue, rights: u32) -> HandleValue {
    let mut h: HandleValue = 0;
    let s = unsafe { zx_handle_duplicate(handle, rights, &mut h) };
    if s != ZX_OK {
        fail(b"dup_with_rights", b"failed");
    }
    h
}

/// C++: TEST(CounterTest, CreateInitial)
fn create_initial() {
    let name = b"create_initial";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });
    let val = counter_read(h).unwrap_or_else(|_| {
        fail(name, b"read");
        0
    });
    if val != 0 {
        fail(name, b"initial value not 0");
    }
    // Check signals via wait_one with zero timeout
    let mut pending: u32 = 0;
    let s = unsafe { zx_object_wait_one(h, ZX_COUNTER_NON_POSITIVE, 0, &mut pending) };
    // ZX_ERR_TIMED_OUT (-21) is expected with deadline=0 if signal is present
    if s != ZX_OK && s != -21 {
        fail(name, b"wait_one failed");
    }
    if pending & ZX_COUNTER_NON_POSITIVE == 0 {
        fail(name, b"missing NON_POSITIVE");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, CreateInvalidOptions)
fn create_invalid_options() {
    let name = b"create_invalid_options";
    for opt in [1u32, 2, 3, u32::MAX] {
        let r = counter_create(opt);
        if r != Err(ZX_ERR_INVALID_ARGS) {
            fail(name, b"expected INVALID_ARGS");
        }
    }
    pass(name);
}

/// C++: TEST(CounterTest, AddInvalidHandle)
fn add_invalid_handle() {
    let name = b"add_invalid_handle";
    assert_status(
        unsafe { zx_counter_add(0xDEAD, 0) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// C++: TEST(CounterTest, AddWrongType)
fn add_wrong_type() {
    let name = b"add_wrong_type";
    let ev = event_create();
    assert_status(
        unsafe { zx_counter_add(ev, 0) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );
    unsafe { zx_handle_close(ev) };
    pass(name);
}

/// C++: TEST(CounterTest, AddMissingRights)
fn add_missing_rights() {
    let name = b"add_missing_rights";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Without READ
    let no_read = dup_with_rights(h, ZX_RIGHTS_DEFAULT_COUNTER & !ZX_RIGHT_READ);
    assert_status(
        unsafe { zx_counter_add(no_read, 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"no read",
    );
    unsafe { zx_handle_close(no_read) };

    // Without WRITE
    let no_write = dup_with_rights(h, ZX_RIGHTS_DEFAULT_COUNTER & !ZX_RIGHT_WRITE);
    assert_status(
        unsafe { zx_counter_add(no_write, 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"no write",
    );
    unsafe { zx_handle_close(no_write) };

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, AddOverflow)
fn add_overflow() {
    let name = b"add_overflow";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });
    assert_ok(unsafe { zx_counter_add(h, i64::MAX) }, name, b"add MAX");
    assert_status(
        unsafe { zx_counter_add(h, 1) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"overflow",
    );
    let val = counter_read(h).unwrap_or_else(|_| {
        fail(name, b"read");
        0
    });
    if val != i64::MAX {
        fail(name, b"value changed after overflow");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, AddUnderflow)
fn add_underflow() {
    let name = b"add_underflow";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });
    assert_ok(unsafe { zx_counter_add(h, i64::MIN) }, name, b"add MIN");
    assert_status(
        unsafe { zx_counter_add(h, -1) },
        ZX_ERR_OUT_OF_RANGE,
        name,
        b"underflow",
    );
    let val = counter_read(h).unwrap_or_else(|_| {
        fail(name, b"read");
        0
    });
    if val != i64::MIN {
        fail(name, b"value changed after underflow");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, AddHappyCase)
fn add_happy_case() {
    let name = b"add_happy_case";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Initial: 0
    if counter_read(h) != Ok(0) {
        fail(name, b"initial not 0");
    }

    // Add 0: still 0
    assert_ok(unsafe { zx_counter_add(h, 0) }, name, b"add 0");
    if counter_read(h) != Ok(0) {
        fail(name, b"after add 0");
    }

    // Add -1: value = -1
    assert_ok(unsafe { zx_counter_add(h, -1) }, name, b"add -1");
    if counter_read(h) != Ok(-1) {
        fail(name, b"after add -1");
    }

    // Add 1: value = 0
    assert_ok(unsafe { zx_counter_add(h, 1) }, name, b"add 1");
    if counter_read(h) != Ok(0) {
        fail(name, b"after add 1");
    }

    // Add 1: value = 1, transitions to POSITIVE
    assert_ok(unsafe { zx_counter_add(h, 1) }, name, b"add 1 again");
    if counter_read(h) != Ok(1) {
        fail(name, b"after second add 1");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, ReadInvalidHandle)
fn read_invalid_handle() {
    let name = b"read_invalid_handle";
    if counter_read(0xDEAD) != Err(ZX_ERR_BAD_HANDLE) {
        fail(name, b"expected BAD_HANDLE");
    }
    pass(name);
}

/// C++: TEST(CounterTest, ReadWrongType)
fn read_wrong_type() {
    let name = b"read_wrong_type";
    let ev = event_create();
    if counter_read(ev) != Err(ZX_ERR_WRONG_TYPE) {
        fail(name, b"expected WRONG_TYPE");
    }
    unsafe { zx_handle_close(ev) };
    pass(name);
}

/// C++: TEST(CounterTest, ReadMissingRights)
fn read_missing_rights() {
    let name = b"read_missing_rights";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });
    let no_read = dup_with_rights(h, ZX_RIGHTS_DEFAULT_COUNTER & !ZX_RIGHT_READ);
    if counter_read(no_read) != Err(ZX_ERR_ACCESS_DENIED) {
        fail(name, b"expected ACCESS_DENIED");
    }
    unsafe { zx_handle_close(no_read) };
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, ReadHappyCase)
fn read_happy_case() {
    let name = b"read_happy_case";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });
    if counter_read(h) != Ok(0) {
        fail(name, b"initial");
    }
    assert_ok(unsafe { zx_counter_write(h, 42) }, name, b"write 42");
    if counter_read(h) != Ok(42) {
        fail(name, b"after write 42");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, WriteInvalidHandle)
fn write_invalid_handle() {
    let name = b"write_invalid_handle";
    assert_status(
        unsafe { zx_counter_write(0xDEAD, 0) },
        ZX_ERR_BAD_HANDLE,
        name,
        b"",
    );
    pass(name);
}

/// C++: TEST(CounterTest, WriteWrongType)
fn write_wrong_type() {
    let name = b"write_wrong_type";
    let ev = event_create();
    assert_status(
        unsafe { zx_counter_write(ev, 0) },
        ZX_ERR_WRONG_TYPE,
        name,
        b"",
    );
    unsafe { zx_handle_close(ev) };
    pass(name);
}

/// C++: TEST(CounterTest, WriteMissingRights)
fn write_missing_rights() {
    let name = b"write_missing_rights";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Without WRITE -> ACCESS_DENIED
    let no_write = dup_with_rights(h, ZX_RIGHTS_DEFAULT_COUNTER & !ZX_RIGHT_WRITE);
    assert_status(
        unsafe { zx_counter_write(no_write, 0) },
        ZX_ERR_ACCESS_DENIED,
        name,
        b"no write",
    );
    unsafe { zx_handle_close(no_write) };

    // Without READ -> OK (write doesn't require READ)
    let no_read = dup_with_rights(h, ZX_RIGHTS_DEFAULT_COUNTER & !ZX_RIGHT_READ);
    assert_ok(
        unsafe { zx_counter_write(no_read, 0) },
        name,
        b"no read should succeed",
    );
    unsafe { zx_handle_close(no_read) };

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, WriteHappyCase)
fn write_happy_case() {
    let name = b"write_happy_case";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    if counter_read(h) != Ok(0) {
        fail(name, b"initial");
    }

    assert_ok(unsafe { zx_counter_write(h, 0) }, name, b"write 0");
    if counter_read(h) != Ok(0) {
        fail(name, b"after write 0");
    }

    assert_ok(unsafe { zx_counter_write(h, -1) }, name, b"write -1");
    if counter_read(h) != Ok(-1) {
        fail(name, b"after write -1");
    }

    assert_ok(unsafe { zx_counter_write(h, 1) }, name, b"write 1");
    if counter_read(h) != Ok(1) {
        fail(name, b"after write 1");
    }

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, Signal)
fn signal() {
    let name = b"signal";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });

    // Set USER_SIGNAL_0
    assert_ok(
        unsafe { zx_object_signal(h, 0, ZX_USER_SIGNAL_0) },
        name,
        b"set signal",
    );

    // Clear USER_SIGNAL_0
    assert_ok(
        unsafe { zx_object_signal(h, ZX_USER_SIGNAL_0, 0) },
        name,
        b"clear signal",
    );

    unsafe { zx_handle_close(h) };
    pass(name);
}

/// C++: TEST(CounterTest, ReadInvalidPointer)
fn read_invalid_pointer() {
    let name = b"read_invalid_pointer";
    let h = counter_create(0).unwrap_or_else(|_| {
        fail(name, b"create");
        0
    });
    let s = unsafe { zx_counter_read(h, core::ptr::null_mut()) };
    if s == ZX_OK {
        fail(name, b"null pointer read should fail");
    }
    unsafe { zx_handle_close(h) };
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"counter_test: starting\n");

    create_initial();
    create_invalid_options();
    add_invalid_handle();
    add_wrong_type();
    add_missing_rights();
    add_overflow();
    add_underflow();
    add_happy_case();
    read_invalid_handle();
    read_wrong_type();
    read_missing_rights();
    read_happy_case();
    write_invalid_handle();
    write_wrong_type();
    write_missing_rights();
    write_happy_case();
    signal();
    read_invalid_pointer();

    zx::debug_write(b"counter_test: all tests passed\n");
}
