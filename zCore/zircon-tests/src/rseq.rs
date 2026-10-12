//! Restartable sequences (rseq) tests.
//!
//! Ports of Fuchsia's rseq test suite. Restartable sequences require
//! kernel support for preemption-safe per-CPU data access.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(RseqTest, Abort)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn abort() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, ArithmeticOverflow)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn arithmetic_overflow() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, BadHandle)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn bad_handle() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, DoubleRegister)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn double_register() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, InsufficientRights)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn insufficient_rights() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, InvalidOffset)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn invalid_offset() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, InvalidRseq)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn invalid_rseq() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, InvalidSize)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn invalid_size() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, InvalidUnregisterArgs)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn invalid_unregister_args() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, ManyConcurrent)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn many_concurrent() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, NoLostUpdates)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn no_lost_updates() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, RegisterUnregister)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn register_unregister() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, SetStreamSize)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn set_stream_size() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, UnregisterNotRegistered)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn unregister_not_registered() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, UpdateCpuId)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn update_cpu_id() {
    todo!("requires restartable sequences infrastructure");
}

/// C++: TEST(RseqTest, WrongHandleType)
#[test]
#[ignore = "requires restartable sequences infrastructure"]
fn wrong_handle_type() {
    todo!("requires restartable sequences infrastructure");
}
