use crate::MMUFlags;
use hal::TrapReason;
use trapframe::TrapFrame;

/// Get [`TrapReason`] from `trap_num` and `error_code` in trap frame.
pub fn trap_reason_from(trap_num: usize, error_code: usize) -> TrapReason {
    use x86::irq::*;
    const X86_INT_BASE: u8 = 0x20;
    const X86_INT_MAX: u8 = 0xff;

    if trap_num == 0x100 {
        return TrapReason::Syscall;
    }
    match trap_num as u8 {
        DEBUG_VECTOR => TrapReason::HardwareBreakpoint,
        BREAKPOINT_VECTOR => TrapReason::SoftwareBreakpoint,
        INVALID_OPCODE_VECTOR => TrapReason::UndefinedInstruction,
        ALIGNMENT_CHECK_VECTOR => TrapReason::UnalignedAccess,
        PAGE_FAULT_VECTOR => {
            bitflags::bitflags! {
                struct PageFaultErrorCode: u32 {
                    const PRESENT =     1 << 0;
                    const WRITE =       1 << 1;
                    const USER =        1 << 2;
                    const RESERVED =    1 << 3;
                    const INST =        1 << 4;
                }
            }
            let fault_vaddr = x86_64::registers::control::Cr2::read()
                .expect("invalid CR2")
                .as_u64() as _;
            let code = PageFaultErrorCode::from_bits_truncate(error_code as u32);
            let mut flags = MMUFlags::empty();
            if code.contains(PageFaultErrorCode::WRITE) {
                flags |= MMUFlags::WRITE
            } else {
                flags |= MMUFlags::READ
            }
            if code.contains(PageFaultErrorCode::USER) {
                flags |= MMUFlags::USER
            }
            if code.contains(PageFaultErrorCode::INST) {
                flags |= MMUFlags::EXECUTE
            }
            if code.contains(PageFaultErrorCode::RESERVED) {
                error!("page table entry has reserved bits set!");
            }
            TrapReason::PageFault(fault_vaddr, flags)
        }
        vec @ X86_INT_BASE..=X86_INT_MAX => TrapReason::Interrupt(vec as usize),
        _ => TrapReason::GeneralFault(trap_num),
    }
}

pub(super) const X86_INT_LOCAL_APIC_BASE: usize = 0xf0;
pub(super) const _X86_INT_APIC_SPURIOUS: usize = X86_INT_LOCAL_APIC_BASE;
pub(super) const X86_INT_APIC_TIMER: usize = X86_INT_LOCAL_APIC_BASE + 0x1;
pub(super) const _X86_INT_APIC_ERROR: usize = X86_INT_LOCAL_APIC_BASE + 0x2;

/// Cross-CPU wake-IPI vector (executor `ipi_wakeup`).
///
/// MUST stay in a strictly LOWER APIC priority class than the timer
/// (`X86_INT_APIC_TIMER` = 0xF1, class 15). A wake IPI can end up
/// accepted-but-never-EOI'd (observed identically under KVM and TCG;
/// see the self-heal in `super_timer`): its ISR bit stays set, PPR
/// rises to the vector's class, and the local APIC blocks delivery of
/// every same-or-lower-class interrupt on that CPU. At the old vector
/// 0xFE (class 15) that also blocked the timer — a halted CPU then
/// slept forever, and any task spawned to it never ran (the
/// ProcessTest.SuspendTwiceBeforeCreatingThreads +
/// SuspendWithDyingThread pair hang, #523). At 0x80 (class 8) the
/// pending timer always outranks the stuck bit, so the next tick
/// wakes the executor loop and runs the self-heal below, which EOIs
/// the wedged bit and restores normal delivery.
pub(super) const X86_INT_IPI_WAKEUP: usize = 0x80;

// ISA IRQ numbers
pub(super) const _X86_ISA_IRQ_PIT: usize = 0;
#[cfg(feature = "ps2-keyboard")]
pub(super) const X86_ISA_IRQ_KEYBOARD: usize = 1;
#[cfg(not(feature = "ps2-keyboard"))]
pub(super) const _X86_ISA_IRQ_KEYBOARD: usize = 1;
pub(super) const _X86_ISA_IRQ_PIC2: usize = 2;
pub(super) const X86_ISA_IRQ_COM2: usize = 3;
pub(super) const X86_ISA_IRQ_COM1: usize = 4;
pub(super) const _X86_ISA_IRQ_CMOSRTC: usize = 8;
pub(super) const _X86_ISA_IRQ_MOUSE: usize = 12;
pub(super) const _X86_ISA_IRQ_IDE: usize = 14;

/// Handle a kernel-mode breakpoint trap: fatal with a full dump
/// (routed through the same no-panic path as other kernel traps).
fn breakpoint(tf: &TrapFrame) -> ! {
    kernel_trap_fatal("kernel-mode breakpoint", tf)
}

pub(super) fn super_timer() {
    crate::timer::timer_tick();
    // #523/#862 wedge self-heal: if a wake IPI was accepted but its
    // handler never ran to the EOI (observed under both KVM and TCG;
    // trigger still under investigation), its ISR bit stays set and
    // blocks all same-or-lower-class interrupt delivery on this CPU.
    // The timer vector outranks the wake IPI by design (class 15 vs
    // 8 — see X86_INT_IPI_WAKEUP), so this handler still runs on a
    // wedged CPU: detect the stuck bit and EOI it. One register read
    // per tick; clearing cannot hit a legitimate in-service bit,
    // because Apic::handle_irq already sent the timer's own EOI
    // before dispatching here and IRQ handlers never nest (they run
    // with IF=0).
    let lapic = ::drivers::irq::x86::Apic::local_apic();
    let word = lapic.isr_word(X86_INT_IPI_WAKEUP / 32);
    if word & (1 << (X86_INT_IPI_WAKEUP % 32)) != 0 {
        error!(
            "[ipi-wedge] CPU{}: wake IPI #{:#x} in-service without EOI — clearing",
            super::cpu::cpu_id(),
            X86_INT_IPI_WAKEUP
        );
        lapic.eoi();
    }
}

/// Attempt recovery from a kernel-mode #GP by skipping the faulting
/// instruction when it is an MSR access (`rdmsr` = `0F 32`,
/// `wrmsr` = `0F 30`).
///
/// #GP from these two instructions means the MSR does not exist on
/// this CPU — e.g. x2APIC MSRs touched before enabling x2APIC mode,
/// or model-specific registers absent under a particular
/// virtualization configuration. Both instructions are fixed 2-byte
/// encodings, so the recovery is well-defined: advance RIP past the
/// instruction and, for reads, zero the result registers (the MSR
/// "reads as zero"). The system keeps running (with an error logged)
/// instead of parking the CPU in a panic loop.
///
/// This is deliberately narrow: only MSR accesses are skipped, only
/// in kernel mode. Any other #GP is still fatal — skipping an
/// arbitrary faulting instruction is unsafe.
///
/// Returns `true` if recovered (`tf` updated; the caller returns and
/// the trap.S restore path resumes past the instruction).
fn try_recover_kernel_gp(tf: &mut TrapFrame) -> bool {
    if tf.trap_num != 0x0d {
        return false;
    }
    // The instruction bytes at the faulting RIP. Kernel text is
    // always mapped — a code-fetch fault would be a #PF, not #GP —
    // but clamp the read to the page boundary anyway: the second
    // byte of an instruction at page end may be unmapped (#523
    // review). rdmsr/wrmsr are 2-byte instructions; a 1-byte tail
    // simply fails both matches.
    let avail = (4096 - (tf.rip & 0xfff)).min(2);
    let mut code_buf = [0u8; 2];
    unsafe { core::ptr::copy_nonoverlapping(tf.rip as *const u8, code_buf.as_mut_ptr(), avail) };
    let code = &code_buf[..avail];
    let cpu = super::cpu::cpu_id();
    match code {
        [0x0f, 0x32] => {
            // rdmsr: ECX holds the MSR index at fault time; the
            // result registers are undefined on #GP — zero them.
            error!(
                "rdmsr({:#x}) #GP at rip={:#x} on CPU{}: MSR not present — \
                 skipping instruction, result zeroed",
                tf.rcx, tf.rip, cpu
            );
            tf.rax = 0;
            tf.rdx = 0;
            tf.rip += 2;
            true
        }
        [0x0f, 0x30] => {
            // wrmsr: the write simply did not happen.
            error!(
                "wrmsr({:#x}) #GP at rip={:#x} on CPU{}: MSR not present — \
                 skipping instruction (write dropped)",
                tf.rcx, tf.rip, cpu
            );
            tf.rip += 2;
            true
        }
        _ => false,
    }
}

/// Handle an unrecoverable kernel-mode trap: dump the complete frame
/// through the early console, stage the current thread's death, and
/// either redirect the excursion coroutine to the thread-exit path or
/// halt this CPU.
///
/// The kernel never panics here. The dump goes through the direct
/// early-console path (no log filtering, no console-lock contention)
/// BEFORE anything else — that is where messages were lost
/// historically (the KVM x2APIC #GP: 7 CPUs parked with no output).
///
/// After the dump, [`KernelHandler::handle_kernel_trap_fatal`] lets
/// the kernel stage the current thread's death so the rest of the
/// system observes it (waiters woken, crashsvc runs). If the thread's
/// user excursion is active, the coroutine is resumed at its
/// `syscall_return` marker so the thread exits through the loader's
/// task-context path and this CPU keeps scheduling work. Otherwise
/// this CPU halts: there is no safe way to unwind a kernel-mode fault
/// through possibly-held locks, and returning would re-execute the
/// faulting instruction forever. Every other CPU keeps running.
fn kernel_trap_fatal(reason: &'static str, tf: &TrapFrame) -> ! {
    // Instruction bytes at the faulting RIP — the single most useful
    // diagnostic (identifies the faulting instruction). Kernel text
    // is always mapped (a code-fetch fault would be #PF, not #GP),
    // but an instruction at the very end of a mapped page may be
    // followed by an unmapped one — clamp the read to the page
    // boundary instead of assuming 8 readable bytes (#523 review).
    let page_avail = 4096 - (tf.rip & 0xfff);
    let n = page_avail.min(8);
    let mut code = [0u8; 8];
    unsafe { core::ptr::copy_nonoverlapping(tf.rip as *const u8, code.as_mut_ptr(), n) };
    let cpu = super::cpu::cpu_id();
    // #523 review: do NOT go through `console_write_fmt` — it takes
    // CONSOLE_WRITER, and if the trap interrupted the context holding
    // that lock, the dump would spin forever and never print. Write
    // via the lock-free early console (same as the panic handler).
    struct FatalWriter;
    impl core::fmt::Write for FatalWriter {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            crate::hal_fn::console::console_write_early(s);
            Ok(())
        }
    }
    use core::fmt::Write as _;
    let _ = FatalWriter.write_fmt(format_args!(
        "\n[ktrap] CPU{} {}: trap_num={:#x} error_code={:#x}\n\
         [ktrap] rip={:#x} cs={:#x} rflags={:#x} rsp={:#x}\n\
         [ktrap] rax={:#x} rbx={:#x} rcx={:#x} rdx={:#x}\n\
         [ktrap] rsi={:#x} rdi={:#x} rbp={:#x} r8={:#x}\n\
         [ktrap] r9={:#x} r10={:#x} r11={:#x} r12={:#x}\n\
         [ktrap] r13={:#x} r14={:#x} r15={:#x}\n\
         [ktrap] code@rip: {:02x?}\n",
        cpu,
        reason,
        tf.trap_num,
        tf.error_code,
        tf.rip,
        tf.cs,
        tf.rflags,
        tf.rsp,
        tf.rax,
        tf.rbx,
        tf.rcx,
        tf.rdx,
        tf.rsi,
        tf.rdi,
        tf.rbp,
        tf.r8,
        tf.r9,
        tf.r10,
        tf.r11,
        tf.r12,
        tf.r13,
        tf.r14,
        tf.r15,
        code,
    ));
    // Let the kernel stage the current thread's death (lock-free —
    // see KernelHandler::handle_kernel_trap_fatal). Must not unwind;
    // the handler is advisory.
    use hal::kernel_handler::{KernelTrapInfo, KernelTrapOutcome};
    let outcome = crate::KHANDLER.handle_kernel_trap_fatal(&KernelTrapInfo {
        trap_num: tf.trap_num,
        error_code: tf.error_code,
        rip: tf.rip,
        rsp: tf.rsp,
        cpu: cpu as usize,
        reason,
    });
    if matches!(outcome, KernelTrapOutcome::Redirect) {
        // A thread's death was staged and the executor is mid-poll on
        // this CPU. Abandon the faulted poll: switch to the runtime
        // idle context WITHOUT saving the faulted context (it must
        // never be resumed — the demoted-weak-executor machinery would
        // otherwise re-enter the fault forever). The faulted task is
        // orphaned (its Arc<Thread> leaks — bounded by the number of
        // kernel faults) and the executor stack is reused by
        // subsequent polls. The reaper task completes the staged death
        // in task context. Never returns on success.
        //
        // abandon_current_poll requires interrupts disabled (the
        // runtime-lock discipline — it must not race the timer's
        // sched_yield); trap entry had IF=0 but this handler may have
        // re-enabled it (see trap_handler).
        crate::interrupt::intr_off();
        if executor::abandon_current_poll() {
            unreachable!()
        }
        // Could not abandon (runtime lock held by the faulted code, no
        // running poll, ...) — fall through to halt.
        crate::console::console_write_fmt(format_args!(
            "[ktrap] abandon_current_poll failed — halting this CPU\n"
        ));
    }
    // Terminal state for THIS CPU only: interrupts on (it can still
    // service IPIs), sleep forever. Other CPUs keep running (and the
    // reaper completes the staged death).
    crate::interrupt::intr_on();
    loop {
        unsafe { core::arch::asm!("hlt") };
    }
}

#[no_mangle]
pub extern "C" fn trap_handler(tf: &mut TrapFrame) {
    trace!(
        "Interrupt: {:#x} @ CPU{}",
        tf.trap_num,
        super::cpu::cpu_id()
    );

    // #862 fix: every IDT entry is a 64-bit *interrupt gate* (trapframe
    // sets `EntryOptions::minimal()` = type 0xE), so ALL traps — not
    // just IRQs — enter with IF=0. Kernel-mode fault handling
    // (`handle_page_fault` → VMAR inner KernelMutexes, the fatal path)
    // may contend those locks; with IF=0 a contended `KernelMutex`
    // cannot yield and escalates to the mutex.rs nested-spin panic —
    // the historical multi-CPU panic cascade during process-crash
    // teardown. The trap frame is fully saved by `__alltraps` before
    // this point, so re-enabling interrupts here is safe.
    //
    // IRQs keep IF=0: interrupt handlers must not nest, and the
    // interrupted rflags are restored by `iretq` on return.
    //
    // Faults that interrupted code already running with IF=0 (an IRQ
    // handler holding an IrqLock) also keep IF=0: enabling interrupts
    // there would let the fault handler yield and suspend the lock
    // holder mid-critical-section (the observed residual
    // lock_order.rs:135 panics — the suspended handler's slot stays
    // nonzero while parked). Restoring the interrupted context's own
    // IF state is exactly the right semantics.
    // Compute the reason ONCE, before re-enabling interrupts: a
    // page-fault address is read from CR2, and a timer IRQ landing in
    // the gap below could page-fault and overwrite CR2, making the
    // match dispatch a different fault than the guard checked
    // (#523 review).
    let reason = trap_reason_from(tf.trap_num, tf.error_code);
    if tf.rflags & 0x200 != 0 && !matches!(reason, TrapReason::Interrupt(_)) {
        crate::interrupt::intr_on();
    }

    match reason {
        TrapReason::HardwareBreakpoint | TrapReason::SoftwareBreakpoint => breakpoint(tf),
        TrapReason::PageFault(vaddr, flags) => {
            // On x86, the PRESENT bit in the error code distinguishes
            // unmapped pages (bit 0 = 0) from permission violations
            // (bit 0 = 1). Pass this to the user-copy fault handler.
            let is_permission_fault = (tf.error_code & 1) != 0;
            if let Some(recovery_pc) =
                crate::thread::user_copy_check_fault_with_perm(vaddr, is_permission_fault)
            {
                // Inside a guarded user-copy. Try to resolve the fault
                // first (e.g., demand-page the user data). Only redirect
                // to recovery if the page fault handler fails.
                if crate::KHANDLER.try_handle_page_fault(vaddr, flags) {
                    // Fault resolved — re-arm the guard for subsequent
                    // faults in the same multi-page copy, then resume.
                    crate::thread::user_copy_rearm(recovery_pc);
                } else {
                    // Unresolvable fault — recovery PC is already set
                    // by user_copy_check_fault; redirect execution.
                    tf.rip = recovery_pc;
                }
                return;
            }
            crate::KHANDLER.handle_page_fault(vaddr, flags);
        }
        TrapReason::Interrupt(vector) => {
            crate::interrupt::handle_irq(vector);
            if vector == X86_INT_APIC_TIMER {
                executor::set_preempt_pending();
                executor::handle_timeout();
            }
        }
        _other => {
            // If the trap came from user mode (RPL=3 in CS), forward
            // it to the kernel handler for Zircon exception delivery.
            // Only treat kernel-mode traps as fatal.
            if tf.cs & 3 == 3 {
                crate::KHANDLER.handle_user_trap(tf.trap_num, tf.error_code);
            } else {
                // Kernel mode: attempt bounded recovery (skipping MSR
                // accesses that faulted because the MSR doesn't exist
                // on this CPU) before declaring the trap fatal.
                if try_recover_kernel_gp(tf) {
                    return; // tf updated; trap.S resumes past the instruction
                }
                kernel_trap_fatal("unhandled kernel trap", tf);
            }
        }
    }
}
