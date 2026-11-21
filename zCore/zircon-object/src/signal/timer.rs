// Timer::inner uses InterruptSpinLock (not KernelMutex) because
// Timer::touch() is called from timer interrupt callbacks via
// timer_tick() → expire() → callback → touch() → inner.lock().
use crate::object::*;
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::time::Duration;
use lock::Mutex;

/// An object that may be signaled at some point in the future
///
/// ## SYNOPSIS
///
/// A timer is used to wait until a specified point in time has occurred
/// or the timer has been canceled.
pub struct Timer {
    base: KObjectBase,
    _counter: CountHelper,
    #[allow(dead_code)]
    slack: Slack,
    inner: Mutex<TimerInner>,
}

impl_kobject!(Timer);
define_count_helper!(Timer);

#[derive(Default)]
struct TimerInner {
    deadline: Option<Duration>,
    slack: Duration,
}

/// Slack specifies how much a timer or event is allowed to deviate from its deadline.
///
/// **Not supported: Now slack has no effect on the timer.**
#[repr(u32)]
#[derive(Debug, Copy, Clone)]
pub enum Slack {
    /// slack is centered around deadline
    Center = 0,
    /// slack interval is (deadline - slack, deadline]
    Early = 1,
    /// slack interval is [deadline, deadline + slack)
    Late = 2,
}

impl Timer {
    /// Create a new `Timer`.
    pub fn new() -> Arc<Self> {
        Self::with_slack(Slack::Center)
    }

    /// Create a new `Timer` with slack.
    pub fn with_slack(slack: Slack) -> Arc<Self> {
        Arc::new(Timer {
            base: KObjectBase::default(),
            _counter: CountHelper::new(),
            slack,
            inner: Mutex::default(),
        })
    }

    /// Create a one-shot timer.
    pub fn one_shot(deadline: Duration) -> Arc<Self> {
        let timer = Timer::new();
        timer.set(deadline, Duration::default());
        timer
    }

    /// Starts a one-shot timer that will fire when `deadline` passes.
    ///
    /// If a previous call to `set` was pending, the previous timer is canceled
    /// and `Signal::SIGNALED` is de-asserted as needed.
    ///
    /// If the deadline is already in the past (or zero), the timer fires
    /// immediately by asserting SIGNALED synchronously.
    pub fn set(self: &Arc<Self>, deadline: Duration, slack: Duration) {
        let fire_now = {
            let mut inner = self.inner.lock();
            inner.slack = slack;
            let now = hal_impl::timer::timer_now();
            if deadline <= now {
                inner.deadline = None;
                true
            } else {
                inner.deadline = Some(deadline);
                false
            }
        };
        // Signal operations are done outside the lock to avoid
        // potential deadlocks with signal handler callbacks.
        if fire_now {
            self.base.signal_set(Signal::SIGNALED);
        } else {
            self.base.signal_clear(Signal::SIGNALED);
            let me = Arc::downgrade(self);
            hal_impl::timer::timer_set(
                deadline,
                Box::new(move |now| me.upgrade().map(|timer| timer.touch(now)).unwrap_or(())),
            );
        }
    }

    /// Cancel the pending timer started by `set`.
    ///
    /// Clears `Signal::SIGNALED` if it was asserted.
    pub fn cancel(&self) {
        {
            let mut inner = self.inner.lock();
            inner.deadline = None;
        }
        self.base.signal_clear(Signal::SIGNALED);
    }

    /// Get timer info for `ZX_INFO_TIMER`.
    ///
    /// Returns `(options, deadline_nanos, slack_nanos)` where options
    /// encodes the slack mode (0=Center, 1=Early, 2=Late).
    pub fn get_info(&self) -> (u32, i64, i64) {
        let inner = self.inner.lock();
        let options = self.slack as u32;
        let deadline = inner
            .deadline
            .map_or(0i64, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX));
        let slack = i64::try_from(inner.slack.as_nanos()).unwrap_or(i64::MAX);
        (options, deadline, slack)
    }

    /// Called by HAL timer.
    fn touch(&self, now: Duration) {
        let mut inner = self.inner.lock();
        if let Some(deadline) = inner.deadline {
            if now >= deadline {
                self.base.signal_set(Signal::SIGNALED);
                inner.deadline = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hal_impl::timer::timer_now;

    // Timer callback delivery depends on a host OS thread getting CPU time
    // within a few ms (hal-impl's libos timer_set spawns a std::thread).
    // On resource-constrained CI runners (macOS: 3 cores) a timer thread
    // can be starved past its deadline when many tests compete for CPU.
    //
    // These tests are `#[serial_test::serial]` (exclusive lock on the
    // default key) while every other test in this crate is marked
    // `#[serial_test::parallel]` (shared lock on the same key). That
    // combination means: all other tests still run in parallel with each
    // other, but while a timer test runs, they are all blocked — the
    // timer thread only competes with the one test thread that spawned it.
    // New tests in this crate must also be marked `#[serial_test::parallel]`
    // (mod-level for sync-only modules, fn-level in modules with
    // `#[async_std::test]` tests, where it must come AFTER the
    // `#[async_std::test]` attribute).

    #[test]
    #[serial_test::serial]
    fn one_shot() {
        let timer = Timer::one_shot(timer_now() + Duration::from_millis(15));
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(timer.signal(), Signal::empty());

        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(timer.signal(), Signal::SIGNALED);
    }

    #[test]
    #[serial_test::serial]
    fn set() {
        let timer = Timer::new();
        timer.set(timer_now() + Duration::from_millis(10), Duration::default());
        timer.set(timer_now() + Duration::from_millis(20), Duration::default());

        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(timer.signal(), Signal::empty());

        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(timer.signal(), Signal::SIGNALED);

        timer.set(timer_now() + Duration::from_millis(10), Duration::default());
        assert_eq!(timer.signal(), Signal::empty());
    }

    #[test]
    #[serial_test::serial]
    fn cancel() {
        let timer = Timer::new();
        timer.set(timer_now() + Duration::from_millis(10), Duration::default());

        std::thread::sleep(Duration::from_millis(5));
        timer.cancel();

        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(timer.signal(), Signal::empty());
    }
}
