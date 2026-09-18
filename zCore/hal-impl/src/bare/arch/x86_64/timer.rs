use core::time::Duration;

use kernel_drivers::irq::x86::Apic;

pub fn timer_now() -> Duration {
    let cycle = unsafe { core::arch::x86_64::_rdtsc() };
    Duration::from_nanos(cycle * 1000 / super::cpu::cpu_frequency() as u64)
}

pub fn init() {
    // Enable the local APIC timer directly. This was previously called
    // through the generic IrqScheme trait, but enabling the APIC timer
    // is inherently x86-specific and doesn't belong in a generic trait.
    Apic::local_apic().enable_timer();
}
