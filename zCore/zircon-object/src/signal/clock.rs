//! Zircon clock object.
//!
//! A clock maintains a synthetic timeline that can be read and adjusted.
//! The clock's time is computed from the monotonic clock via a linear
//! transformation: `clock_time = (mono - reference_mono) * rate + offset`.

use crate::object::*;
use kernel_hal::timer::timer_now;
use lock::Mutex;

/// Clock creation options.
pub const ZX_CLOCK_OPT_MONOTONIC: u64 = 1 << 0;
pub const ZX_CLOCK_OPT_CONTINUOUS: u64 = 1 << 1;
pub const ZX_CLOCK_OPT_AUTO_START: u64 = 1 << 2;

/// A Zircon clock kernel object.
pub struct Clock {
    base: KObjectBase,
    inner: Mutex<ClockInner>,
    /// Options set at creation time.
    options: u64,
}

impl_kobject!(Clock);

struct ClockInner {
    /// Whether the clock has been started (produces valid readings).
    started: bool,
    /// The monotonic time at the last update.
    reference_mono: i64,
    /// The clock value at `reference_mono`.
    synthetic_offset: i64,
    /// Rate adjustment: synthetic ticks per monotonic tick, as a ratio.
    /// Default 1/1 (no adjustment). Stored as (numerator, denominator).
    rate_num: u32,
    rate_den: u32,
    /// Error bound in nanoseconds (0 = unknown).
    error_bound: u64,
    /// Generation counter (incremented on each update).
    generation: u32,
}

impl Default for ClockInner {
    fn default() -> Self {
        ClockInner {
            started: false,
            reference_mono: 0,
            synthetic_offset: 0,
            rate_num: 1,
            rate_den: 1,
            error_bound: 0,
            generation: 0,
        }
    }
}

impl Clock {
    /// Create a new clock with the given options.
    pub fn new(options: u64) -> ZxResult<Self> {
        let auto_start = options & ZX_CLOCK_OPT_AUTO_START != 0;
        let mut inner = ClockInner::default();
        if auto_start {
            inner.started = true;
            inner.reference_mono = timer_now().as_nanos() as i64;
        }
        Ok(Clock {
            base: KObjectBase::new(),
            inner: Mutex::new(inner),
            options,
        })
    }

    /// Read the current clock value.
    ///
    /// Returns the synthetic time based on the clock's transformation.
    pub fn read(&self) -> ZxResult<i64> {
        let inner = self.inner.lock();
        if !inner.started {
            return Err(ZxError::BAD_STATE);
        }
        let mono_now = timer_now().as_nanos() as i64;
        let elapsed = mono_now - inner.reference_mono;
        let adjusted = if inner.rate_num == inner.rate_den {
            elapsed
        } else {
            elapsed * inner.rate_num as i64 / inner.rate_den as i64
        };
        Ok(inner.synthetic_offset + adjusted)
    }

    /// Update the clock's transformation.
    ///
    /// `args` layout (from Fuchsia `zx_clock_update_args_v2_t`):
    /// - offset 0: rate_adjust (i32) -- ppm adjustment (ignored if not in options)
    /// - offset 4: padding
    /// - offset 8: synthetic_value (i64) -- new clock value
    /// - offset 16: reference_value (i64) -- monotonic reference point
    /// - offset 24: error_bound (u64) -- error bound in ns
    pub fn update(&self, options: u64, args: &[u8]) -> ZxResult {
        let mut inner = self.inner.lock();

        // Option bits indicate which fields are valid.
        const OPT_VALUE: u64 = 1 << 0;
        const OPT_RATE: u64 = 1 << 1;
        const OPT_ERROR: u64 = 1 << 2;
        const OPT_REFERENCE: u64 = 1 << 3;

        if args.len() < 32 {
            return Err(ZxError::INVALID_ARGS);
        }

        if options & OPT_VALUE != 0 {
            let value = i64::from_le_bytes(args[8..16].try_into().unwrap());
            inner.synthetic_offset = value;
        }

        if options & OPT_REFERENCE != 0 {
            let reference = i64::from_le_bytes(args[16..24].try_into().unwrap());
            inner.reference_mono = reference;
        } else if options & OPT_VALUE != 0 {
            // If setting value without reference, use current monotonic
            inner.reference_mono = timer_now().as_nanos() as i64;
        }

        if options & OPT_RATE != 0 {
            let rate_ppm = i32::from_le_bytes(args[0..4].try_into().unwrap());
            // rate_adjust is in PPM: actual_rate = 1 + rate_ppm / 1_000_000
            // Store as ratio: num = 1_000_000 + rate_ppm, den = 1_000_000
            inner.rate_num = (1_000_000i64 + rate_ppm as i64) as u32;
            inner.rate_den = 1_000_000;
        }

        if options & OPT_ERROR != 0 {
            let error = u64::from_le_bytes(args[24..32].try_into().unwrap());
            inner.error_bound = error;
        }

        // Start the clock on first update if not auto-started.
        if !inner.started {
            inner.started = true;
            if options & OPT_REFERENCE == 0 && options & OPT_VALUE == 0 {
                inner.reference_mono = timer_now().as_nanos() as i64;
            }
        }

        inner.generation = inner.generation.wrapping_add(1);

        // Signal that the clock was updated.
        self.base.signal_set(Signal::CLOCK_STARTED);

        Ok(())
    }

    /// Get detailed clock information.
    ///
    /// Returns a `zx_clock_details_v1_t` struct (64 bytes).
    pub fn get_details(&self) -> ZxResult<[u8; 64]> {
        let inner = self.inner.lock();
        let mono_now = timer_now().as_nanos() as i64;

        let mut details = [0u8; 64];
        // options (u64) at offset 0
        details[0..8].copy_from_slice(&self.options.to_le_bytes());
        // mono_to_synthetic numerator (u32) at offset 8
        details[8..12].copy_from_slice(&inner.rate_num.to_le_bytes());
        // mono_to_synthetic denominator (u32) at offset 12
        details[12..16].copy_from_slice(&inner.rate_den.to_le_bytes());
        // mono_to_synthetic reference offset (i64) at offset 16
        details[16..24].copy_from_slice(&inner.reference_mono.to_le_bytes());
        // mono_to_synthetic synthetic offset (i64) at offset 24
        details[24..32].copy_from_slice(&inner.synthetic_offset.to_le_bytes());
        // error_bound (u64) at offset 32
        details[32..40].copy_from_slice(&inner.error_bound.to_le_bytes());
        // query_ticks (i64) at offset 40
        details[40..48].copy_from_slice(&mono_now.to_le_bytes());
        // last_value_update_ticks (i64) at offset 48
        details[48..56].copy_from_slice(&inner.reference_mono.to_le_bytes());
        // last_rate_adjust_update_ticks (i64) at offset 56
        details[56..64].copy_from_slice(&inner.reference_mono.to_le_bytes());

        Ok(details)
    }
}
