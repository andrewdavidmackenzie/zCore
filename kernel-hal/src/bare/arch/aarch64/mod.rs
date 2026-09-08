pub mod config;
pub mod cpu;
pub mod drivers;
pub mod interrupt;
pub mod mem;
pub mod timer;
pub mod trap;
pub mod vm;

use crate::KCONFIG;
use crate::{mem::phys_to_virt, utils::init_once::InitOnce, PhysAddr};
use alloc::string::{String, ToString};
use core::ops::Range;

hal_fn_impl_default!(crate::hal_fn::console);

static INITRD_REGION: InitOnce<Option<Range<PhysAddr>>> = InitOnce::new_with_default(None);
static CMDLINE: InitOnce<String> = InitOnce::new_with_default(String::new());

pub fn cmdline() -> String {
    CMDLINE.clone()
}

pub fn init_ram_disk() -> Option<&'static mut [u8]> {
    INITRD_REGION.as_ref().map(|range| unsafe {
        core::slice::from_raw_parts_mut(phys_to_virt(range.start) as *mut u8, range.len())
    })
}

pub fn primary_init_early() {
    // Parse DTB for bootargs and initrd if a valid DTB was provided.
    let dtb_paddr = KCONFIG.dtb_paddr;
    if dtb_paddr != 0 {
        parse_dtb(dtb_paddr);
    } else {
        CMDLINE.init_once_by(KCONFIG.cmdline.to_string());
    }
    drivers::init_early();
}

/// Parse the DTB to extract bootargs and initrd location.
fn parse_dtb(dtb_paddr: usize) {
    use dtb_walker::{Dtb, DtbObj, Property, Str, WalkOperation::*};

    let dtb_vaddr = phys_to_virt(dtb_paddr);
    let dtb = unsafe {
        Dtb::from_raw_parts_filtered(dtb_vaddr as _, |e| {
            log::warn!("DTB parse error: {:?}", e);
            false
        })
    };
    let dtb = match dtb {
        Ok(dtb) => dtb,
        Err(e) => {
            log::warn!("DTB parse failed: {:?}", e);
            log::warn!("Failed to parse DTB at {:#x}", dtb_paddr);
            CMDLINE.init_once_by(KCONFIG.cmdline.to_string());
            return;
        }
    };

    log::info!("DTB at {:#x}, size={}", dtb_paddr, dtb.total_size());

    let mut initrd_start: Option<usize> = None;
    let mut initrd_end: Option<usize> = None;
    let mut bootargs: Option<String> = None;

    dtb.walk(|path, obj| match obj {
        DtbObj::SubNode { name } => {
            if path.is_root() && name == Str::from("chosen") {
                StepInto
            } else {
                StepOver
            }
        }
        DtbObj::Property(Property::General { name, value }) => {
            if name == Str::from("bootargs") {
                if let Ok(s) = core::str::from_utf8(value) {
                    let s = s.trim_end_matches('\0');
                    log::info!("DTB bootargs: {:?}", s);
                    bootargs = Some(s.to_string());
                }
            } else if name == Str::from("linux,initrd-start") {
                initrd_start = parse_dtb_u64(value);
                log::info!("DTB initrd-start: {:#x?}", initrd_start);
            } else if name == Str::from("linux,initrd-end") {
                initrd_end = parse_dtb_u64(value);
                log::info!("DTB initrd-end: {:#x?}", initrd_end);
            }
            StepOver
        }
        _ => StepOver,
    });

    // Use DTB bootargs if available, otherwise fall back to compile-time
    let cmdline = bootargs.unwrap_or_else(|| KCONFIG.cmdline.to_string());
    CMDLINE.init_once_by(cmdline);

    // Set initrd region if both start and end are provided
    if let (Some(start), Some(end)) = (initrd_start, initrd_end) {
        if end > start {
            log::info!(
                "DTB initrd: {:#x}..{:#x} ({} bytes)",
                start,
                end,
                end - start
            );
            INITRD_REGION.init_once_by(Some(start..end));
        }
    }
}

/// Parse a DTB property value as a u64 (big-endian, 4 or 8 bytes).
fn parse_dtb_u64(value: &[u8]) -> Option<usize> {
    match value.len() {
        4 => Some(u32::from_be_bytes(value.try_into().ok()?) as usize),
        8 => Some(u64::from_be_bytes(value.try_into().ok()?) as usize),
        _ => None,
    }
}

pub fn primary_init() {
    vm::init();
    drivers::init();
}

pub fn secondary_init() {
    unimplemented!()
}

pub const fn timer_interrupt_vector() -> usize {
    30
}

pub fn timer_init() {
    timer::init();
}
