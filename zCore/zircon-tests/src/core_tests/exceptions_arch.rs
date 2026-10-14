//! Architecture-specific exception tests.
//!
//! Ports of Fuchsia's architecture-specific exception test suite.
//! Tests ARM64 exception types: data abort, instruction abort, etc.
//! zCore has no hardware exception injection. These tests verify
//! exception channel creation on different task types.

use crate::helpers::TestContext;
use zircon_object::object::{Handle, Rights};
use zircon_object::task::{Job, Thread};

/// C++: TEST(ExceptionsTest, DataAbort)
/// Verify exception channel can be created on a process.
#[test]
fn data_abort() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    let proc_h = ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights));

    let mut channel_h: u32 = 0;
    let out = hal_impl::user::UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(proc_h, 0, out).unwrap();
    assert_ne!(channel_h, 0);
}

/// C++: TEST(ExceptionsTest, InstructionAbort)
/// Verify exception channel can be created on a job.
#[test]
fn instruction_abort() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let job = Job::root();
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    let job_h = ctx.proc.add_handle(Handle::new(job, rights));

    let mut channel_h: u32 = 0;
    let out = hal_impl::user::UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(job_h, 0, out).unwrap();
    assert_ne!(channel_h, 0);
}

/// C++: TEST(ExceptionsTest, PCAlignmentFault)
/// Verify exception channel can be created on a thread.
#[test]
fn pc_alignment_fault() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let thread = Thread::create(&ctx.proc, "align-thread").unwrap();
    let rights = Rights::INSPECT | Rights::DUPLICATE | Rights::TRANSFER | Rights::MANAGE_THREAD;
    let thread_h = ctx.proc.add_handle(Handle::new(thread, rights));

    let mut channel_h: u32 = 0;
    let out = hal_impl::user::UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(thread_h, 0, out).unwrap();
    assert_ne!(channel_h, 0);
}

/// C++: TEST(ExceptionsTest, SPMisalignment)
/// Verify debugger exception channel creation on a process.
#[test]
fn sp_misalignment() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let rights = Rights::INSPECT
        | Rights::DUPLICATE
        | Rights::TRANSFER
        | Rights::MANAGE_THREAD
        | Rights::ENUMERATE;
    let proc_h = ctx.proc.add_handle(Handle::new(ctx.proc.clone(), rights));

    let mut channel_h: u32 = 0;
    let out = hal_impl::user::UserOutPtr::from(&mut channel_h as *mut _ as usize);
    sc.sys_create_exception_channel(proc_h, 1, out).unwrap(); // 1 = debugger
    assert_ne!(channel_h, 0);
}
