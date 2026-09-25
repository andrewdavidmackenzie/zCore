//! ARM Generic Timer.
//!
//! QEMU virt: uses the physical timer (CNTP, PPI 14 = IRQ 30).
//! RPi 400: uses the virtual timer (CNTV, PPI 11 = IRQ 27), matching Linux.
//! The physical counter (CNTPCT_EL0) is used for reading wall time on both.

use crate::timer::TICKS_PER_SEC;
use core::time::Duration;
use cortex_a::{asm::barrier, registers::*};
use tock_registers::interfaces::{Readable, Writeable};

pub fn timer_now() -> Duration {
    unsafe { barrier::isb(barrier::SY) }
    let cnt = CNTPCT_EL0.get();
    let freq = CNTFRQ_EL0.get();
    // Divide first to avoid u64 overflow (cnt * 1_000_000_000 overflows
    // after ~341s at 54 MHz). Compute the remainder separately to
    // preserve nanosecond precision.
    let secs = cnt / freq;
    let rem = cnt % freq;
    Duration::new(secs, (rem * 1_000_000_000 / freq) as u32)
}

pub fn set_next_trigger() {
    #[cfg(feature = "board-raspi400")]
    CNTV_TVAL_EL0.set(CNTFRQ_EL0.get() / TICKS_PER_SEC);
    #[cfg(not(feature = "board-raspi400"))]
    CNTP_TVAL_EL0.set(CNTFRQ_EL0.get() / TICKS_PER_SEC);
}

pub fn init() {
    #[cfg(feature = "board-raspi400")]
    {
        CNTV_CTL_EL0.write(CNTV_CTL_EL0::ENABLE::SET);
        set_next_trigger();
        info!("timer: virtual timer enabled, CNTFRQ={}", CNTFRQ_EL0.get());
    }

    #[cfg(not(feature = "board-raspi400"))]
    {
        CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);
        set_next_trigger();
    }
}
