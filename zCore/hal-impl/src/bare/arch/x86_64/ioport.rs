//! x86_64 I/O Port Bitmap (IOPB) management.
//!
//! The TSS contains an I/O Permission Bitmap that controls which I/O
//! ports user-mode code (at IOPL < 3) can access without triggering
//! #GP. Each bit corresponds to one I/O port: 0 = allow, 1 = deny.
//!
//! The IOPB is 8192 bytes (65536 ports / 8 bits per byte), followed
//! by a mandatory 0xFF terminator byte.
//!
//! This module provides a global IOPB that is shared by all processes.
//! `zx_ioports_request` clears bits (allows access) and
//! `zx_ioports_release` sets bits (denies access).

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU8, Ordering};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// Size of the I/O Permission Bitmap in bytes (65536 ports / 8).
const IOPB_SIZE: usize = 8192;

/// Extended TSS: the standard 104-byte TSS followed by the IOPB
/// and a mandatory 0xFF terminator byte.
///
/// The `iomap_base` field in the TSS is set to
/// `offset_of!(TssWithIopb, iopb)` so the CPU knows where the
/// bitmap starts.
#[repr(C)]
struct TssWithIopb {
    tss: TaskStateSegment,
    iopb: [u8; IOPB_SIZE],
    /// Intel SDM requires a terminating 0xFF byte after the bitmap.
    terminator: u8,
}

/// Per-CPU extended TSS storage. Each CPU gets its own TSS (for
/// different privilege stack pointers) but they share the same
/// IOPB bitmap via `GLOBAL_IOPB`.
///
/// We use a fixed array for up to 64 CPUs. Each entry is initialized
/// to None and set during BSP/AP init.
static mut PER_CPU_TSS: [Option<&'static mut TssWithIopb>; 64] = {
    const NONE: Option<&'static mut TssWithIopb> = None;
    [NONE; 64]
};

/// Global IOPB bitmap. All bits start as 1 (deny all ports).
/// `ioports_request` clears bits; `ioports_release` sets them.
/// Using AtomicU8 for safe concurrent access from syscall handlers.
#[allow(clippy::declare_interior_mutable_const)]
static GLOBAL_IOPB: [AtomicU8; IOPB_SIZE] = {
    #[allow(clippy::declare_interior_mutable_const)]
    const DENY_ALL: AtomicU8 = AtomicU8::new(0xFF);
    [DENY_ALL; IOPB_SIZE]
};

/// Create an extended TSS with IOPB for a CPU.
///
/// Copies the trap stack from `trap_stack_top` into the new TSS.
/// Returns a static reference to the TSS portion (for GDT descriptor
/// creation) and stores the full TssWithIopb for later IOPB updates.
pub fn create_tss_with_iopb(cpu_id: u8, trap_stack_top: u64) -> &'static TaskStateSegment {
    let ext = Box::leak(Box::new(TssWithIopb {
        tss: TaskStateSegment::new(),
        iopb: [0xFF; IOPB_SIZE], // deny all by default
        terminator: 0xFF,
    }));

    // Set the privilege stack for ring 0 (trap/interrupt stack).
    ext.tss.privilege_stack_table[0] = VirtAddr::new(trap_stack_top);

    // Point iomap_base to the IOPB within this structure.
    ext.tss.iomap_base = core::mem::size_of::<TaskStateSegment>() as u16;

    // Copy current global IOPB state into this CPU's bitmap.
    for i in 0..IOPB_SIZE {
        ext.iopb[i] = GLOBAL_IOPB[i].load(Ordering::Relaxed);
    }

    // Get the TSS pointer before storing ext (which consumes the
    // mutable reference).
    let tss_ptr: *const TaskStateSegment = &ext.tss;

    // Store reference for later IOPB updates.
    unsafe {
        PER_CPU_TSS[cpu_id as usize] = Some(ext);
    }

    // Return reference to the TSS portion for GDT descriptor.
    unsafe { &*tss_ptr }
}

/// Allow access to I/O ports in range [port, port+count).
/// Updates the global IOPB and all per-CPU TSS IOPBs.
pub fn ioports_allow(port: u16, count: u32) {
    for i in 0..count {
        let p = port as u32 + i;
        if p >= 65536 {
            break;
        }
        let byte_idx = (p / 8) as usize;
        let bit_idx = (p % 8) as u8;
        // Clear bit in global bitmap (0 = allow).
        GLOBAL_IOPB[byte_idx].fetch_and(!(1 << bit_idx), Ordering::Relaxed);
    }
    // Propagate to all per-CPU TSS IOPBs.
    sync_iopb_to_all_cpus();
}

/// Deny access to I/O ports in range [port, port+count).
/// Updates the global IOPB and all per-CPU TSS IOPBs.
pub fn ioports_deny(port: u16, count: u32) {
    for i in 0..count {
        let p = port as u32 + i;
        if p >= 65536 {
            break;
        }
        let byte_idx = (p / 8) as usize;
        let bit_idx = (p % 8) as u8;
        // Set bit in global bitmap (1 = deny).
        GLOBAL_IOPB[byte_idx].fetch_or(1 << bit_idx, Ordering::Relaxed);
    }
    // Propagate to all per-CPU TSS IOPBs.
    sync_iopb_to_all_cpus();
}

/// Copy the global IOPB to all per-CPU TSS IOPBs.
fn sync_iopb_to_all_cpus() {
    unsafe {
        for entry in PER_CPU_TSS.iter_mut().flatten() {
            for i in 0..IOPB_SIZE {
                entry.iopb[i] = GLOBAL_IOPB[i].load(Ordering::Relaxed);
            }
        }
    }
}
