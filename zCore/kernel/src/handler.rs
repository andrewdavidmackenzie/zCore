use hal_impl::{KernelHandler, MMUFlags};
use zircon_object::object::KernelObject;
use zircon_object::task::Thread;

pub struct ZcoreKernelHandler;

impl KernelHandler for ZcoreKernelHandler {
    fn handle_user_trap(&self, trap_num: usize, error_code: usize) {
        // A user-mode trap (e.g. #GP, #UD) that isn't a page fault.
        //
        // This callback runs inside the HAL trap handler (interrupt context).
        // It does NOT need to stop the thread or deliver an exception here —
        // the Zircon loader's userspace loop (`run_userspace` in zircon.rs)
        // inspects `ctx.trap_reason()` after `enter_uspace()` returns and
        // dispatches the proper Zircon exception (e.g., ExceptionType::General
        // for #GP, UndefinedInstruction for #UD). If no exception handler
        // resolves it, `Exception::handle` kills the process.
        //
        // We log here for diagnostic visibility; the actual resolution
        // happens at the loader level, not in this synchronous callback.
        if let Some(thread) = hal_impl::thread::get_current_thread() {
            let thread = thread.downcast::<Thread>().unwrap();
            warn!(
                "User trap #{:#x} (error_code={:#x}) in thread '{}' (tid={})",
                trap_num,
                error_code,
                thread.name(),
                thread.id(),
            );
        } else {
            warn!(
                "User trap #{:#x} (error_code={:#x}) with no current thread",
                trap_num, error_code
            );
        }
    }

    fn frame_alloc_contiguous(&self, frame_count: usize, align_log2: usize) -> Option<usize> {
        hal_impl::memory::frame_alloc(frame_count, align_log2)
    }

    fn frame_dealloc(&self, paddr: usize) {
        hal_impl::memory::frame_dealloc(paddr);
    }

    fn handle_page_fault(&self, fault_vaddr: usize, access_flags: MMUFlags) {
        if let Some(thread) = hal_impl::thread::get_current_thread() {
            let thread = thread.downcast::<Thread>().unwrap();
            let vmar = thread.proc().vmar();
            if let Err(err) = vmar.handle_page_fault(fault_vaddr, access_flags) {
                use zircon_object::ZxError;
                if access_flags.contains(MMUFlags::USER) {
                    // User-mode page faults: do NOT kill the process here.
                    // The zircon-loader's handler_user_trap will see the
                    // same PageFault trap reason after enter_uspace()
                    // returns, call handle_page_fault again, and properly
                    // deliver a FatalPageFault exception through the
                    // exception channel. This gives user-space exception
                    // handlers (e.g., WaitForCrash in tests) a chance to
                    // intercept the fault before the process is killed.
                    //
                    // SHOULD_WAIT means a pager-backed page needs to be
                    // supplied — also handled at the loader level.
                } else if err != ZxError::SHOULD_WAIT {
                    panic!(
                        "handle kernel page fault error: {:?} vaddr(0x{:x}) flags({:?})",
                        err, fault_vaddr, access_flags
                    );
                }
            }
        } else {
            panic!(
                "page fault from kernel private address 0x{:x}, flags = {:?}",
                fault_vaddr, access_flags
            );
        }
    }

    fn try_handle_page_fault(&self, fault_vaddr: usize, access_flags: MMUFlags) -> bool {
        if let Some(thread) = hal_impl::thread::get_current_thread() {
            let thread = thread.downcast::<Thread>().unwrap();
            let vmar = thread.proc().vmar();
            match vmar.handle_page_fault(fault_vaddr, access_flags) {
                Ok(()) => true,
                Err(zircon_object::ZxError::SHOULD_WAIT) => {
                    // Pager-backed page fault during kernel usercopy.
                    // Return false to let the guarded
                    // copy fail. Usercopy demand-paging requires
                    // syscall-level pre-faulting of pager pages.
                    // TODO review correctness/completeness of this
                    false
                }
                Err(_) => false,
            }
        } else {
            false
        }
    }
}
