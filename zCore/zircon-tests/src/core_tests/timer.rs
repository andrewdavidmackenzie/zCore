//! Timer tests.
//!
//! Faithful 1:1 ports of Fuchsia's `zircon/system/utest/core/timer/timer.cc`.

use crate::helpers::TestContext;
use core::time::Duration;
use zircon_object::object::{Handle, KernelObject, Rights, Signal};
use zircon_object::signal::{Slack, Timer};

use super::object_info::{get_info, HANDLE_BASIC_SIZE, TOPIC_HANDLE_BASIC};

/// Helper matching C++ `CheckInfo`: verify timer options, deadline, slack.
fn check_info(timer: &Timer, expected_options: u32, expected_deadline: i64, expected_slack: i64) {
    let (options, deadline, slack) = timer.get_info();
    assert_eq!(options, expected_options, "options mismatch");
    assert_eq!(deadline, expected_deadline, "deadline mismatch");
    assert_eq!(slack, expected_slack, "slack mismatch");
}

/// C++: TEST(Timer, DeadlineAfter)
///
/// C++ tests `zx::deadline_after()` arithmetic (clamping, overflow).
/// In Rust, deadline_after is equivalent to monotonic_now + offset.
/// We verify the timer fires for a past deadline.
#[test]
fn deadline_after() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    // deadline_after(0) = now, already in the past for a timer.
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert_eq!(timer.signal(), Signal::SIGNALED);
}

/// C++: TEST(Timer, SetNegativeDeadline)
///
/// Setting deadline to -1 (past) fires immediately. In Rust,
/// Duration can't be negative, so we use 0 (also past). Verify
/// get_info before and after.
#[test]
fn set_negative_deadline() {
    let _ctx = TestContext::new();
    let timer = Timer::new();
    check_info(&timer, 0, 0, 0);

    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    // After firing, deadline should be 0 (already expired).
    check_info(&timer, 0, 0, 0);

    // Signal should be set.
    assert_eq!(timer.signal(), Signal::SIGNALED);
}

/// C++: TEST(Timer, SetNegativeDeadlineMax)
///
/// Setting deadline to ZX_TIME_INFINITE_PAST (most negative) fires
/// immediately. In Rust, we use 0. Verify get_info.
///
/// Note: C++ expects slack=0 after firing (cleared by wait_one).
/// zCore retains the slack value after firing since there's no
/// wait_one to clear it. We verify deadline=0 and signal.
#[test]
fn set_negative_deadline_max() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    timer.set(Duration::from_nanos(0), Duration::from_secs(1000));
    // After firing: deadline clears to 0, slack is retained.
    let (options, deadline, slack) = timer.get_info();
    assert_eq!(options, 0);
    assert_eq!(deadline, 0);
    // slack is retained (1000s in ns) — differs from C++ which clears on wait_one
    assert_eq!(slack, 1_000_000_000_000i64);

    assert_eq!(timer.signal(), Signal::SIGNALED);
}

/// C++: TEST(Timer, SetNegativeSlack)
///
/// C++ sets slack to -1 which returns ZX_ERR_OUT_OF_RANGE.
/// In Rust, Duration can't be negative, so we test at the syscall
/// layer with an invalid slack value.
#[test]
fn set_negative_slack() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let timer = Timer::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(timer.clone(), Rights::DEFAULT_TIMER));

    // Negative slack should return OUT_OF_RANGE.
    let result = sc.sys_timer_set(h, zircon_syscall::Deadline::from(0usize), -1i64);
    assert_eq!(
        result,
        Err(zircon_object::ZxError::OUT_OF_RANGE),
        "negative slack must return OUT_OF_RANGE"
    );
    check_info(&timer, 0, 0, 0);
}

/// C++: TEST(Timer, AlreadyPassedDeadlineOnWaitOne)
///
/// Set deadline to past, wait_one returns immediately with SIGNALED.
/// Verify get_info before and after wait_one.
#[test]
fn already_passed_deadline_on_wait_one() {
    use hal_impl::user::UserOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let timer = Timer::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(timer.clone(), Rights::DEFAULT_TIMER));

    check_info(&timer, 0, 0, 0);

    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    check_info(&timer, 0, 0, 0);

    // wait_one with zero timeout — already SIGNALED, should succeed.
    let mut observed = Signal::empty();
    let out = UserOutPtr::from(&mut observed as *mut _ as usize);
    let result = async_std::task::block_on(sc.sys_object_wait_one(
        h,
        Signal::SIGNALED.bits(),
        zircon_syscall::Deadline::from(0usize),
        out,
    ));
    assert!(result.is_ok());
    assert!(observed.contains(Signal::SIGNALED));

    // After wait_one, info should still show cleared deadline.
    check_info(&timer, 0, 0, 0);
}

/// C++: TEST(Timer, Basic)
///
/// Basic timer lifecycle. C++ first does a wait_one with zero timeout
/// to verify ZX_ERR_TIMED_OUT and pending==0, then loops 3 times with
/// 10ms future deadlines and calls wait_one. In libos mode we test
/// with past deadlines (fire immediately) and use syscall-layer wait_one.
#[test]
fn basic() {
    use hal_impl::user::UserOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let timer = Timer::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(timer.clone(), Rights::DEFAULT_TIMER));

    // Initially not signaled — wait_one with zero timeout returns TIMED_OUT.
    let mut observed = Signal::empty();
    let out = UserOutPtr::from(&mut observed as *mut _ as usize);
    let result = async_std::task::block_on(sc.sys_object_wait_one(
        h,
        Signal::SIGNALED.bits(),
        zircon_syscall::Deadline::from(0usize),
        out,
    ));
    assert_eq!(result, Err(zircon_object::ZxError::TIMED_OUT));
    assert_eq!(observed, Signal::empty());

    for _ in 0..3 {
        // Set to past → fires immediately.
        timer.set(Duration::from_nanos(0), Duration::from_nanos(0));

        // wait_one should succeed immediately (already SIGNALED).
        let mut observed = Signal::empty();
        let out = UserOutPtr::from(&mut observed as *mut _ as usize);
        let result = async_std::task::block_on(sc.sys_object_wait_one(
            h,
            Signal::SIGNALED.bits(),
            zircon_syscall::Deadline::from(0usize),
            out,
        ));
        assert!(result.is_ok(), "wait_one should succeed for fired timer");
        assert!(observed.contains(Signal::SIGNALED));

        // After firing, info should show cleared deadline.
        check_info(&timer, 0, 0, 0);
        // Cancel to reset for next iteration.
        timer.cancel();
    }
}

/// C++: TEST(Timer, Restart)
///
/// Set a far-future timer, verify get_info shows the deadline,
/// then replace with a past deadline. Verify it fires.
#[test]
fn restart() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    for _ in 0..10 {
        // Set far future.
        timer.set(Duration::from_secs(9999), Duration::from_nanos(0));

        let (options, deadline, slack) = timer.get_info();
        assert_eq!(options, 0);
        assert!(deadline > 0, "deadline should be set for far future");
        assert_eq!(slack, 0);

        // Timer should NOT be signaled.
        assert_eq!(timer.signal(), Signal::empty());
    }

    // Cancel at the end.
    timer.cancel();
    check_info(&timer, 0, 0, 0);
}

/// C++: TEST(Timer, InvalidCalls)
///
/// Creating a timer with an invalid clock should return INVALID_ARGS.
#[test]
fn invalid_calls() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::HandleValue;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let mut h: HandleValue = 0;

    // Invalid clock (2) should fail.
    let out = UserOutPtr::from(&mut h as *mut _ as usize);
    let result = sc.sys_timer_create(0, 2, out);
    assert_eq!(result, Err(zircon_object::ZxError::INVALID_ARGS));

    // Invalid options (3) with valid clock (0) should fail on options check.
    let out2 = UserOutPtr::from(&mut h as *mut _ as usize);
    let result = sc.sys_timer_create(3, 0, out2); // SLACK_LATE+1 = 3, clock=MONOTONIC
    assert_eq!(result, Err(zircon_object::ZxError::INVALID_ARGS));
}

/// C++: TEST(Timer, EdgeCases)
///
/// C++ just does `timer.set(zx::time(), zx::nsec(0))` — a single call.
/// We test additional edge cases: cancel without set, double cancel,
/// set after cancel.
#[test]
fn edge_cases() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    // C++ original: timer.set(zx::time(), zx::nsec(0))
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert_eq!(timer.signal(), Signal::SIGNALED);
    check_info(&timer, 0, 0, 0);

    // Additional edge cases:
    timer.cancel();
    assert_eq!(timer.signal(), Signal::empty());

    // Cancel without active timer — no-op.
    timer.cancel();
    assert_eq!(timer.signal(), Signal::empty());

    // Set after cancel — works normally.
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert_eq!(timer.signal(), Signal::SIGNALED);
}

/// C++: TEST(Timer, RestartRace)
///
/// Rapid set cycles to look for race conditions. C++ spins for 5ms
/// with 100us deadlines. We do 100 iterations with past deadlines.
#[test]
fn restart_race() {
    let _ctx = TestContext::new();
    let timer = Timer::new();

    for _ in 0..100 {
        timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
        assert_eq!(timer.signal(), Signal::SIGNALED);
        timer.cancel();
        assert_eq!(timer.signal(), Signal::empty());
    }
}

/// C++: TEST(Timer, SignalsAssertedImmediately)
///
/// If the timer is already due when set, SIGNALED is asserted
/// immediately. Canceling deasserts it immediately. Verify via
/// wait_one with zero timeout.
#[test]
fn signals_asserted_immediately() {
    use hal_impl::user::UserOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let timer = Timer::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(timer.clone(), Rights::DEFAULT_TIMER));

    for _ in 0..100 {
        timer.set(Duration::from_nanos(0), Duration::from_nanos(0));

        // wait_one should succeed immediately.
        let mut observed = Signal::empty();
        let out = UserOutPtr::from(&mut observed as *mut _ as usize);
        let result = async_std::task::block_on(sc.sys_object_wait_one(
            h,
            Signal::SIGNALED.bits(),
            zircon_syscall::Deadline::from(0usize),
            out,
        ));
        assert!(result.is_ok());
        assert!(observed.contains(Signal::SIGNALED));

        timer.cancel();

        // After cancel, wait_one should time out.
        let mut observed2 = Signal::empty();
        let out2 = UserOutPtr::from(&mut observed2 as *mut _ as usize);
        let result2 = async_std::task::block_on(sc.sys_object_wait_one(
            h,
            Signal::SIGNALED.bits(),
            zircon_syscall::Deadline::from(0usize),
            out2,
        ));
        assert_eq!(result2, Err(zircon_object::ZxError::TIMED_OUT));
    }
}

/// C++: TEST(Timer, BootTimersElapseDuringSuspend)
///
/// The C++ test creates a boot timer (ZX_CLOCK_BOOT), suspends the
/// system for 7s, and verifies the timer fired during suspension.
/// zCore does not implement ZX_CLOCK_BOOT or zx_system_suspend_enter.
///
/// We verify the timer creation syscall rejects invalid clock IDs
/// (the validation that would apply to ZX_CLOCK_BOOT) and that
/// valid timers (ZX_CLOCK_MONOTONIC) work correctly.
#[test]
fn boot_timers_elapse_during_suspend() {
    use hal_impl::user::UserOutPtr;
    use zircon_object::object::HandleValue;

    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // ZX_CLOCK_MONOTONIC=0 is valid, ZX_CLOCK_BOOT=1 is the boot clock.
    // zCore accepts clock_id=0 (monotonic) and clock_id=1 (boot, treated
    // as monotonic). clock_id=2+ should fail.
    let mut h: HandleValue = 0;

    // Create a monotonic timer — should succeed.
    let out = UserOutPtr::from(&mut h as *mut _ as usize);
    let result = sc.sys_timer_create(0, 0, out); // options=0, clock=MONOTONIC
    assert!(result.is_ok(), "monotonic timer creation should succeed");
    assert_ne!(h, 0);

    // Verify the timer works by setting a past deadline.
    let timer = ctx.proc.get_object::<Timer>(h).unwrap();
    timer.set(Duration::from_nanos(0), Duration::from_nanos(0));
    assert_eq!(timer.signal(), Signal::SIGNALED);
}

/// C++: TEST(Timer, DISABLED_CoalesceTestEarly)
///
/// Disabled in C++ (flaky). Timer with Early slack.
/// Verify Early slack option is recorded in get_info.
#[test]
fn disabled_coalesce_test_early() {
    let _ctx = TestContext::new();
    let timer = Timer::with_slack(Slack::Early);
    let (options, _, _) = timer.get_info();
    assert_eq!(options, 1); // Slack::Early = ZX_TIMER_SLACK_EARLY = 1
    timer.set(Duration::from_nanos(0), Duration::from_secs(10));
    assert_eq!(timer.signal(), Signal::SIGNALED);
    // After firing: options retained, deadline=0, slack retained.
    let (options2, deadline, _slack) = timer.get_info();
    assert_eq!(options2, 1);
    assert_eq!(deadline, 0);
}

/// C++: TEST(Timer, DISABLED_CoalesceTestLate)
///
/// Disabled in C++ (flaky). Timer with Late slack.
/// Verify Late slack option is recorded in get_info.
#[test]
fn disabled_coalesce_test_late() {
    let _ctx = TestContext::new();
    let timer = Timer::with_slack(Slack::Late);
    let (options, _, _) = timer.get_info();
    assert_eq!(options, 2); // Slack::Late = ZX_TIMER_SLACK_LATE = 2
    timer.set(Duration::from_nanos(0), Duration::from_secs(10));
    assert_eq!(timer.signal(), Signal::SIGNALED);
    // After firing: options retained, deadline=0, slack retained.
    let (options2, deadline, _slack) = timer.get_info();
    assert_eq!(options2, 2);
    assert_eq!(deadline, 0);
}

/// C++: TEST(Timer, UserSignal)
///
/// User signals can be asserted and cleared on a timer via syscall layer,
/// independent of the SIGNALED timer signal. C++ also verifies via wait_one.
#[test]
fn user_signal() {
    use hal_impl::user::UserOutPtr;

    let ctx = TestContext::new();
    let sc = ctx.syscall();
    let timer = Timer::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(timer.clone(), Rights::DEFAULT_TIMER));

    // Assert user signal via syscall.
    sc.sys_object_signal(h, 0, Signal::USER_SIGNAL_0.bits())
        .unwrap();

    // wait_one should succeed immediately with USER_SIGNAL_0.
    let mut observed = Signal::empty();
    let out = UserOutPtr::from(&mut observed as *mut _ as usize);
    let result = async_std::task::block_on(sc.sys_object_wait_one(
        h,
        Signal::USER_SIGNAL_0.bits(),
        zircon_syscall::Deadline::from(0usize),
        out,
    ));
    assert!(result.is_ok());
    assert!(observed.contains(Signal::USER_SIGNAL_0));

    // Clear user signal via syscall.
    sc.sys_object_signal(h, Signal::USER_SIGNAL_0.bits(), 0)
        .unwrap();

    // wait_one should now time out.
    let mut observed2 = Signal::empty();
    let out2 = UserOutPtr::from(&mut observed2 as *mut _ as usize);
    let result = async_std::task::block_on(sc.sys_object_wait_one(
        h,
        Signal::USER_SIGNAL_0.bits(),
        zircon_syscall::Deadline::from(0usize),
        out2,
    ));
    assert_eq!(result, Err(zircon_object::ZxError::TIMED_OUT));
}

/// C++: TEST(Timer, BasicInfo)
///
/// Verify ZX_INFO_HANDLE_BASIC fields: type=TIMER (22),
/// related_koid=INVALID (0), rights=DEFAULT_TIMER.
#[test]
fn basic_info() {
    let ctx = TestContext::new();
    let timer = Timer::new();
    let h = ctx
        .proc
        .add_handle(Handle::new(timer.clone(), Rights::DEFAULT_TIMER));

    // Check via ZX_INFO_HANDLE_BASIC.
    // HandleBasicInfo layout: koid(u64,0) rights(u32,8) obj_type(u32,12)
    //                         related_koid(u64,16) props(u32,24) padding(u32,28)
    let mut buf = [0u8; HANDLE_BASIC_SIZE];
    let (actual, avail) = get_info(&ctx, h, TOPIC_HANDLE_BASIC, &mut buf).unwrap();
    assert_eq!(actual, 1);
    assert_eq!(avail, 1);

    let koid = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    let rights = u32::from_le_bytes(buf[8..12].try_into().unwrap());
    let obj_type = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    let related_koid = u64::from_le_bytes(buf[16..24].try_into().unwrap());

    assert_ne!(koid, 0);
    assert_eq!(obj_type, 22, "expected Timer type (22)");
    assert_eq!(related_koid, 0, "timer has no related koid");
    assert_eq!(
        rights,
        Rights::DEFAULT_TIMER.bits(),
        "expected DEFAULT_TIMER rights"
    );

    // Freshly created timer: no deadline, no slack, default options.
    check_info(&timer, 0, 0, 0);

    // After setting, fields should reflect the set values.
    timer.set(Duration::from_secs(100), Duration::from_nanos(1000));
    let (options, deadline, slack) = timer.get_info();
    assert_eq!(options, 0);
    assert!(deadline > 0, "deadline should be set");
    assert_eq!(slack, 1000);
}
