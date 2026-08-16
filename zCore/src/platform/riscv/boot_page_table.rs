use super::consts::{kernel_mem_info, kernel_mem_probe};
use core::arch::{asm, naked_asm};
use page_table::{MmuMeta, Pte, Sv39, VAddr, VmFlags, VmMeta, PPN};

/// Boot page table.
#[repr(align(4096))]
pub(super) struct BootPageTable([Pte<Sv39>; 512]);

impl BootPageTable {
    /// A zero-initialized boot page table.
    pub const ZERO: Self = Self([Pte::ZERO; 512]);

    /// Initialize the boot page table based on the kernel's actual location.
    pub fn init(&mut self) {
        cfg_if! {
            if #[cfg(feature = "thead-maee")] {
                const FLAGS: VmFlags<Sv39> = unsafe {
                    VmFlags::from_raw(VmFlags::<Sv39>::build_from_str("DAG_XWRV").val() | (1 << 62))
                };
            } else {
                const FLAGS: VmFlags<Sv39> = VmFlags::build_from_str("DAG_XWRV");
            }
        }

        // Before boot page table initialization, pc must be in the physical address space,
        // so it is safe to locate the kernel address information.
        let mem_info = unsafe { kernel_mem_probe() };
        // Ensure virtual and physical addresses are aligned within 1 GiB
        assert!(mem_info.offset().trailing_zeros() >= 30);
        // Map the trampoline page
        let base = VAddr::<Sv39>::new(mem_info.paddr_base)
            .floor()
            .index_in(Sv39::MAX_LEVEL);
        self.0[base] = FLAGS.build_pte(PPN::new(base << 18));
        // Map the first 128 GiB of the physical address space
        let base = VAddr::<Sv39>::new(mem_info.offset())
            .floor()
            .index_in(Sv39::MAX_LEVEL);
        for i in 0..128 {
            self.0[base + i] = FLAGS.build_pte(PPN::new(i << 18));
        }
    }

    /// Enable address translation, jump to the high address space, and set the thread pointer
    /// and kernel access permissions for user pages.
    ///
    /// # Safety
    ///
    /// The caller is in different address spaces before and after the call; must be inlined.
    #[inline(always)]
    pub unsafe fn launch(&self) -> usize {
        use riscv::register::satp;
        // Enable address translation
        satp::set(
            satp::Mode::Sv39,
            0,
            self.0.as_ptr() as usize >> Sv39::PAGE_BITS,
        );
        // The original address space is still mapped, so no need to flush the TLB
        // riscv::asm::sfence_vma_all();
        // Jump to the corresponding position in the high page
        Self::jump_higher(kernel_mem_info().offset());
        // Set kernel access to user pages
        let mut sstatus = 1usize << 18;
        asm!("csrrs {0}, sstatus, {0}", inlateout(reg) sstatus);
        sstatus | (1usize << 18)
    }

    /// Jump upward to a new address at distance `offset` and continue execution.
    ///
    /// # Safety
    ///
    /// Naked function.
    ///
    /// Causes stack relocation; pointers on the stack will become invalid!
    #[unsafe(naked)]
    unsafe extern "C" fn jump_higher(offset: usize) {
        naked_asm!("add sp, sp, a0", "add ra, ra, a0", "ret")
    }
}
