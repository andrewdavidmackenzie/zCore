use core::time::Duration;

fn get_cycle() -> u64 {
    riscv::register::time::read() as u64
}

/// Convert a monotonic `Duration` to a timer cycle count.
fn duration_to_cycles(d: Duration) -> u64 {
    let freq = super::cpu::cpu_frequency() as u64;
    (d.as_nanos() as u128 * freq as u128 / 1000) as u64
}

/// Program the RISC-V timer to fire at `deadline` (monotonic).
///
/// Uses SBI set_timer which programs the hardware comparator
/// to fire when the time CSR reaches the target value.
pub fn set_oneshot_timer(deadline: Duration) {
    let target = duration_to_cycles(deadline);
    sbi_rt::set_timer(target);
}

pub(super) fn init() {
    // Arm with a 10ms backstop.
    set_oneshot_timer(timer_now() + Duration::from_millis(10));
}

pub(crate) fn timer_now() -> Duration {
    Duration::from_nanos(get_cycle() * 1000 / super::cpu::cpu_frequency() as u64)
}
