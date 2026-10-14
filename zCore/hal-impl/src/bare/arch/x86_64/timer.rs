use core::time::Duration;

use ::drivers::irq::x86::Apic;

pub fn timer_now() -> Duration {
    let cycle = unsafe { core::arch::x86_64::_rdtsc() };
    Duration::from_nanos(cycle * 1000 / super::cpu::cpu_frequency() as u64)
}

pub fn init() {
    // Configure and enable the local APIC timer. The timer registers
    // (mode, divide, initial count) are per-CPU — each core must set
    // them independently. The BSP sets them first in drivers.rs during
    // driver init; APs call this during secondary_init via timer_enable().
    use x2apic::lapic::{TimerDivide, TimerMode};
    let freq = super::cpu::cpu_frequency();
    let cycles = freq as u64 * 1_000_000 / super::super::timer::TICKS_PER_SEC;
    Apic::local_apic().set_timer_mode(TimerMode::Periodic);
    Apic::local_apic().set_timer_divide(TimerDivide::Div1);
    Apic::local_apic().set_timer_initial(cycles as u32);
    Apic::local_apic().enable_timer();
}
