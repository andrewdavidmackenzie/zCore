// RISCV

/// Number of stack pages per hardware thread for the kernel.
pub const STACK_PAGES_PER_HART: usize = 32;

/// Maximum number of SMP hardware threads.
pub const MAX_HART_NUM: usize = 5;

#[inline]
pub fn phys_to_virt_offset() -> usize {
    kernel_mem_info().offset()
}

use spin::Once;

/// Kernel memory layout information.
pub struct KernelMemInfo {
    /// Base address of the kernel in the physical address space.
    pub paddr_base: usize,

    /// Base address of the kernel in the virtual address space.
    ///
    /// This is actually the start address of the last 1 GiB page in the virtual address space,
    /// aligned with the physical memory at a 2 MiB page offset.
    /// Consistent with the address set at link time.
    pub vaddr_base: usize,

    /// Length of the kernel linked region.
    pub size: usize,
}

impl KernelMemInfo {
    /// Initialize the physical memory information.
    ///
    /// # Safety
    ///
    /// To obtain the kernel's physical address,
    /// this function must be called while `pc` is still in the physical address space!
    unsafe fn new() -> Self {
        extern "C" {
            fn start();
            fn end();
        }
        let paddr_base = start as *const () as usize;
        let vaddr_base = 0xffff_ffc0_8020_0000;
        Self {
            paddr_base,
            vaddr_base,
            size: end as *const () as usize - paddr_base,
        }
    }

    /// Calculate the offset from the kernel virtual address space to the physical address space.
    #[inline]
    pub fn offset(&self) -> usize {
        self.vaddr_base - self.paddr_base
    }
}

static KERNEL_MEM_INFO: Once<KernelMemInfo> = Once::new();

#[inline]
pub fn kernel_mem_info() -> &'static KernelMemInfo {
    KERNEL_MEM_INFO.wait()
}

#[inline]
pub(super) unsafe fn kernel_mem_probe() -> &'static KernelMemInfo {
    KERNEL_MEM_INFO.call_once(|| KernelMemInfo::new())
}
