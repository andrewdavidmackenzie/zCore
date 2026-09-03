// x86_64 entry point -- placeholder until bootloader integration (#148).
//
// The previous entry expected rboot's BootInfo struct. That dependency
// has been removed. A new bootloader integration is needed.

// TODO: This entry point needs a proper bootloader to call it.
// Options being evaluated in #148:
// - bootloader crate (multiboot2 + UEFI)
// - rboot (updated to current uefi crate)
// - custom multiboot2 stub
//
// For now this is a minimal placeholder that allows the kernel to compile
// for x86_64 but cannot actually boot.

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // No bootloader provides BootInfo yet.
    loop {
        unsafe { core::arch::asm!("hlt") };
    }
}
