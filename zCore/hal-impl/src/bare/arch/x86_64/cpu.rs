//! CPU information.

use core::sync::atomic::{AtomicUsize, Ordering};
use raw_cpuid::CpuId;

/// Number of online CPUs, set during SMP initialization from ACPI
/// discovery. Starts at 1 (BSP only) and is updated by
/// `set_num_cpus` after APs are enumerated.
static NUM_CPUS: AtomicUsize = AtomicUsize::new(1);

/// Set the online CPU count. Called from SMP init after ACPI
/// enumerates application processors. Capped at MAX_CORE_NUM
/// from the build-time config.
pub fn set_num_cpus(count: usize) {
    NUM_CPUS.store(count.min(crate::config::MAX_CORE_NUM), Ordering::Relaxed);
}

hal_fn_impl! {
    impl mod crate::hal_fn::cpu {
        fn cpu_id() -> u8 {
            CpuId::new()
                .get_feature_info()
                .unwrap()
                .initial_local_apic_id()
        }

        fn cpu_index() -> usize {
            let apic_id = CpuId::new()
                .get_feature_info()
                .unwrap()
                .initial_local_apic_id();
            super::smp::apic_id_to_logical(apic_id) as usize
        }

        fn cpu_frequency() -> u16 {
            static CPU_FREQ_MHZ: spin::Once<u16> = spin::Once::new();
            *CPU_FREQ_MHZ.call_once(|| {
                const DEFAULT: u16 = 4000;
                CpuId::new()
                    .get_processor_frequency_info()
                    .map(|info| info.processor_base_frequency())
                    .unwrap_or(DEFAULT)
                    .max(DEFAULT)
            })
        }

        fn num_cpus() -> usize {
            NUM_CPUS.load(Ordering::Relaxed)
        }

        fn reset() -> ! {
            info!("shutdown...");
            loop {
                use ::drivers::io::{Io, Pmio};
                Pmio::<u16>::new(0x604).write(0x2000);
                super::interrupt::wait_for_interrupt();
            }
        }
    }
}
