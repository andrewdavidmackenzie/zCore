mod drivers;
pub(crate) mod fb_console;
mod smp;
pub(crate) mod trap;

pub mod config;
pub mod cpu;
pub mod interrupt;
pub mod mem;
pub mod timer;
pub mod vm;

pub mod special;

hal_fn_impl! {
    impl mod crate::hal_fn::console {
        fn console_write_early(s: &str) {
            // COM1 (0x3F8): direct register writes, always available on x86.
            for b in s.bytes() {
                unsafe {
                    // Wait for THR empty (LSR bit 5)
                    while x86::io::inb(0x3FD) & 0x20 == 0 {}
                    x86::io::outb(0x3F8, b);
                }
            }
            // Framebuffer: visible output on real hardware without serial.
            fb_console::write_str(s);
        }
    }
}

use crate::KCONFIG;
use x86_64::registers::control::{Cr4, Cr4Flags};

pub const fn timer_interrupt_vector() -> usize {
    trap::X86_INT_APIC_TIMER
}

pub fn cmdline() -> alloc::string::String {
    KCONFIG.cmdline.into()
}

pub fn init_ram_disk() -> Option<&'static mut [u8]> {
    if KCONFIG.initrd_start == 0 || KCONFIG.initrd_size == 0 {
        return None;
    }
    // The bootloader crate maps the ramdisk into the kernel's virtual address
    // space and provides the virtual address in BootInfo.ramdisk_addr.
    // Do NOT apply phys_to_virt -- the address is already virtual.
    let start = KCONFIG.initrd_start as usize;
    Some(unsafe { core::slice::from_raw_parts_mut(start as *mut u8, KCONFIG.initrd_size as usize) })
}

pub fn primary_init_early() {
    // init serial output first
    drivers::init_early().unwrap();
    // init framebuffer console (if available)
    fb_console::init();
    // Log key boot values for diagnostics
    info!(
        "phys_to_virt_offset = {:#x}",
        crate::KCONFIG.phys_to_virt_offset
    );
    if let Some(ref fb) = *config::FRAMEBUFFER {
        info!(
            "framebuffer: {}x{}, bpp inferred, phys={:#x}",
            fb.width, fb.height, fb.addr
        );
    }
}

pub fn primary_init() {
    // Save the BSP's GDT descriptor so APs can load the same GDT.
    // This must happen after trapframe::init() (called in boot.rs)
    // which sets up the GDT with user-mode segments.
    smp::save_bsp_gdt();

    drivers::init().unwrap();

    // Enable SSE support for user-space programs.
    // User-space code (e.g. busybox compiled with SSE2) will #UD on any
    // SSE instruction without these flags.
    unsafe {
        // Clear CR0.EM (x87 emulation) -- must be clear for SSE to work.
        use x86_64::registers::control::{Cr0, Cr0Flags};
        Cr0::update(|f| f.remove(Cr0Flags::EMULATE_COPROCESSOR));

        Cr4::update(|f| {
            f.insert(Cr4Flags::PAGE_GLOBAL);
            f.insert(Cr4Flags::OSFXSR); // enable FXSAVE/FXRSTOR
            f.insert(Cr4Flags::OSXMMEXCPT_ENABLE); // enable SSE exceptions
        });

        // Detect SMAP support and enable it if available.
        // copy_from_user/copy_to_user APIs bracket user memory access
        // with stac/clac when SMAP is active.
        crate::user::init_smap();
    }
    // FPU/SSE state is saved/restored via FXSAVE/FXRSTOR in
    // UserContext::enter_uspace() (hal-impl/src/common/context.rs).

    // Boot application processors (SMP)
    // Note: SMP boot hangs on ThinkPad P1 Gen 3 real hardware (#295),
    // but works on QEMU.
    smp::boot_application_processors();
}

pub fn timer_init() {
    timer::init();
}

pub fn secondary_init() {
    ::drivers::irq::x86::Apic::init_local_apic_ap();
}
pub mod platform;
