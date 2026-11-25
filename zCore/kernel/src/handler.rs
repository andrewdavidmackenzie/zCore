use alloc::sync::Arc;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;
use hal_impl::{KernelHandler, KernelTrapInfo, KernelTrapOutcome, MMUFlags, PhysAddr};
use zircon_object::object::KernelObject;
use zircon_object::task::{Process, Task, Thread, TASK_RETCODE_EXCEPTION_KILL};

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

    fn handle_kernel_trap_fatal(&self, info: &KernelTrapInfo) -> KernelTrapOutcome {
        // Stage the current thread's death so the rest of the system
        // observes it: the process's death signals fire, waiters wake,
        // and component_manager sees the crash and tears down the
        // component — the same observable behavior as a userspace
        // crash.
        //
        // #829/#862 lock discipline: this runs in interrupt context
        // with interrupts disabled (x86 interrupt gates clear IF on
        // entry). We must NOT acquire a KernelMutex here — a contended
        // acquisition cannot yield from interrupt context and would
        // escalate to the mutex.rs nested-spin panic (the historical
        // multi-CPU panic cascade during process-crash teardown, where
        // every CPU faulting into a teardown path contended the dying
        // process's inner locks). Instead the thread is queued in a
        // lock-free slot array; the reaper task (spawned at boot, see
        // `spawn_reaper`) performs the real teardown in task context
        // where locks can be taken and yields are safe.
        if let Some(thread) = hal_impl::thread::get_current_thread() {
            if let Ok(thread) = thread.downcast::<Thread>() {
                // #829/#862 review: do NOT call thread.name() here —
                // it takes KObjectBase.inner (a KernelMutex) and
                // clones a String (heap allocation). If the trap
                // interrupted code holding either lock, this line
                // would deadlock or run drop glue in interrupt
                // context. `id` is a plain field — safe to read. The
                // reaper logs the thread's name in task context.
                error!(
                    "kernel trap on CPU{} ({}: trap_num={:#x} rip={:#x}): \
                     killing thread tid={}",
                    info.cpu,
                    info.reason,
                    info.trap_num,
                    info.rip,
                    thread.id()
                );
                if queue_reap(thread) {
                    return KernelTrapOutcome::Redirect;
                }
                // Reap queue full — the death cannot be staged; halt
                // this CPU instead of redirecting into a staging that
                // never completes (the abandoned poll would spin in
                // the runtime loop forever).
                error!("[ktrap-reaper] reap queue full; halting CPU{}", info.cpu);
                return KernelTrapOutcome::Halt;
            }
        }
        // No current thread (pure interrupt context) — nothing to kill.
        error!(
            "kernel trap on CPU{} ({}: trap_num={:#x} rip={:#x}): \
             no current thread; halting this CPU",
            info.cpu, info.reason, info.trap_num, info.rip
        );
        KernelTrapOutcome::Halt
    }

    fn frame_alloc_contiguous(&self, frame_count: usize, align_log2: usize) -> Option<PhysAddr> {
        hal_impl::memory::frame_alloc(frame_count, align_log2)
    }

    fn frame_dealloc(&self, paddr: PhysAddr) {
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

// ── Kernel-trap reaper (#523) ──────────────────────────────────────

/// Lock-free staging slots for threads killed by kernel-mode traps.
///
/// The trap handler (interrupt context, interrupts disabled) cannot
/// take KernelMutexes (#829/#862), so it parks the doomed thread's
/// `Arc` here and the reaper task picks it up. Each slot holds the
/// `Arc::into_raw` pointer (0 = empty). Producers claim a slot with a
/// CAS; the reaper drains with a swap. Interrupt-safe: no locks, no
/// allocation.
#[cfg_attr(feature = "baremetal-test", allow(dead_code))]
static REAP_SLOTS: [AtomicUsize; 16] = [const { AtomicUsize::new(0) }; 16];

/// Queue a thread for task-context teardown. Interrupt-safe.
#[cfg_attr(feature = "baremetal-test", allow(dead_code))]
/// Queue a thread for reaping. Returns false when every slot is
/// full — the caller must then choose a fallback (halt) instead of
/// redirecting, because the staged death would never complete.
fn queue_reap(thread: Arc<Thread>) -> bool {
    let ptr = Arc::into_raw(thread) as usize;
    for slot in &REAP_SLOTS {
        if slot
            .compare_exchange(0, ptr, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
        {
            return true;
        }
    }
    // Queue full: LEAK the reference (do not resurrect-and-drop).
    // Dropping from interrupt context would run the Thread drop glue
    // — kernel locks, signal delivery — exactly what the slot queue
    // exists to avoid. The leak is bounded by the slot count and
    // only occurs under a crash storm.
    error!("[ktrap-reaper] reap queue full — leaking thread handle");
    core::mem::forget(unsafe { Arc::from_raw(ptr as *const Thread) });
    false
}

/// The reaper task: completes staged thread deaths in task context.
///
/// For each queued thread, performs the full teardown a userspace
/// crash would get: `thread.kill()` (terminal state, waiters wake) and
/// `Process::exit(TASK_RETCODE_EXCEPTION_KILL)` (process signals fire,
/// component_manager observes the crash and tears down the component).
/// The thread's own future was orphaned by
/// `executor::abandon_current_poll` — it never runs its exit path — so
/// the reaper drives the observable effects from outside.
#[cfg_attr(feature = "baremetal-test", allow(dead_code))]
async fn reaper_loop() {
    loop {
        for slot in &REAP_SLOTS {
            let ptr = slot.swap(0, Ordering::SeqCst);
            if ptr == 0 {
                continue;
            }
            let thread = unsafe { Arc::from_raw(ptr as *const Thread) };
            let proc = thread.proc();
            error!(
                "[ktrap-reaper] completing staged death of thread {} ('{}') in process {} ('{}')",
                thread.id(),
                thread.name(),
                proc.id(),
                proc.name()
            );
            thread.kill();
            Process::exit(&proc, TASK_RETCODE_EXCEPTION_KILL);
        }
        // Poll interval: kernel faults are rare; 50ms is instant on
        // the scale of a crash teardown cascade.
        hal_impl::thread::sleep_until(hal_impl::timer::timer_now() + Duration::from_millis(50))
            .await;
    }
}

/// Spawn the kernel-trap reaper task. Call once during kernel init,
/// after the executor runtimes exist.
#[cfg_attr(feature = "baremetal-test", allow(dead_code))]
pub fn spawn_reaper() {
    hal_impl::thread::spawn(reaper_loop());
}
