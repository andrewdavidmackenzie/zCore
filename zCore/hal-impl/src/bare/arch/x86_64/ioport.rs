//! x86_64 I/O Port Bitmap (IOPB) management.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU8, Ordering};
use x86_64::structures::gdt::Descriptor;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

const IOPB_SIZE: usize = 8192;

#[repr(C)]
pub struct TssWithIopb {
    pub tss: TaskStateSegment,
    iopb: [u8; IOPB_SIZE],
    terminator: u8,
}

static mut PER_CPU_TSS: [Option<&'static mut TssWithIopb>; 64] = {
    const NONE: Option<&'static mut TssWithIopb> = None;
    [NONE; 64]
};

#[allow(clippy::declare_interior_mutable_const)]
static GLOBAL_IOPB: [AtomicU8; IOPB_SIZE] = {
    #[allow(clippy::declare_interior_mutable_const)]
    const DENY_ALL: AtomicU8 = AtomicU8::new(0xFF);
    [DENY_ALL; IOPB_SIZE]
};

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
    unsafe { PER_CPU_TSS[cpu_id as usize] = Some(ext) }
    unsafe { &*ptr }
}

pub fn make_tss_descriptor(ext: &'static TssWithIopb) -> Descriptor {
    let iomap: &'static [u8] =
        unsafe { core::slice::from_raw_parts(ext.iopb.as_ptr(), IOPB_SIZE + 1) };
    Descriptor::tss_segment_with_iomap(&ext.tss, iomap).expect("invalid IOPB layout")
}

pub fn ioports_allow(port: u16, count: u32) {
    for i in 0..count {
        let p = port as u32 + i;
        if p >= 65536 {
            break;
        }
        GLOBAL_IOPB[(p / 8) as usize].fetch_and(!(1 << (p % 8) as u8), Ordering::Relaxed);
    }
    sync_iopb_to_all_cpus();
}

pub fn ioports_deny(port: u16, count: u32) {
    for i in 0..count {
        let p = port as u32 + i;
        if p >= 65536 {
            break;
        }
        GLOBAL_IOPB[(p / 8) as usize].fetch_or(1 << (p % 8) as u8, Ordering::Relaxed);
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
