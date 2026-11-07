//! Bootstrap and initialization.

use crate::{KernelConfig, KernelHandler, KCONFIG, KHANDLER};

hal_fn_impl! {
    impl mod crate::hal_fn::boot {
        fn cmdline() -> alloc::string::String {
            super::arch::cmdline()
        }

        fn init_ram_disk() -> Option<&'static mut [u8]> {
            super::arch::init_ram_disk()
        }

        fn primary_init_early(cfg: KernelConfig, handler: &'static impl KernelHandler) {
            info!("Primary CPU {} init early...", crate::cpu::cpu_id());
            KCONFIG.init_once_by(cfg);
            KHANDLER.init_once_by(handler);
            super::arch::primary_init_early();
        }

        fn primary_init() {
            // Save the kernel page table before trapframe::init may modify it.
            #[cfg(target_arch = "x86_64")]
            super::arch::vm::save_kernel_page_table();

            info!("Primary CPU {} init: calling trapframe::init()...", crate::cpu::cpu_id());
            unsafe { trapframe::init() };
            info!("trapframe::init() completed");
            // Verify GDT selectors were initialized
            #[cfg(target_arch = "x86_64")]
            {
                extern "C" {
                    static USER_SS: u16;
                    static USER_CS: u16;
                }
                let (ss, cs) = unsafe { (USER_SS, USER_CS) };
                info!("trapframe::init() done: USER_SS={:#x}, USER_CS={:#x}", ss, cs);
                assert!(ss != 0, "USER_SS not initialized!");
                assert!(cs != 0, "USER_CS not initialized!");
                // Save the BSP's GDT layout so APs can clone it with
                // the same structure (same entry count, same selectors)
                // but their own per-CPU TSS.
                super::arch::smp::save_bsp_gdt_template();
            }
            super::arch::primary_init();
        }

        fn secondary_init() {
            // info!("Secondary CPU {} init...", crate::cpu::cpu_id());
            // we can't print anything here, see reason: zcore/main.rs::secondary_main()
            #[cfg(target_arch = "x86_64")]
            {
                // Build a GDT with the SAME layout as the BSP's, but with
                // this AP's own TSS. This avoids the cascading GDT growth
                // from calling trapframe::init() on each AP, which caused
                // each CPU to have differently-sized GDTs with different
                // selector offsets. With a uniform layout, USER_SS/USER_CS
                // and STAR MSR are consistent across all CPUs.
                super::arch::smp::init_ap_gdt_idt();
            }
            #[cfg(not(target_arch = "x86_64"))]
            unsafe {
                trapframe::init()
            };
            super::arch::secondary_init();
            // now can print
        }
    }
}
