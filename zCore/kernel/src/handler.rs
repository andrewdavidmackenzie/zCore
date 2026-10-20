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
                panic!(
                    "handle kernel page fault error: {:?} vaddr(0x{:x}) flags({:?})",
                    err, fault_vaddr, access_flags
                );
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
                    // The page request has already been sent to the pager
                    // port by handle_page_fault. We need to wait for the
                    // page to be supplied.
                    //
                    // Since we're in a synchronous trap handler, we spin-
                    // poll the executor to let the pager thread supply
                    // the page. This matches Fuchsia's behavior where
                    // the kernel usercopy blocks on a pager wait (non-
                    // suspendable). The thread stays in BlockedPager state.
                    //
                    // Set thread state so WaitForBlocked polls detect it.
                    use zircon_object::task::ThreadState;
                    thread.set_state(ThreadState::BlockedPager);

                    // Find the pager VMO for this address and wait.
                    let pager_vmo = vmar.find_pager_vmo(fault_vaddr).or_else(|| {
                        let mapping = vmar.find_mapping(fault_vaddr)?;
                        if mapping.vmo.is_in_pager_tree() {
                            mapping.vmo.find_root_pager_vmo()
                        } else {
                            None
                        }
                    });
                    let offset = vmar.vaddr_to_vmo_offset(fault_vaddr);

                    if let (Some(vmo), Some(offset)) = (pager_vmo, offset) {
                        let page_idx = offset / hal_impl::PAGE_SIZE;
                        // Spin-poll: run the executor to let pager tasks
                        // make progress, then check if the page was
                        // supplied. The thread is killed by dropping the
                        // process, which fails all pager requests, so
                        // this loop terminates.
                        loop {
                            if vmo.committed_pages_in_range(page_idx, page_idx + 1) > 0 {
                                thread.set_state(ThreadState::Running);
                                return true;
                            }
                            // Check if the thread was killed during wait.
                            if thread.state() == ThreadState::Dying
                                || thread.state() == ThreadState::Dead
                            {
                                return false;
                            }
                            // Poll the executor to let other tasks run
                            // (e.g., the pager thread that supplies pages).
                            executor::run_until_idle();
                        }
                    } else {
                        false
                    }
                }
                Err(_) => false,
            }
        } else {
            false
        }
    }
}
