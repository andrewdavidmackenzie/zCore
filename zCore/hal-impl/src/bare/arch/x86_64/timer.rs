use core::time::Duration;

use ::drivers::irq::x86::Apic;

/// MSR address for IA32_TSC_DEADLINE.
const IA32_TSC_DEADLINE: u32 = 0x6E0;

/// Whether this CPU supports TSC Deadline mode (cached after first check).
static TSC_DEADLINE_SUPPORTED: spin::Once<bool> = spin::Once::new();

pub(crate) fn has_tsc_deadline() -> bool {
    *TSC_DEADLINE_SUPPORTED.call_once(|| {
        raw_cpuid::CpuId::new()
            .get_feature_info()
            .map_or(false, |f| f.has_tsc_deadline())
    })
}

/// Convert a monotonic `Duration` to a TSC cycle count.
fn duration_to_tsc(d: Duration) -> u64 {
    let freq = super::cpu::cpu_frequency() as u64;
    // d.as_nanos() * freq_mhz / 1000 = TSC cycles
    // Use u128 to avoid overflow.
    (d.as_nanos() as u128 * freq as u128 / 1000) as u64
}

pub fn timer_now() -> Duration {
    let cycle = unsafe { core::arch::x86_64::_rdtsc() };
    Duration::from_nanos(cycle * 1000 / super::cpu::cpu_frequency() as u64)
}

pub fn init() {
    // Configure the local APIC timer. Per-CPU — each core must call
    // this independently (BSP via drivers.rs, APs via timer_enable).
    use x2apic::lapic::{TimerDivide, TimerMode};

    if has_tsc_deadline() {
        // TSC Deadline mode: the APIC fires when RDTSC reaches the
        // value written to IA32_TSC_DEADLINE MSR. No divide or
        // initial count needed.
        Apic::local_apic().set_timer_mode(TimerMode::TscDeadline);
        Apic::local_apic().enable_timer();
        // Arm with a 10ms backstop so we get periodic preemption.
        set_oneshot_timer(timer_now() + Duration::from_millis(10));
    } else {
        // Fallback: APIC one-shot mode with divide-by-1.
        // We'll reprogram the initial count on each set_oneshot_timer.
        Apic::local_apic().set_timer_mode(TimerMode::OneShot);
        Apic::local_apic().set_timer_divide(TimerDivide::Div1);
        Apic::local_apic().enable_timer();
        set_oneshot_timer(timer_now() + Duration::from_millis(10));
    }
}

/// Program the APIC timer to fire at `deadline` (monotonic).
///
/// If `deadline` is in the past, the timer fires immediately (or on
/// the next cycle). This is the platform-specific one-shot primitive
/// that matches Fuchsia's `platform_set_oneshot_timer`.
pub fn set_oneshot_timer(deadline: Duration) {
    if has_tsc_deadline() {
        let tsc_target = duration_to_tsc(deadline);
        unsafe {
            // MFENCE + CPUID serialize prior stores before arming.
            core::arch::x86_64::_mm_mfence();
            core::arch::x86_64::__cpuid(0);
            x86_64::registers::model_specific::Msr::new(IA32_TSC_DEADLINE).write(tsc_target);
        }
    } else {
        // OneShot mode: compute remaining cycles from now.
        let now = timer_now();
        let remaining = if deadline > now {
            duration_to_tsc(deadline) - duration_to_tsc(now)
        } else {
            1 // Fire immediately
        };
        // Clamp to u32::MAX (APIC initial count is 32-bit).
        let count = remaining.min(u32::MAX as u64) as u32;
        Apic::local_apic().set_timer_initial(count);
    }
}
