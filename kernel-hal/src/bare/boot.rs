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
            info!("Primary CPU {} init...", crate::cpu::cpu_id());
            unsafe { trapframe::init() };
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
            }
            super::arch::primary_init();
        }

        fn secondary_init() {
            // info!("Secondary CPU {} init...", crate::cpu::cpu_id());
            // we can't print anything here, see reason: zcore/main.rs::secondary_main()
            #[cfg(target_arch = "x86_64")]
            {
                // Save BSP's USER_SS/USER_CS before trapframe::init() overwrites them.
                // On x86_64, trapframe::init() extends the current GDT and recomputes
                // these globals based on the new entry count. Since the AP starts with
                // the BSP's GDT (loaded in ap_entry), the AP's init adds duplicate
                // entries, changing the selectors. We restore the BSP's values after.
                extern "C" {
                    static mut USER_SS: u16;
                    static mut USER_CS: u16;
                }
                let saved_ss = unsafe { USER_SS };
                let saved_cs = unsafe { USER_CS };
                unsafe { trapframe::init() };
                unsafe {
                    USER_SS = saved_ss;
                    USER_CS = saved_cs;
                }
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
