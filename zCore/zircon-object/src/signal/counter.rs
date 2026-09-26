//! Zircon Counter kernel object.
//!
//! A Counter wraps a signed 64-bit integer with atomic operations
//! and signal integration. It asserts `ZX_COUNTER_POSITIVE` when
//! the value is positive and `ZX_COUNTER_NON_POSITIVE` otherwise.

use crate::object::*;
use alloc::sync::Arc;
use lock::Mutex;

/// Signal asserted when the counter value is > 0.
const COUNTER_POSITIVE: Signal = Signal::USER_SIGNAL_0;
/// Signal asserted when the counter value is <= 0.
const COUNTER_NON_POSITIVE: Signal = Signal::USER_SIGNAL_1;

/// A Counter kernel object wrapping a signed 64-bit integer.
///
/// The value and signals are updated atomically under a lock
/// to prevent signal/value disagreement under concurrent access.
pub struct Counter {
    base: KObjectBase,
    _counter: CountHelper,
    value: Mutex<i64>,
}

impl_kobject!(Counter
    fn allowed_signals(&self) -> Signal {
        COUNTER_POSITIVE | COUNTER_NON_POSITIVE
    }
);
define_count_helper!(Counter);

impl Counter {
    /// Create a new Counter with initial value 0.
    pub fn new() -> Arc<Self> {
        Arc::new(Counter {
            base: KObjectBase::with_signal(COUNTER_NON_POSITIVE),
            _counter: CountHelper::new(),
            value: Mutex::new(0),
        })
    }

    /// Read the current value.
    pub fn read(&self) -> i64 {
        *self.value.lock()
    }

    /// Write a new value and update signals.
    pub fn write(&self, value: i64) {
        let mut guard = self.value.lock();
        *guard = value;
        self.update_signals(value);
    }

    /// Atomically add `delta` to the value and update signals.
    ///
    /// Uses wrapping addition to avoid overflow panics from
    /// user-supplied delta values.
    ///
    /// Returns the value after the addition.
    pub fn add(&self, delta: i64) -> i64 {
        let mut guard = self.value.lock();
        let new = (*guard).wrapping_add(delta);
        *guard = new;
        self.update_signals(new);
        new
    }

    fn update_signals(&self, value: i64) {
        if value > 0 {
            self.base
                .signal_change(COUNTER_NON_POSITIVE, COUNTER_POSITIVE);
        } else {
            self.base
                .signal_change(COUNTER_POSITIVE, COUNTER_NON_POSITIVE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_read() {
        let counter = Counter::new();
        assert_eq!(counter.read(), 0);
        assert!(counter.signal().contains(COUNTER_NON_POSITIVE));
        assert!(!counter.signal().contains(COUNTER_POSITIVE));
    }

    #[test]
    fn write_updates_signals() {
        let counter = Counter::new();

        counter.write(5);
        assert_eq!(counter.read(), 5);
        assert!(counter.signal().contains(COUNTER_POSITIVE));
        assert!(!counter.signal().contains(COUNTER_NON_POSITIVE));

        counter.write(0);
        assert_eq!(counter.read(), 0);
        assert!(!counter.signal().contains(COUNTER_POSITIVE));
        assert!(counter.signal().contains(COUNTER_NON_POSITIVE));

        counter.write(-3);
        assert_eq!(counter.read(), -3);
        assert!(!counter.signal().contains(COUNTER_POSITIVE));
        assert!(counter.signal().contains(COUNTER_NON_POSITIVE));
    }

    #[test]
    fn add_atomically() {
        let counter = Counter::new();

        let result = counter.add(10);
        assert_eq!(result, 10);
        assert_eq!(counter.read(), 10);
        assert!(counter.signal().contains(COUNTER_POSITIVE));

        let result = counter.add(-15);
        assert_eq!(result, -5);
        assert_eq!(counter.read(), -5);
        assert!(counter.signal().contains(COUNTER_NON_POSITIVE));

        let result = counter.add(5);
        assert_eq!(result, 0);
        assert_eq!(counter.read(), 0);
        assert!(counter.signal().contains(COUNTER_NON_POSITIVE));
    }
}
