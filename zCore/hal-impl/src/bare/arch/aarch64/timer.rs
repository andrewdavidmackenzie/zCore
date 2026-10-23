//! ARM Generic Timer — one-shot mode using CVAL (absolute comparator).
//!
//! QEMU virt: uses the physical timer (CNTP, PPI 14 = IRQ 30).
//! RPi 400 / Jolla C2: uses the virtual timer (CNTV, PPI 11 = IRQ 27).
//! The physical counter (CNTPCT_EL0) is used for reading wall time on both.
//!
//! Instead of periodic TVAL-based triggering, we write the exact deadline
//! to CVAL (comparator value register). The timer fires when the counter
//! reaches the comparator, matching Fuchsia's one-shot model.

use core::time::Duration;
use cortex_a::{asm::barrier, registers::*};
use tock_registers::interfaces::{Readable, Writeable};

pub fn timer_now() -> Duration {
    unsafe { barrier::isb(barrier::SY) }
    let cnt = CNTPCT_EL0.get();
    let freq = CNTFRQ_EL0.get();
    let secs = cnt / freq;
    let rem = cnt % freq;
    Duration::new(secs, (rem * 1_000_000_000 / freq) as u32)
}

/// Convert a monotonic `Duration` to a counter tick value.
fn duration_to_cntpct(d: Duration) -> u64 {
    let freq = CNTFRQ_EL0.get();
    // d.as_nanos() * freq / 1_000_000_000
    (d.as_nanos() as u128 * freq as u128 / 1_000_000_000) as u64
}

/// Program the ARM generic timer to fire at `deadline` (monotonic).
///
/// Writes the absolute comparator value (CVAL) so the timer fires
/// when the counter reaches the deadline. If the deadline is in the
/// past, the timer fires immediately.
pub fn set_oneshot_timer(deadline: Duration) {
    let cval = duration_to_cntpct(deadline);
    #[cfg(any(feature = "board-raspi400", feature = "board-jollac2"))]
    CNTV_CVAL_EL0.set(cval);
    #[cfg(not(any(feature = "board-raspi400", feature = "board-jollac2")))]
    CNTP_CVAL_EL0.set(cval);
}

pub fn init() {
    let backstop = timer_now() + Duration::from_millis(10);

    #[cfg(feature = "board-jollac2")]
    {
        CNTV_CTL_EL0.write(CNTV_CTL_EL0::ENABLE::SET);
        set_oneshot_timer(backstop);
        info!(
            "timer: virtual timer enabled (one-shot), CNTFRQ={}",
            CNTFRQ_EL0.get()
        );
    }

    #[cfg(feature = "board-raspi400")]
    {
        CNTV_CTL_EL0.write(CNTV_CTL_EL0::ENABLE::SET);
        set_oneshot_timer(backstop);
        info!("timer: virtual timer enabled (one-shot), CNTFRQ={}", CNTFRQ_EL0.get());
    }

    #[cfg(not(any(feature = "board-raspi400", feature = "board-jollac2")))]
    {
        CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);
        set_oneshot_timer(backstop);
    }
}
