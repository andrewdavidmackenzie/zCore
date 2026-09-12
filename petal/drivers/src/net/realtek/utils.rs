// c906
use core::arch::asm;

const FREQUENCY: u64 = 24_000_000; // C906: 24_000_000, Qemu: 10_000_000
const MMIO_MTIMECMP0: *mut u64 = 0x0200_4000usize as *mut u64;
const MMIO_MTIME: *const u64 = 0x0200_BFF8 as *const u64;

const L1_CACHE_BYTES: u64 = 64;
const CACHE_LINE_SIZE: u64 = 64;

pub fn flush_cache(addr: u64, size: u64) {
    flush_dcache_range(addr, addr + size);
}

pub fn invalidate_dcache(addr: u64, size: u64) {
    invalidate_dcache_range(addr, addr + size);
}

// Note: start takes a physical address
pub fn flush_dcache_range(start: u64, end: u64) {
    // Align to 64-byte cache line
    let end = (end + (CACHE_LINE_SIZE - 1)) & !(CACHE_LINE_SIZE - 1);

    // Align address to L1 cache line boundary
    let mut i: u64 = start & !(L1_CACHE_BYTES - 1);
    while i < end {
        unsafe {
            // Old-style llvm asm
            // DCACHE clean dirty entry at specified physical address
            // llvm_asm!("dcache.cpa $0"::"r"(i));

            // New asm
            asm!(".long 0x0295000b", in("a0") i); // dcache.cpa a0, because the compiler cannot recognize this instruction
        }

        i += L1_CACHE_BYTES;
    }

    unsafe {
        //llvm_asm!("sync.is");

        asm!(".long 0x01b0000b"); // sync.is
    }
}

// start: physical address
pub fn invalidate_dcache_range(start: u64, end: u64) {
    let end = (end + (CACHE_LINE_SIZE - 1)) & !(CACHE_LINE_SIZE - 1);
    let mut i: u64 = start & !(L1_CACHE_BYTES - 1);
    while i < end {
        unsafe {
            //llvm_asm!("dcache.ipa $0"::"r"(i)); // DCACHE invalidate entry at specified physical address
            asm!(".long 0x02a5000b", in("a0") i); // dcache.ipa a0
        }

        i += L1_CACHE_BYTES;
    }

    unsafe {
        //llvm_asm!("sync.is");
        asm!(".long 0x01b0000b"); // sync.is
    }
}

pub fn fence_w() {
    unsafe {
        //llvm_asm!("fence ow, ow" ::: "memory");
        asm!("fence ow, ow");
    }
}

pub fn get_cycle() -> u64 {
    unsafe { MMIO_MTIME.read_volatile() }
}

// Timer, Freq = 24000000Hz
// TIMER_CLOCK = (24 * 1000 * 1000)
// Microseconds (us)
pub fn usdelay(us: u64) {
    let mut t1: u64 = get_cycle();
    let t2 = t1 + us * 24;

    while t2 >= t1 {
        t1 = get_cycle();
    }
}

// Milliseconds (ms)
#[allow(unused)]
pub fn msdelay(ms: u64) {
    usdelay(ms * 1000);
}
