use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use core::{any::Any, future::Future, ops::Range, time::Duration};

use crate::device_registry::prelude::{IrqHandler, IrqPolarity, IrqTriggerMode};
use crate::{common, DeviceResult, KernelConfig, KernelHandler, PhysAddr, VirtAddr};

hal_fn_def! {
    /// Bootstrap and initialization.
    pub mod boot {
        /// The kernel command line.
        ///
        /// TODO: use `&'a str` as return type.
        pub fn cmdline() -> String { String::new() }

        /// Returns the slice of the initial RAM disk, or `None` if not exist.
        pub fn init_ram_disk() -> Option<&'static mut [u8]> {
            None
        }

        /// Initialize the primary CPU at an early stage (before the physical frame allocator).
        pub fn primary_init_early(cfg: KernelConfig, handler: &'static impl KernelHandler) {}

        /// The main part of the primary CPU initialization.
        pub fn primary_init();

        /// Initialize the secondary CPUs.
        pub fn secondary_init() {}
    }

    /// CPU information.
    pub mod cpu {
        /// Current CPU ID.
        pub fn cpu_id() -> u8 { 0 }

        /// Current CPU frequency in MHz.
        pub fn cpu_frequency() -> u16 { 3000 }

        /// Shutdown/reboot the machine.
        pub fn reset() -> !;
    }

    /// Physical memory operations.
    pub mod mem: common::mem {
        /// Convert physical address to virtual address.
        pub fn phys_to_virt(paddr: PhysAddr) -> VirtAddr;

        /// Convert virtual address to physical address.
        pub fn virt_to_phys(vaddr: VirtAddr) -> PhysAddr;

        /// Returns all free physical memory regions.
        pub fn free_pmem_regions() -> Vec<Range<PhysAddr>>;

        /// Read physical memory from `paddr` to `buf`.
        pub fn pmem_read(paddr: PhysAddr, buf: &mut [u8]);

        /// Write physical memory to `paddr` from `buf`.
        pub fn pmem_write(paddr: PhysAddr, buf: &[u8]);

        /// Zero physical memory at `[paddr, paddr + len)`.
        pub fn pmem_zero(paddr: PhysAddr, len: usize);

        /// Copy content of physical memory `src` to `dst` with `len` bytes.
        pub fn pmem_copy(dst: PhysAddr, src: PhysAddr, len: usize);

        /// Flush the physical frame.
        pub fn frame_flush(target: PhysAddr);
    }

    /// Virtual memory operations.
    pub mod vm: common::vm {
        /// Read the current VM token, which is the page table root address on
        /// various architectures. (e.g. CR3, SATP, ...)
        pub fn current_vmtoken() -> PhysAddr;

        /// Activate the page table associated with the `vmtoken` by writing the
        /// page table root address.
        pub fn activate_paging(vmtoken: PhysAddr);

        /// Flush TLB by the associated `vaddr`, or flush the entire TLB. (`vaddr` is `None`).
        pub(crate) fn flush_tlb(vaddr: Option<VirtAddr>);

        /// Clone kernel space entries (top level only) from `src` page table to `dst` page table.
        pub(crate) fn pt_clone_kernel_space(dst_pt_root: PhysAddr, src_pt_root: PhysAddr);
    }

    /// Interrupts management.
    pub mod interrupt {
        /// Suspend the CPU (also enable interrupts) and wait for an interrupt
        /// to occurs, then disable interrupts.
        pub fn wait_for_interrupt() {
            core::hint::spin_loop();
        }

        /// Is a valid IRQ number.
        pub fn is_valid_irq(vector: usize) -> bool;

        /// Enable the interrupts
        pub fn intr_on();

        /// Disable the interrupts
        pub fn intr_off();

        /// Test weather interrupt is enabled
        pub fn intr_get() -> bool;

        /// Disable IRQ.
        pub fn mask_irq(vector: usize) -> DeviceResult;

        /// Enable IRQ.
        pub fn unmask_irq(vector: usize) -> DeviceResult;

        /// Configure the specified interrupt vector. If it is invoked, it must be
        /// invoked prior to interrupt registration.
        pub fn configure_irq(vector: usize, tm: IrqTriggerMode, pol: IrqPolarity) -> DeviceResult;

        /// Add an interrupt handler to an IRQ.
        pub fn register_irq_handler(vector: usize, handler: IrqHandler) -> DeviceResult;

        /// Remove the interrupt handler to an IRQ.
        pub fn unregister_irq_handler(vector: usize) -> DeviceResult;

        /// Handle IRQ.
        pub fn handle_irq(vector: usize);

        /// Method used for platform allocation of blocks of MSI and MSI-X compatible
        /// IRQ targets.
        pub fn msi_alloc_block(requested_irqs: usize) -> DeviceResult<Range<usize>>;

        /// Method used to free a block of MSI IRQs previously allocated by msi_alloc_block().
        /// This does not unregister IRQ handlers.
        pub fn msi_free_block(block: Range<usize>) -> DeviceResult;

        /// Register a handler function for a given msi_id within an msi_block_t. Passing a
        /// NULL handler will effectively unregister a handler for a given msi_id within the
        /// block.
        pub fn msi_register_handler(block: Range<usize>, msi_id: usize, handler: IrqHandler) -> DeviceResult;

        pub fn send_ipi(cpuid: usize, reason: usize) -> DeviceResult;

        pub fn ipi_reason() -> Vec<usize>;
    }

    pub mod console {
        pub fn console_write_early(_s: &str) {}
    }

    /// Thread spawning.
    pub mod thread: common::thread {
        /// Spawn a new thread.
        pub fn spawn(future: impl Future<Output = ()> + Send + 'static);

        /// Set tid and pid of current task.
        pub fn set_current_thread(thread: Option<Arc<dyn Any + Send + Sync>>) {}

        /// Get tid and pid of current task.
        pub fn get_current_thread() -> Option<Arc<dyn Any + Send + Sync>> { None }
    }

    /// Time and clock functions.
    pub mod timer {
        /// Set the first time interrupt
        pub fn timer_enable();

        /// Get monotonic time (duration since boot).
        ///
        /// This is the CLOCK_MONOTONIC source. It never goes backwards
        /// and is not affected by NTP or manual clock adjustments.
        pub fn timer_now() -> Duration;

        /// Get wall-clock (real) time as duration since Unix epoch.
        ///
        /// This is the CLOCK_REALTIME source. In bare-metal mode without
        /// an RTC, this falls back to boot-relative time (same as
        /// `timer_now()`). In libos mode, this returns `SystemTime::now()`.
        pub fn timer_clock_realtime() -> Duration {
            // Default: same as monotonic (no RTC available).
            // Overridden in libos mode to return wall-clock time.
            timer_now()
        }

        /// Converting from now-relative durations to absolute deadlines.
        pub fn deadline_after(dur: Duration) -> Duration {
            timer_now() + dur
        }

        /// Set a new timer. After `deadline`, the `callback` will be called.
        /// TODO: use `Instant` as the type of `deadline`.
        pub fn timer_set(deadline: Duration, callback: Box<dyn FnOnce(Duration) + Send + Sync>);

        /// Check timers, call when timer interrupt happened.
        pub(crate) fn timer_tick();
    }

    /// Random number generator.
    pub mod rand {
        /// Fill random bytes to the buffer
        #[allow(unused_variables)]
        pub fn fill_random(buf: &mut [u8]) {
            cfg_if! {
                if #[cfg(target_arch = "x86_64")] {
                    // TODO: optimize
                    for x in buf.iter_mut() {
                        let mut r = 0;
                        unsafe { core::arch::x86_64::_rdrand16_step(&mut r) };
                        *x = r as _;
                    }
                } else {
                    static mut SEED: u64 = 0xdead_beef_cafe_babe;
                    for x in buf.iter_mut() {
                        unsafe {
                            // from musl
                            SEED = SEED.wrapping_mul(0x5851_f42d_4c95_7f2d);
                            *x = (SEED >> 33) as u8;
                        }
                    }
                }
            }
        }
    }

    /// Platform capabilities and policies.
    ///
    /// These allow OS crates to query platform behavior without
    /// checking `cfg(feature = "libos")` directly.
    pub mod platform {
        /// Whether the kernel is running as a hosted process (libos mode).
        ///
        /// When true, the kernel runs inside a host OS process and uses
        /// host memory mappings. When false, it runs on bare hardware
        /// with its own page tables.
        pub fn is_hosted() -> bool { false }

        /// Whether user-space memory writes via VMO need to be followed
        /// by a direct write_volatile to the user VA for coherency.
        ///
        /// On some libos hosts (e.g., macOS with 16K pages), mmap'd
        /// user pages may not see writes to the backing VMO storage
        /// until explicitly flushed through the user VA.
        pub fn needs_user_write_flush() -> bool { false }

        /// Whether the platform needs kernel-side TEXTREL relocation
        /// for PIE binaries (because W^X prevents runtime relocation).
        pub fn needs_kernel_textrel() -> bool { false }

        /// Whether syscall args 7-8 come from the user stack instead
        /// of registers. True on x86_64 libos where the host ABI uses
        /// the stack for overflow arguments.
        pub fn syscall_args_from_stack() -> bool { false }

        /// Returns the host filesystem path for the rootfs directory
        /// in libos mode.  Returns `None` on bare-metal.
        ///
        /// The `personality` parameter is `"linux"` or `"zircon"`.
        pub fn libos_rootfs_path(personality: &str) -> Option<String> { None }
    }

    /// VDSO constants.
    pub mod vdso: common::vdso {
        /// Get platform specific information.
        pub fn vdso_constants() -> VdsoConstants {
            vdso_constants_template()
        }
    }
}
