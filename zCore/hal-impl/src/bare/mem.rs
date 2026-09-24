//! Physical memory operations.

use alloc::vec::Vec;
use core::ops::Range;

use crate::{PhysAddr, VirtAddr, KCONFIG};

hal_fn_impl! {
    impl mod crate::hal_fn::mem {
        fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
            // Safety net: on x86_64, mask bits 52-63 which are PTE flags
            // (including NX at bit 63). Callers should extract physical
            // addresses properly via PTE_ADDR_MASK, but this prevents
            // non-canonical address faults if they don't.
            #[cfg(target_arch = "x86_64")]
            let paddr = paddr & 0x000F_FFFF_FFFF_FFFF;
            KCONFIG.phys_to_virt_offset + paddr
        }

        fn virt_to_phys(vaddr: VirtAddr) -> PhysAddr {
            vaddr - KCONFIG.phys_to_virt_offset
        }

        fn free_pmem_regions() -> Vec<Range<PhysAddr>> {
            super::arch::mem::free_pmem_regions()
        }

        fn pmem_read(paddr: PhysAddr, buf: &mut [u8]) {
            trace!("pmem_read: paddr={:#x}, len={:#x}", paddr, buf.len());
            let src = phys_to_virt(paddr) as _;
            unsafe { buf.as_mut_ptr().copy_from_nonoverlapping(src, buf.len()) };
        }

        fn pmem_write(paddr: PhysAddr, buf: &[u8]) {
            #[cfg(feature = "uefi-boot")]
            unsafe {
                let u = 0xffff_0000_0900_0000 as *mut u8;
                core::ptr::write_volatile(u, b'W');
            }
            let dst = phys_to_virt(paddr) as *mut u8;
            unsafe { dst.copy_from_nonoverlapping(buf.as_ptr(), buf.len()) };
            #[cfg(feature = "uefi-boot")]
            unsafe {
                let u = 0xffff_0000_0900_0000 as *mut u8;
                core::ptr::write_volatile(u, b'w');
            }
        }

        fn pmem_zero(paddr: PhysAddr, len: usize) {
            #[cfg(feature = "uefi-boot")]
            unsafe {
                let u = 0xffff_0000_0900_0000 as *mut u8;
                core::ptr::write_volatile(u, b'Z');
            }
            unsafe { core::ptr::write_bytes(phys_to_virt(paddr) as *mut u8, 0, len) };
            #[cfg(feature = "uefi-boot")]
            unsafe {
                let u = 0xffff_0000_0900_0000 as *mut u8;
                core::ptr::write_volatile(u, b'z');
            }
        }

        fn pmem_copy(dst: PhysAddr, src: PhysAddr, len: usize) {
            trace!("pmem_copy: {:#x} <- {:#x}, len={:#x}", dst, src, len);
            let dst = phys_to_virt(dst) as *mut u8;
            unsafe { dst.copy_from_nonoverlapping(phys_to_virt(src) as _, len) };
        }

        fn frame_flush(target: PhysAddr) {
            super::arch::mem::frame_flush(target)
        }
    }
}
