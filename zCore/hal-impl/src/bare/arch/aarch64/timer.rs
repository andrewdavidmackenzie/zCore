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
    let cur_cnt = CNTPCT_EL0.get() * 1_000_000_000;
    let freq = CNTFRQ_EL0.get();
    Duration::from_nanos(cur_cnt / freq)
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
