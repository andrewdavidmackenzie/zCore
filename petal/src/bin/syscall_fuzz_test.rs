//! petal syscall_fuzz test -- adversarial syscall arguments from userspace (#913).
//!
//! Each case feeds a syscall a hostile-but-plausible argument pattern
//! (invalid bit patterns, type/rights mismatches, duplicate handle use,
//! invalid opcodes) and asserts the kernel returns an error instead of
//! panicking or hanging. The channel-write dispositions are the first
//! target family: `zx_channel_write_etc` takes a fully user-controlled
//! disposition array, and the kernel-side processing historically
//! unwrapped on invalid rights bits (issue #913, Tier 1).

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate petal;

use zx::sys::*;

/// `ZX_HANDLE_OP_MOVE` (kernel: channel.rs ZX_HANDLE_OP_MOVE = 0).
const OP_MOVE: u32 = 0;
/// `ZX_HANDLE_OP_DUP` (kernel: channel.rs ZX_HANDLE_OP_DUP = 1).
const OP_DUP: u32 = 1;

fn pass(name: &[u8]) {
    zx::debug_write(b"  syscall_fuzz_test: ");
    zx::debug_write(name);
    zx::debug_write(
        b" PASS
",
    );
}

fn fail(name: &[u8], msg: &[u8]) -> ! {
    zx::debug_write(b"FAIL: syscall_fuzz_test: ");
    zx::debug_write(name);
    zx::debug_write(b" - ");
    zx::debug_write(msg);
    zx::debug_write(
        b"
",
    );
    zx::Process::exit(1);
}

fn assert_status(got: ZxStatus, expected: ZxStatus, name: &[u8], msg: &[u8]) {
    if got != expected {
        fail(name, msg);
    }
}

fn channel_create() -> (HandleValue, HandleValue) {
    let mut h0: HandleValue = 0;
    let mut h1: HandleValue = 0;
    let s = unsafe { zx_channel_create(0, &mut h0, &mut h1) };
    if s != ZX_OK {
        fail(b"channel_create", b"failed");
    }
    (h0, h1)
}

fn event_create() -> HandleValue {
    let mut ev: HandleValue = 0;
    let s = unsafe { zx_event_create(0, &mut ev) };
    if s != ZX_OK {
        fail(b"event_create", b"failed");
    }
    ev
}

/// MOVE an event through the channel with a valid disposition — the
/// happy path, proving the hardening did not break normal transfers.
fn write_etc_move_ok() {
    let name = b"write_etc_move_ok";
    let (ch0, ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: OP_MOVE,
        handle: ev,
        type_: 0,
        rights: 0, // 0 = default/same rights per the kernel's check
        result: 0,
    }];
    // 0 means "default/same rights" per the ABI check in the kernel
    // (`rights != SAME_RIGHTS` guards the conversion), so keep 0.
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_OK, name, b"expected OK for a valid MOVE");
    assert_status(dispositions[0].result, ZX_OK, name, b"disposition result");
    // Read it back from the peer end: the event handle arrived there.
    let mut got: HandleValue = 0;
    let s = unsafe {
        zx_channel_read(
            ch1,
            0,
            core::ptr::null_mut(),
            &mut got,
            0,
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    if s != ZX_OK {
        fail(name, b"read back failed");
    }
    unsafe {
        zx_handle_close(got);
        zx_handle_close(ch1);
        zx_handle_close(ch0);
    }
    pass(name);
}

/// Invalid rights bit pattern (bit 25 is outside the defined Rights
/// set). Before #913's fix this reached `Rights::from_bits(..)
/// .unwrap()` in the kernel and panicked — a one-syscall kernel kill.
fn write_etc_invalid_rights_bit() {
    let name = b"write_etc_invalid_rights_bit";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: OP_MOVE,
        handle: ev,
        type_: 0,
        rights: 1 << 25, // undefined Rights bit
        result: 0,
    }];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"expected INVALID_ARGS");
    assert_status(
        dispositions[0].result,
        ZX_ERR_INVALID_ARGS,
        name,
        b"disposition result",
    );
    // MOVE semantics: the source handle is consumed even on failure.
    let mut out: HandleValue = 0;
    let s = unsafe { zx_handle_duplicate(ev, 0, &mut out) };
    assert_status(
        s,
        ZX_ERR_BAD_HANDLE,
        name,
        b"MOVE should consume the source",
    );
    unsafe {
        zx_handle_close(ch0);
    }
    pass(name);
}

/// The same handle moved twice in one disposition array: the first
/// MOVE consumes it, the second must report BAD_HANDLE (not a
/// double-remove panic).
fn write_etc_move_twice() {
    let name = b"write_etc_move_twice";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [
        HandleDisposition {
            op: OP_MOVE,
            handle: ev,
            type_: 0,
            rights: 0,
            result: 0,
        },
        HandleDisposition {
            op: OP_MOVE,
            handle: ev,
            type_: 0,
            rights: 0,
            result: 0,
        },
    ];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 2) };
    assert_status(s, ZX_ERR_BAD_HANDLE, name, b"expected BAD_HANDLE overall");
    let second = dispositions[1].result;
    assert_status(
        second,
        ZX_ERR_BAD_HANDLE,
        name,
        b"second disposition result",
    );
    unsafe {
        zx_handle_close(ch0);
    }
    pass(name);
}

/// An operation code outside MOVE/DUP.
fn write_etc_invalid_op() {
    let name = b"write_etc_invalid_op";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: 0xFF,
        handle: ev,
        type_: 0,
        rights: 0,
        result: 0,
    }];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"expected INVALID_ARGS");
    unsafe {
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}

/// A nonzero expected object type that does not match the handle.
fn write_etc_type_mismatch() {
    let name = b"write_etc_type_mismatch";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: OP_MOVE,
        handle: ev,
        type_: 0xFF, // never a valid object type
        rights: 0,
        result: 0,
    }];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_ERR_WRONG_TYPE, name, b"expected WRONG_TYPE");
    unsafe {
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}

/// DUP requesting rights the source does not carry (EXECUTE on an
/// event): valid Rights bits, but not a subset — INVALID_ARGS, and no
/// handle with elevated rights may be delivered.
fn write_etc_dup_elevated_rights() {
    let name = b"write_etc_dup_elevated_rights";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [HandleDisposition {
        op: OP_DUP,
        handle: ev,
        type_: 0,
        rights: ZX_RIGHT_EXECUTE,
        result: 0,
    }];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"expected INVALID_ARGS");
    assert_status(
        dispositions[0].result,
        ZX_ERR_INVALID_ARGS,
        name,
        b"disposition result",
    );
    // Nothing may have been delivered.
    let mut got: HandleValue = 0;
    let s = unsafe {
        zx_channel_read(
            ch0,
            0,
            core::ptr::null_mut(),
            &mut got,
            0,
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    if s == ZX_OK {
        fail(name, b"elevated handle was delivered");
    }
    unsafe {
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}

/// NULL disposition pointer with a nonzero count: a user-buffer read
/// failure, must be an error (any) — not a fault or panic.
fn write_etc_null_dispositions() {
    let name = b"write_etc_null_dispositions";
    let (ch0, _ch1) = channel_create();
    let s = unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, core::ptr::null_mut(), 2) };
    if s == ZX_OK {
        fail(name, b"NULL dispositions accepted");
    }
    unsafe {
        zx_handle_close(ch0);
    }
    pass(name);
}

#[no_mangle]
pub fn main() {
    zx::debug_write(
        b"== syscall_fuzz_test ==
",
    );
    write_etc_move_ok();
    write_etc_invalid_rights_bit();
    write_etc_move_twice();
    write_etc_invalid_op();
    write_etc_type_mismatch();
    write_etc_dup_elevated_rights();
    write_etc_null_dispositions();
    write_etc_self_channel();
    write_etc_dup_same_handle_twice();
    get_info_topic_matrix();
    task_wrong_handle_types();
    vmo_vmar_edge_cases();
    property_wrong_handle_types();
    zx::debug_write(
        b"== syscall_fuzz_test PASS ==
",
    );
    zx::Process::exit(0);
}

fn write_etc_self_channel() {
    let name = b"write_etc_self_channel";
    let (ch0, _ch1) = channel_create();
    let mut dispositions = [HandleDisposition {
        op: OP_MOVE,
        handle: ch0,
        type_: 0,
        rights: 0,
        result: 0,
    }];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 1) };
    assert_status(s, ZX_ERR_NOT_SUPPORTED, name, b"expected NOT_SUPPORTED");
    assert_status(
        dispositions[0].result,
        ZX_ERR_NOT_SUPPORTED,
        name,
        b"disposition result",
    );
    unsafe {
        zx_handle_close(ch0);
    }
    pass(name);
}

/// Repeated DUP of the same handle is legal (the BAD_HANDLE rule for
/// repeated handles applies to MOVE only) — both copies must be
/// delivered with the source preserved.
fn write_etc_dup_same_handle_twice() {
    let name = b"write_etc_dup_same_handle_twice";
    let (ch0, ch1) = channel_create();
    let ev = event_create();
    let mut dispositions = [
        HandleDisposition {
            op: OP_DUP,
            handle: ev,
            type_: 0,
            rights: 0,
            result: 0,
        },
        HandleDisposition {
            op: OP_DUP,
            handle: ev,
            type_: 0,
            rights: 0,
            result: 0,
        },
    ];
    let s =
        unsafe { zx_channel_write_etc(ch0, 0, core::ptr::null(), 0, dispositions.as_mut_ptr(), 2) };
    assert_status(s, ZX_OK, name, b"expected OK for repeated DUP");
    // Read both copies back from the peer.
    let mut got: [HandleValue; 2] = [0; 2];
    let s = unsafe {
        zx_channel_read(
            ch1,
            0,
            core::ptr::null_mut(),
            got.as_mut_ptr(),
            0,
            2,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    assert_status(s, ZX_OK, name, b"read back both copies");
    // The source must still be usable (DUP preserves it).
    let mut out: HandleValue = 0;
    let s = unsafe { zx_handle_duplicate(ev, 0, &mut out) };
    assert_status(s, ZX_OK, name, b"DUP source must be preserved");
    unsafe {
        zx_handle_close(out);
        zx_handle_close(got[0]);
        zx_handle_close(got[1]);
        zx_handle_close(ev);
        zx_handle_close(ch1);
        zx_handle_close(ch0);
    }
    pass(name);
}

/// Kernel Topic values (zircon-syscall/src/object.rs numeric_enum).
const TOPIC_NONE: u32 = 0;
const TOPIC_HANDLE_VALID: u32 = 1;
const TOPIC_HANDLE_BASIC: u32 = 2;
const TOPIC_PROCESS: u32 = 3;
const TOPIC_PROCESS_THREADS: u32 = 4;
const TOPIC_VMAR: u32 = 7;
const TOPIC_JOB_CHILDREN: u32 = 8;
const TOPIC_THREAD: u32 = 10;
const TOPIC_SOCKET: u32 = 22;
const TOPIC_VMO: u32 = 23;
const TOPIC_JOB: u32 = 24;

fn get_info_raw(
    handle: HandleValue,
    topic: u32,
    buf: &mut [u8],
    actual: &mut usize,
    avail: &mut usize,
) -> ZxStatus {
    unsafe { zx_object_get_info(handle, topic, buf.as_mut_ptr(), buf.len(), actual, avail) }
}

/// Type-specific topics against every wrong handle type: the kernel
/// must return an error (WRONG_TYPE-class), never OK-with-garbage and
/// never a panic. HandleBasic works on any handle and must succeed —
/// the control row.
fn get_info_topic_matrix() {
    let name = b"get_info_topic_matrix";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut vmo: HandleValue = 0;
    let s = unsafe { zx_vmo_create(4096, 0, &mut vmo) };
    if s != ZX_OK {
        fail(name, b"vmo_create failed");
    }

    let handles: [(HandleValue, &[u8]); 3] = [(ch0, b"channel"), (ev, b"event"), (vmo, b"vmo")];
    let typed_topics: [(u32, &[u8]); 8] = [
        (TOPIC_PROCESS, b"process"),
        (TOPIC_PROCESS_THREADS, b"process-threads"),
        (TOPIC_VMAR, b"vmar"),
        (TOPIC_JOB_CHILDREN, b"job-children"),
        (TOPIC_THREAD, b"thread"),
        (TOPIC_SOCKET, b"socket"),
        (TOPIC_VMO, b"vmo"),
        (TOPIC_JOB, b"job"),
    ];

    let mut buf = [0u8; 64];
    let mut actual: usize = 0;
    let mut avail: usize = 0;

    for (handle, hname) in handles.iter() {
        // Control: HandleBasic succeeds on every handle type.
        let s = get_info_raw(
            *handle,
            TOPIC_HANDLE_BASIC,
            &mut buf,
            &mut actual,
            &mut avail,
        );
        assert_status(s, ZX_OK, name, hname);

        // Every type-specific topic on a wrong handle type: an error,
        // not OK-with-garbage, not a panic, not a hang.
        for (topic, tname) in typed_topics.iter() {
            let s = get_info_raw(*handle, *topic, &mut buf, &mut actual, &mut avail);
            if s == ZX_OK {
                fail(name, b"typed topic accepted wrong handle type");
            }
            let _ = tname;
        }
    }

    // Unknown topics: NOT_SUPPORTED — Fuchsia's topic dispatch
    // default arm returns NOT_SUPPORTED for unknown topics; zCore's
    // earlier INVALID_ARGS was a divergence found by this fuzz test
    // and is fixed alongside it. Includes numbering gaps, the None
    // sentinel, and far-out values.
    let unknown_topics: [u32; 6] = [5, 6, 100, 0x7FFF_FFFF, 0xFFFF_FFFF, TOPIC_NONE];
    for topic in unknown_topics.iter() {
        let s = get_info_raw(ch0, *topic, &mut buf, &mut actual, &mut avail);
        assert_status(s, ZX_ERR_NOT_SUPPORTED, name, b"unknown topic");
    }

    // HandleValid on a valid handle: OK (1-byte record, any buffer).
    let s = get_info_raw(ev, TOPIC_HANDLE_VALID, &mut buf, &mut actual, &mut avail);
    assert_status(s, ZX_OK, name, b"handle-valid");

    // Tiny buffer for a valid query: BUFFER_TOO_SMALL, with the
    // documented actual=0/avail=1 semantics — not a panic.
    let mut tiny = [0u8; 4];
    let s = get_info_raw(ev, TOPIC_HANDLE_BASIC, &mut tiny, &mut actual, &mut avail);
    assert_status(s, ZX_ERR_BUFFER_TOO_SMALL, name, b"tiny buffer");

    // NULL buffer with nonzero size on a record topic.
    let s = unsafe {
        zx_object_get_info(
            ev,
            TOPIC_HANDLE_BASIC,
            core::ptr::null_mut(),
            16,
            &mut actual,
            &mut avail,
        )
    };
    if s == ZX_OK {
        fail(name, b"NULL buffer accepted");
    }

    unsafe {
        zx_handle_close(vmo);
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}

// ── Family 3: task syscalls on wrong-type handles (#913) ──────────

/// zx_task_kill / zx_task_suspend_token / zx_task_create_exception_channel
/// accept only job/process/thread handles — every other type must
/// return WRONG_TYPE, and invalid handle values BAD_HANDLE. zCore's
/// dispatch does a typed lookup first (matching Fuchsia's
/// get_dispatcher_with semantics).
fn task_wrong_handle_types() {
    let name = b"task_wrong_handle_types";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut vmo: HandleValue = 0;
    let s = unsafe { zx_vmo_create(4096, 0, &mut vmo) };
    if s != ZX_OK {
        fail(name, b"vmo_create failed");
    }
    let wrong: [(HandleValue, &[u8]); 3] = [(ch0, b"channel"), (ev, b"event"), (vmo, b"vmo")];

    let mut token: HandleValue = 0;
    let mut out: HandleValue = 0;
    for (handle, _) in wrong.iter() {
        let s = unsafe { zx_task_kill(*handle) };
        assert_status(s, ZX_ERR_WRONG_TYPE, name, b"task_kill");
        let s = unsafe { zx_task_suspend_token(*handle, &mut token) };
        assert_status(s, ZX_ERR_WRONG_TYPE, name, b"task_suspend_token");
        let s = unsafe { zx_task_create_exception_channel(*handle, 0, &mut out) };
        assert_status(s, ZX_ERR_WRONG_TYPE, name, b"exception_channel");
    }

    // Invalid handle values: BAD_HANDLE.
    let bad: HandleValue = 0xDEAD_BEEF;
    let s = unsafe { zx_task_kill(bad) };
    assert_status(s, ZX_ERR_BAD_HANDLE, name, b"kill bad handle");
    let s = unsafe { zx_task_suspend_token(bad, &mut token) };
    assert_status(s, ZX_ERR_BAD_HANDLE, name, b"suspend bad handle");
    let s = unsafe { zx_task_create_exception_channel(bad, 0, &mut out) };
    assert_status(s, ZX_ERR_BAD_HANDLE, name, b"exception_channel bad handle");

    unsafe {
        zx_handle_close(vmo);
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}

// ── Family 4: VMO/VMAR edge cases (#913) ──────────────────────────

/// Adversarial sizes, options, rights and handle types on the VMO and
/// VMAR paths.
fn vmo_vmar_edge_cases() {
    let name = b"vmo_vmar_edge_cases";

    // Invalid creation options (bit 20 is outside
    // RESIZABLE|DISCARDABLE|UNBOUNDED): INVALID_ARGS.
    let mut vmo: HandleValue = 0;
    let s = unsafe { zx_vmo_create(4096, 1 << 20, &mut vmo) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"invalid vmo options");

    // RESIZABLE and UNBOUNDED are mutually exclusive: INVALID_ARGS.
    let s = unsafe { zx_vmo_create(4096, (1 << 1) | (1 << 4), &mut vmo) };
    assert_status(s, ZX_ERR_INVALID_ARGS, name, b"resizable+unbounded");

    // Typed operations on wrong handle types: WRONG_TYPE.
    let ev = event_create();
    let (ch0, _ch1) = channel_create();
    let mut buf = [0u8; 8];
    let s = unsafe { zx_vmo_read(ev, buf.as_mut_ptr(), 0, 8) };
    assert_status(s, ZX_ERR_WRONG_TYPE, name, b"vmo_read on event");
    let mut size: usize = 0;
    let s = unsafe { zx_vmo_get_size(ch0, &mut size) };
    assert_status(s, ZX_ERR_WRONG_TYPE, name, b"vmo_get_size on channel");

    // VMAR map edges via the bootstrap root VMAR.
    let bootstrap = match petal::Bootstrap::read() {
        Some(b) => b,
        None => fail(name, b"no bootstrap"),
    };
    let root_vmar = bootstrap.find(zircon_abi::processargs::PA_VMAR_ROOT);
    if root_vmar == 0 {
        fail(name, b"no root vmar");
    }
    let s = unsafe { zx_vmo_create(4096, 0, &mut vmo) };
    if s != ZX_OK {
        fail(name, b"vmo_create failed");
    }

    // Zero length: INVALID_ARGS.
    let mut addr: usize = 0;
    let s = unsafe {
        zx_vmar_map(
            root_vmar,
            ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
            0,
            vmo,
            0,
            0,
            &mut addr,
        )
    };
    if s == ZX_OK {
        fail(name, b"zero-length map accepted");
    }

    // Wrong VMAR handle type: WRONG_TYPE.
    let s = unsafe { zx_vmar_map(ev, ZX_VM_PERM_READ, 0, vmo, 0, 4096, &mut addr) };
    assert_status(s, ZX_ERR_WRONG_TYPE, name, b"vmar_map on event");

    // VMO without ZX_RIGHT_MAP: ACCESS_DENIED. Strip the right via
    // zx_handle_replace (which also exercises replace semantics).
    let mut no_map: HandleValue = 0;
    let s = unsafe {
        zx_handle_replace(
            vmo,
            ZX_RIGHT_READ | ZX_RIGHT_DUPLICATE | ZX_RIGHT_TRANSFER,
            &mut no_map,
        )
    };
    if s != ZX_OK {
        fail(name, b"handle_replace failed");
    }
    let s = unsafe { zx_vmar_map(root_vmar, ZX_VM_PERM_READ, 0, no_map, 0, 4096, &mut addr) };
    assert_status(s, ZX_ERR_ACCESS_DENIED, name, b"map without MAP right");

    unsafe {
        zx_handle_close(no_map);
        zx_handle_close(vmo);
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}

// ── Family 6: property × handle-type matrix (#913) ────────────────

/// Kernel Property values (zircon-syscall object.rs numeric_enum).
const PROP_NAME: u32 = 3;
const PROP_PROCESS_DEBUG_ADDR: u32 = 5;
const PROP_VMO_CONTENT_SIZE: u32 = 17;

/// get/set_property with cross-type handles and unknown properties.
fn property_wrong_handle_types() {
    let name = b"property_wrong_handle_types";
    let (ch0, _ch1) = channel_create();
    let ev = event_create();
    let mut vmo: HandleValue = 0;
    let s = unsafe { zx_vmo_create(4096, 0, &mut vmo) };
    if s != ZX_OK {
        fail(name, b"vmo_create failed");
    }
    let mut value = [0u8; 8];

    // Rights are checked on the untyped handle BEFORE the property
    // dispatch (Fuchsia: get_dispatcher_with_rights with GET_PROPERTY,
    // then the typed match) — a channel lacks GET_PROPERTY, so
    // cross-type properties on it are ACCESS_DENIED.
    let s = unsafe { zx_object_get_property(ch0, PROP_PROCESS_DEBUG_ADDR, value.as_mut_ptr(), 8) };
    assert_status(s, ZX_ERR_ACCESS_DENIED, name, b"debug_addr on channel");
    let s = unsafe { zx_object_get_property(ev, PROP_NAME, value.as_mut_ptr(), 8) };
    assert_status(
        s,
        ZX_ERR_ACCESS_DENIED,
        name,
        b"name on event (no GET_PROPERTY)",
    );
    let s = unsafe { zx_object_get_property(ch0, PROP_VMO_CONTENT_SIZE, value.as_mut_ptr(), 8) };
    assert_status(s, ZX_ERR_ACCESS_DENIED, name, b"content_size on channel");

    // A VMO carries GET_PROPERTY: a process-scoped property on it
    // passes the rights gate and fails the typed dispatch — WRONG_TYPE.
    let s = unsafe { zx_object_get_property(vmo, PROP_PROCESS_DEBUG_ADDR, value.as_mut_ptr(), 8) };
    assert_status(s, ZX_ERR_WRONG_TYPE, name, b"debug_addr on vmo");

    // set_property with a cross-type handle: SET_PROPERTY rights
    // first — ACCESS_DENIED on a channel.
    let s = unsafe { zx_object_set_property(ch0, PROP_PROCESS_DEBUG_ADDR, value.as_ptr(), 8) };
    assert_status(s, ZX_ERR_ACCESS_DENIED, name, b"set debug_addr on channel");

    // Unknown property numbers on a handle that passes the rights
    // gate (the VMO): NOT_SUPPORTED — Fuchsia's get match default.
    let s = unsafe { zx_object_get_property(vmo, 0, value.as_mut_ptr(), 8) };
    assert_status(s, ZX_ERR_NOT_SUPPORTED, name, b"unknown property 0");
    let s = unsafe { zx_object_get_property(vmo, 100, value.as_mut_ptr(), 8) };
    assert_status(s, ZX_ERR_NOT_SUPPORTED, name, b"unknown property 100");

    unsafe {
        zx_handle_close(vmo);
        zx_handle_close(ev);
        zx_handle_close(ch0);
    }
    pass(name);
}
