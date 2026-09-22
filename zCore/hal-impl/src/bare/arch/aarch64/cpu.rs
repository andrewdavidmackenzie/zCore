//! CPU information.

use cortex_a::registers::*;
use tock_registers::interfaces::Readable;

hal_fn_impl! {
    impl mod crate::hal_fn::cpu {
        fn cpu_id() -> u8 {
            let id = MPIDR_EL1.get() & 0x3;
            id as u8
        }

        fn cpu_frequency() -> u16 {
            0
        }

        fn reset() -> ! {
            info!("shutdown...");
            let psci_system_off = 0x8400_0008_usize;
            unsafe {
                core::arch::asm!(
                    "hvc #0",
                    in("x0") psci_system_off
                );
            }
            unreachable!()
        }
    }
}

/// PSCI function IDs (SMC Calling Convention, 64-bit).
const PSCI_CPU_ON_64: usize = 0xC400_0003;

/// Start a secondary CPU core via PSCI CPU_ON.
///
/// - `target_cpu`: MPIDR affinity value of the core to start
/// - `entry_point`: physical address where the core begins execution
/// - `context_id`: value passed in x0 to the secondary core's entry
///
/// Returns Ok(()) on success, Err with PSCI error code on failure.
pub fn psci_cpu_on(target_cpu: usize, entry_point: usize, context_id: usize) -> Result<(), i64> {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "hvc #0",
            inout("x0") PSCI_CPU_ON_64 as u64 => ret,
            in("x1") target_cpu as u64,
            in("x2") entry_point as u64,
            in("x3") context_id as u64,
        );
    }
    if ret == 0 {
        Ok(())
    } else {
        Err(ret)
    }
}
