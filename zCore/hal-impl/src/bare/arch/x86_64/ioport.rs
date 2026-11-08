//! x86_64 I/O Port Bitmap (IOPB) management.
//!
//! The TSS contains an I/O Permission Bitmap that controls which I/O
//! ports user-mode code (at IOPL < 3) can access without triggering
//! #GP. Each bit corresponds to one I/O port: 0 = allow, 1 = deny.
//!
//! The IOPB is 8192 bytes (65536 ports / 8 bits per byte), followed
//! by a mandatory 0xFF terminator byte.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU8, Ordering};
use x86_64::structures::gdt::Descriptor;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// Size of the I/O Permission Bitmap in bytes (65536 ports / 8).
const IOPB_SIZE: usize = 8192;

/// Extended TSS: the standard TSS followed by the IOPB and a
/// mandatory 0xFF terminator byte. The x86_64 crate's
/// `Descriptor::tss_segment_with_iomap` validates this layout.
#[repr(C)]
pub struct TssWithIopb {
    pub tss: TaskStateSegment,
    iopb: [u8; IOPB_SIZE],
    terminator: u8,
}

/// Per-CPU extended TSS storage for later IOPB updates.
static mut PER_CPU_TSS: [Option<&'static mut TssWithIopb>; 64] = {
    const NONE: Option<&'static mut TssWithIopb> = None;
    [NONE; 64]
};

/// Global IOPB bitmap. All bits start as 1 (deny all ports).
#[allow(clippy::declare_interior_mutable_const)]
static GLOBAL_IOPB: [AtomicU8; IOPB_SIZE] = {
    #[allow(clippy::declare_interior_mutable_const)]
    const DENY_ALL: AtomicU8 = AtomicU8::new(0xFF);
    [DENY_ALL; IOPB_SIZE]
};

/// Create an extended TSS with IOPB for a CPU.
pub fn create_tss_with_iopb(cpu_id: u8, trap_stack_top: u64) -> &'static TssWithIopb {
    let ext = Box::leak(Box::new(TssWithIopb {
        tss: TaskStateSegment::new(),
        iopb: [0xFF; IOPB_SIZE],
        terminator: 0xFF,
    }));

    ext.tss.privilege_stack_table[0] = VirtAddr::new(trap_stack_top);
    ext.tss.iomap_base = core::mem::size_of::<TaskStateSegment>() as u16;

    for i in 0..IOPB_SIZE {
        ext.iopb[i] = GLOBAL_IOPB[i].load(Ordering::Relaxed);
    }

    let ptr: *const TssWithIopb = ext;
    unsafe {
        PER_CPU_TSS[cpu_id as usize] = Some(ext);
    }
    unsafe { &*ptr }
}

/// Build a GDT descriptor for the extended TSS with IOPB using
/// the x86_64 crate's validated API.
pub fn make_tss_descriptor(ext: &'static TssWithIopb) -> Descriptor {
    let iomap: &'static [u8] =
        unsafe { core::slice::from_raw_parts(ext.iopb.as_ptr(), IOPB_SIZE + 1) };
    Descriptor::tss_segment_with_iomap(&ext.tss, iomap).expect("invalid IOPB layout")
}

/// Allow access to I/O ports in range [port, port+count).
pub fn ioports_allow(port: u16, count: u32) {
    for i in 0..count {
        let p = port as u32 + i;
        if p >= 65536 {
            break;
        }
        let byte_idx = (p / 8) as usize;
        let bit_idx = (p % 8) as u8;
        GLOBAL_IOPB[byte_idx].fetch_and(!(1 << bit_idx), Ordering::Relaxed);
    }
    sync_iopb_to_all_cpus();
}

/// Deny access to I/O ports in range [port, port+count).
pub fn ioports_deny(port: u16, count: u32) {
    for i in 0..count {
        let p = port as u32 + i;
        if p >= 65536 {
            break;
        }
        let byte_idx = (p / 8) as usize;
        let bit_idx = (p % 8) as u8;
        GLOBAL_IOPB[byte_idx].fetch_or(1 << bit_idx, Ordering::Relaxed);
    }
    sync_iopb_to_all_cpus();
}

fn sync_iopb_to_all_cpus() {
    unsafe {
        for entry in PER_CPU_TSS.iter_mut().flatten() {
            for i in 0..IOPB_SIZE {
                entry.iopb[i] = GLOBAL_IOPB[i].load(Ordering::Relaxed);
            }
        }
    }
}
