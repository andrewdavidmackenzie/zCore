mod drivers;
mod trap;

pub mod config;
pub mod cpu;
pub mod interrupt;
pub mod mem;
pub mod timer;
pub mod vm;

pub mod special;

hal_fn_impl_default!(crate::hal_fn::console);

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
}

pub fn primary_init() {
    drivers::init().unwrap();

    // enable global page
    unsafe { Cr4::update(|f| f.insert(Cr4Flags::PAGE_GLOBAL)) };
    // TODO: SMP boot -- x86_smpboot was removed (old dependency).
    // Need to implement AP startup or find a replacement. See #94.
}

pub fn timer_init() {
    timer::init();
}

pub fn secondary_init() {
    zcore_drivers::irq::x86::Apic::init_local_apic_ap();
}
