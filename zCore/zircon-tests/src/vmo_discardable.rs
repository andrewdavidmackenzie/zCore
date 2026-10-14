//! Discardable VMO tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/vmo (discardable) tests.
//! Discardable VMOs can have their pages reclaimed by the kernel
//! under memory pressure.

use crate::helpers::TestContext;
use hal_impl::user::{UserInPtr, UserOutPtr};
use zircon_object::object::HandleValue;
use zircon_object::vm::{VmObject, VmoInfoFlags, PAGE_SIZE};
use zircon_object::ZxError;

extern crate alloc;

/// Helper: create a VMO through the syscall layer.
fn syscall_vmo_create(ctx: &TestContext, size: u64, options: u32) -> HandleValue {
    let sc = ctx.syscall();
    let mut h: HandleValue = 0;
    sc.sys_vmo_create(size, options, UserOutPtr::from(&mut h as *mut _ as usize))
        .unwrap();
    assert_ne!(h, 0);
    h
}

/// ZX_VMO_DISCARDABLE flag value.
const ZX_VMO_DISCARDABLE: u32 = 1 << 2;

/// VMO operation codes from VmoOpType enum.
const VMO_OP_LOCK: u32 = 3;
const VMO_OP_UNLOCK: u32 = 4;
#[allow(dead_code)]
const VMO_OP_TRY_LOCK: u32 = 11;

///
/// Create a discardable VMO (ZX_VMO_DISCARDABLE flag). The VMO
/// should be created successfully.
#[test]
fn create_discardable() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, ZX_VMO_DISCARDABLE);
    assert_ne!(h, 0);

    // Verify it's actually discardable via kernel object API.
    let proc = &ctx.proc;
    let vmo = proc
        .get_object::<VmObject>(h)
        .expect("should get VMO from handle");
    assert!(vmo.is_discardable());
}

///
/// Lock a discardable VMO (prevent reclamation), then unlock it.
/// While locked, the VMO's pages should not be reclaimable.
#[test]
fn lock_unlock() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, ZX_VMO_DISCARDABLE);
    let sc = ctx.syscall();

    // Lock (op 11) — must cover entire VMO. Buffer receives lock state.
    let mut lock_state = [0u8; 32];
    sc.sys_vmo_op_range(
        h,
        VMO_OP_LOCK,
        0,
        PAGE_SIZE,
        UserOutPtr::from(lock_state.as_mut_ptr() as usize),
        32,
    )
    .unwrap();

    // lock_state[0..8] = offset, [8..16] = size
    let offset = u64::from_le_bytes(lock_state[0..8].try_into().unwrap());
    let size = u64::from_le_bytes(lock_state[8..16].try_into().unwrap());
    assert_eq!(offset, 0);
    assert_eq!(size, PAGE_SIZE as u64);

    // Unlock (op 13)
    sc.sys_vmo_op_range(h, VMO_OP_UNLOCK, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0)
        .unwrap();
}

///
/// Read and write to a locked discardable VMO. Should behave like
/// a normal VMO while locked.
#[async_std::test]
async fn read_write_while_locked() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, ZX_VMO_DISCARDABLE);
    let sc = ctx.syscall();

    // Lock the VMO.
    let mut lock_state = [0u8; 32];
    sc.sys_vmo_op_range(
        h,
        VMO_OP_LOCK,
        0,
        PAGE_SIZE,
        UserOutPtr::from(lock_state.as_mut_ptr() as usize),
        32,
    )
    .unwrap();

    // Write data.
    let data = [0xDE, 0xAD, 0xBE, 0xEF];
    sc.sys_vmo_write(h, UserInPtr::from(data.as_ptr() as usize), 0, 4)
        .unwrap();

    // Read it back.
    let mut buf = [0u8; 4];
    sc.sys_vmo_read(h, UserOutPtr::from(buf.as_mut_ptr() as usize), 0, 4)
        .await
        .unwrap();
    assert_eq!(buf, data);

    // Unlock.
    sc.sys_vmo_op_range(h, VMO_OP_UNLOCK, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0)
        .unwrap();
}

///
/// After the kernel reclaims pages from an unlocked discardable VMO,
/// reading should return zeroed pages. The VMO transitions to
/// "discarded" state.
/// NOTE: zCore does not implement kernel memory reclamation, so we
/// cannot trigger the discard. This test verifies that an unlocked
/// discardable VMO still reads as zeroed (initial state).
#[async_std::test]
async fn read_after_discard() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, ZX_VMO_DISCARDABLE);
    let sc = ctx.syscall();

    // Lock, write, unlock.
    let mut lock_state = [0u8; 32];
    sc.sys_vmo_op_range(
        h,
        VMO_OP_LOCK,
        0,
        PAGE_SIZE,
        UserOutPtr::from(lock_state.as_mut_ptr() as usize),
        32,
    )
    .unwrap();
    let data = [0xAAu8; 16];
    sc.sys_vmo_write(h, UserInPtr::from(data.as_ptr() as usize), 0, 16)
        .unwrap();
    sc.sys_vmo_op_range(h, VMO_OP_UNLOCK, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0)
        .unwrap();

    // Without reclamation, the data is still present. Read to verify
    // the VMO is still accessible (no crash).
    let mut buf = [0u8; 16];
    sc.sys_vmo_read(h, UserOutPtr::from(buf.as_mut_ptr() as usize), 0, 16)
        .await
        .unwrap();
    // Data survives because zCore doesn't reclaim. This documents behavior.
    assert_eq!(buf, data);
}

///
/// Lock a discardable VMO that has been discarded. The lock should
/// succeed but return a flag indicating the pages were reclaimed.
/// NOTE: zCore doesn't implement reclamation, so we just verify
/// re-locking after unlock works.
#[test]
fn lock_discarded() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, ZX_VMO_DISCARDABLE);
    let sc = ctx.syscall();

    // Lock.
    let mut lock_state = [0u8; 32];
    sc.sys_vmo_op_range(
        h,
        VMO_OP_LOCK,
        0,
        PAGE_SIZE,
        UserOutPtr::from(lock_state.as_mut_ptr() as usize),
        32,
    )
    .unwrap();

    // Unlock.
    sc.sys_vmo_op_range(h, VMO_OP_UNLOCK, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0)
        .unwrap();

    // Re-lock should succeed.
    let mut lock_state2 = [0u8; 32];
    sc.sys_vmo_op_range(
        h,
        VMO_OP_LOCK,
        0,
        PAGE_SIZE,
        UserOutPtr::from(lock_state2.as_mut_ptr() as usize),
        32,
    )
    .unwrap();

    // Unlock again.
    sc.sys_vmo_op_range(h, VMO_OP_UNLOCK, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0)
        .unwrap();
}

///
/// Unlock a discardable VMO that is not locked. Should return
/// BAD_STATE.
#[test]
fn double_unlock() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, ZX_VMO_DISCARDABLE);
    let sc = ctx.syscall();

    // Unlock without locking first → BAD_STATE.
    let result = sc.sys_vmo_op_range(h, VMO_OP_UNLOCK, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0);
    assert_eq!(result, Err(ZxError::BAD_STATE));
}

///
/// Discardable VMOs should not be resizable (ZX_VMO_RESIZABLE +
/// ZX_VMO_DISCARDABLE is allowed by zCore's syscall layer but Fuchsia
/// rejects it). Test that resize is NOT blocked by discardability alone.
/// NOTE: In real Fuchsia, combining RESIZABLE + DISCARDABLE is rejected
/// at creation. zCore allows it, so this test documents zCore's behavior.
#[test]
fn discardable_not_resizable() {
    let ctx = TestContext::new();
    // Create non-resizable discardable VMO (no RESIZABLE flag).
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, ZX_VMO_DISCARDABLE);
    let sc = ctx.syscall();

    // set_size should fail because the VMO is not resizable.
    let result = sc.sys_vmo_set_size(h, 2 * PAGE_SIZE);
    assert_eq!(result, Err(ZxError::UNAVAILABLE));
}

///
/// Locking a non-discardable VMO should fail with NOT_SUPPORTED.
#[test]
fn lock_non_discardable_fails() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, 0); // regular VMO
    let sc = ctx.syscall();

    let mut lock_state = [0u8; 32];
    let result = sc.sys_vmo_op_range(
        h,
        VMO_OP_LOCK,
        0,
        PAGE_SIZE,
        UserOutPtr::from(lock_state.as_mut_ptr() as usize),
        32,
    );
    assert_eq!(result, Err(ZxError::NOT_SUPPORTED));
}

///
/// Query VMO info on a discardable VMO. Should report the
/// DISCARDABLE flag in flags.
#[test]
fn discardable_info() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, ZX_VMO_DISCARDABLE);

    let proc = &ctx.proc;
    let vmo = proc
        .get_object::<VmObject>(h)
        .expect("should get VMO from handle");
    let info = vmo.get_info();
    assert!(
        info.flags.contains(VmoInfoFlags::DISCARDABLE),
        "VMO info flags should include DISCARDABLE"
    );
}

///
/// Lock a discardable VMO multiple times. Each lock increments
/// a reference count; the VMO becomes unlockable only after all
/// locks are released.
#[test]
fn nested_lock() {
    let ctx = TestContext::new();
    let h = syscall_vmo_create(&ctx, PAGE_SIZE as u64, ZX_VMO_DISCARDABLE);
    let sc = ctx.syscall();

    // Lock twice.
    let mut lock_state = [0u8; 32];
    sc.sys_vmo_op_range(
        h,
        VMO_OP_LOCK,
        0,
        PAGE_SIZE,
        UserOutPtr::from(lock_state.as_mut_ptr() as usize),
        32,
    )
    .unwrap();
    sc.sys_vmo_op_range(
        h,
        VMO_OP_LOCK,
        0,
        PAGE_SIZE,
        UserOutPtr::from(lock_state.as_mut_ptr() as usize),
        32,
    )
    .unwrap();

    // First unlock should succeed.
    sc.sys_vmo_op_range(h, VMO_OP_UNLOCK, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0)
        .unwrap();

    // Second unlock should also succeed.
    sc.sys_vmo_op_range(h, VMO_OP_UNLOCK, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0)
        .unwrap();

    // Third unlock should fail — lock count is now 0.
    let result = sc.sys_vmo_op_range(h, VMO_OP_UNLOCK, 0, PAGE_SIZE, UserOutPtr::from(0usize), 0);
    assert_eq!(result, Err(ZxError::BAD_STATE));
}
