use super::*;
#[cfg(not(any(target_arch = "riscv64", target_arch = "riscv32")))]
use hal::TrapReason;
#[cfg(not(any(target_arch = "riscv64", target_arch = "riscv32")))]
use hal_impl::context::UserContext;
use hal_impl::context::UserContextField;
use zircon_object::task::Thread;
use zircon_object::vm::VmObject;

impl Syscall<'_> {
    /// Bind a restricted mode state VMO to the calling thread.
    ///
    /// Creates a VMO to hold the `zx_restricted_state_t` register state
    /// and binds it to the calling thread. The VMO handle is returned
    /// so the caller can read/write register state before calling
    /// `restricted_enter`.
    ///
    /// Options must be 0.
    pub fn sys_restricted_bind_state(
        &self,
        options: u32,
        mut out_vmo: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!("restricted.bind_state: options={}", options);
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }

        // Create a 1-page VMO for the restricted state.
        let vmo = VmObject::new_paged(1);
        vmo.set_name("restricted-state");

        // Bind to the current thread.
        self.thread.restricted_bind_state(vmo.clone())?;

        // Return a handle to the VMO so the caller can map/read/write it.
        let proc = self.thread.proc();
        let handle = proc.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));
        out_vmo.write(handle)?;
        Ok(())
    }

    /// Unbind the restricted mode state VMO from the calling thread.
    ///
    /// Options must be 0. Not an error if nothing is bound.
    pub fn sys_restricted_unbind_state(&self, options: u32) -> ZxResult {
        info!("restricted.unbind_state: options={}", options);
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        self.thread.restricted_unbind_state();
        Ok(())
    }

    /// Enter restricted execution mode.
    ///
    /// Loads register state from the bound VMO, transitions to EL0/Ring3,
    /// and runs until a trap occurs (syscall, exception, or kick). On
    /// exit, saves the restricted register state back to the VMO and
    /// sets up the normal thread context to resume at `vector_table_ptr`
    /// with `(context, reason)` as arguments.
    ///
    /// This syscall does not return normally — on success, control
    /// resumes at `vector_table_ptr` in normal mode.
    pub fn sys_restricted_enter(
        &self,
        options: u32,
        vector_table_ptr: usize,
        context: usize,
    ) -> ZxResult {
        info!(
            "restricted.enter: options={}, vector={:#x}, context={:#x}",
            options, vector_table_ptr, context
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        if !self.thread.has_restricted_state() {
            return Err(ZxError::BAD_STATE);
        }

        // Check for pending kick before entering.
        if self.thread.check_restricted_kick() {
            self.setup_normal_return(vector_table_ptr, context, ZX_RESTRICTED_REASON_KICK)?;
            return Ok(());
        }

        // RISC-V restricted mode is not yet fully implemented
        // (only PC+SP are loaded/saved, other GPRs would be lost).
        #[cfg(any(target_arch = "riscv64", target_arch = "riscv32"))]
        {
            warn!("restricted.enter: riscv not yet fully implemented");
            Err(ZxError::NOT_SUPPORTED)
        }

        #[cfg(not(any(target_arch = "riscv64", target_arch = "riscv32")))]
        {
            use zircon_object::task::ThreadState;

            let vmo = self
                .thread
                .restricted_state_vmo()
                .ok_or(ZxError::BAD_STATE)?;
            let mut restricted_ctx = UserContext::new();
            Self::load_restricted_state(&vmo, &mut restricted_ctx)?;

            // Copy vector/FPU state from the normal context so restricted
            // code inherits valid FPU settings (x86_64 only).
            #[cfg(target_arch = "x86_64")]
            {
                if let Ok(()) = self.thread.with_context(|normal_ctx| {
                    restricted_ctx.vector_regs = normal_ctx.vector_regs;
                }) {}
            }

            // Enter restricted userspace in a loop. Interrupts and
            // page faults are handled transparently and we re-enter.
            // Only syscalls, unhandled exceptions, and kicks cause
            // an exit back to normal mode.
            let exit_reason = loop {
                // Check for pending kick, kill, or suspend before (re-)entering.
                if self.thread.check_restricted_kick() {
                    break ZX_RESTRICTED_REASON_KICK;
                }
                if self.thread.state() == ThreadState::Dying
                    || self.thread.state() == ThreadState::Dead
                {
                    break ZX_RESTRICTED_REASON_KICK;
                }

                restricted_ctx.enter_uspace();

                let reason = restricted_ctx.trap_reason();
                match reason {
                    TrapReason::Syscall => break ZX_RESTRICTED_REASON_SYSCALL,
                    TrapReason::Interrupt(vector) => {
                        // Handle the interrupt, yield to let other tasks
                        // run, then re-enter restricted mode.
                        hal_impl::interrupt::handle_irq(vector);
                        // Yield on timer interrupts to prevent starving
                        // other tasks on the same CPU.
                        if vector == hal_impl::context::TIMER_INTERRUPT_VEC {
                            core::hint::spin_loop();
                        }
                        continue;
                    }
                    TrapReason::PageFault(vaddr, flags) => {
                        // Try demand-paging. If the fault can't be resolved,
                        // exit with EXCEPTION.
                        let thread_arc = self.thread.inner();
                        let vmar = thread_arc.proc().vmar();
                        if vmar.handle_page_fault(vaddr, flags).is_err() {
                            break ZX_RESTRICTED_REASON_EXCEPTION;
                        }
                        continue;
                    }
                    _ => break ZX_RESTRICTED_REASON_EXCEPTION,
                }
            };

            // Save the restricted register state back to the VMO.
            Self::save_restricted_state(&vmo, &mut restricted_ctx)?;

            // Copy vector/FPU state back to normal context (x86_64 only).
            #[cfg(target_arch = "x86_64")]
            {
                let vr = restricted_ctx.vector_regs;
                let _ = self.thread.with_context(|normal_ctx| {
                    normal_ctx.vector_regs = vr;
                });
            }

            // Set up normal context to resume at vector_table_ptr.
            self.setup_normal_return(vector_table_ptr, context, exit_reason)?;

            Ok(())
        }
    }

    /// Set up the thread's normal context to resume at vector_table_ptr
    /// with (context, reason) as the first two arguments.
    ///
    /// On aarch64/riscv, the return value register IS the first arg
    /// register (x0/a0), so `context` is delivered via the syscall
    /// return value in the dispatcher. On x86_64, the return value
    /// goes to rax but the first SysV arg is rdi, so we must set
    /// rdi explicitly here.
    fn setup_normal_return(
        &self,
        vector_table_ptr: usize,
        #[allow(unused_variables)] context: usize,
        reason: u64,
    ) -> ZxResult {
        self.thread.with_context(|ctx| {
            ctx.set_field(UserContextField::InstrPointer, vector_table_ptr);
            cfg_if::cfg_if! {
                if #[cfg(target_arch = "aarch64")] {
                    // x0 = context (set by dispatcher return value)
                    ctx.general_mut().x1 = reason as usize;
                } else if #[cfg(target_arch = "x86_64")] {
                    // rax = context (set by dispatcher return value)
                    // but first SysV arg is rdi, so set it explicitly.
                    ctx.general_mut().rdi = context;
                    ctx.general_mut().rsi = reason as usize;
                } else if #[cfg(any(target_arch = "riscv64", target_arch = "riscv32"))] {
                    // a0 = context (set by dispatcher return value)
                    ctx.general_mut().a1 = reason as usize;
                }
            }
        })
    }

    /// Load restricted register state from a VMO into a UserContext.
    ///
    /// The VMO contains `zx_restricted_state_t` at offset 0.
    /// Layout is architecture-specific.
    #[cfg(not(any(target_arch = "riscv64", target_arch = "riscv32")))]
    fn load_restricted_state(vmo: &VmObject, ctx: &mut UserContext) -> ZxResult {
        cfg_if::cfg_if! {
            if #[cfg(target_arch = "aarch64")] {
                // zx_restricted_state_t for aarch64:
                // x[0..31]: 31 GPRs (248 bytes)
                // sp: 8 bytes
                // pc: 8 bytes
                // cpsr: 8 bytes
                // tpidr_el0: 8 bytes
                // Total: 280 bytes
                let mut buf = [0u8; 280];
                vmo.read(0, &mut buf)?;
                let regs = ctx.general_mut();
                // x0..x30 (x0 is at the end of GeneralRegs in trapframe)
                for i in 0..31 {
                    let val = u64::from_le_bytes(buf[i * 8..(i + 1) * 8].try_into().unwrap());
                    match i {
                        0 => regs.x0 = val as usize,
                        1 => regs.x1 = val as usize,
                        2 => regs.x2 = val as usize,
                        3 => regs.x3 = val as usize,
                        4 => regs.x4 = val as usize,
                        5 => regs.x5 = val as usize,
                        6 => regs.x6 = val as usize,
                        7 => regs.x7 = val as usize,
                        8 => regs.x8 = val as usize,
                        9 => regs.x9 = val as usize,
                        10 => regs.x10 = val as usize,
                        11 => regs.x11 = val as usize,
                        12 => regs.x12 = val as usize,
                        13 => regs.x13 = val as usize,
                        14 => regs.x14 = val as usize,
                        15 => regs.x15 = val as usize,
                        16 => regs.x16 = val as usize,
                        17 => regs.x17 = val as usize,
                        18 => regs.x18 = val as usize,
                        19 => regs.x19 = val as usize,
                        20 => regs.x20 = val as usize,
                        21 => regs.x21 = val as usize,
                        22 => regs.x22 = val as usize,
                        23 => regs.x23 = val as usize,
                        24 => regs.x24 = val as usize,
                        25 => regs.x25 = val as usize,
                        26 => regs.x26 = val as usize,
                        27 => regs.x27 = val as usize,
                        28 => regs.x28 = val as usize,
                        29 => regs.x29 = val as usize,
                        30 => regs.x30 = val as usize,
                        _ => {}
                    }
                }
                // Upstream field order after x[31]: sp, pc, tpidr_el0, cpsr
                let sp = u64::from_le_bytes(buf[248..256].try_into().unwrap());
                let pc = u64::from_le_bytes(buf[256..264].try_into().unwrap());
                let tpidr = u64::from_le_bytes(buf[264..272].try_into().unwrap());
                let cpsr = u64::from_le_bytes(buf[272..280].try_into().unwrap());
                ctx.set_field(UserContextField::StackPointer, sp as usize);
                ctx.set_field(UserContextField::InstrPointer, pc as usize);
                ctx.set_field(UserContextField::ThreadPointer, tpidr as usize);
                // Force EL0 execution: SPSR_EL1 with M[3:0] = 0 (EL0t)
                // Keep the condition flags from the VMO but mask the mode bits.
                let spsr = (cpsr & 0xF000_0000) | (1 << 8); // NZCV flags + mask SError
                ctx.set_spsr(spsr as usize);
            } else if #[cfg(target_arch = "x86_64")] {
                // zx_restricted_state_t for x86_64 (upstream field order):
                // rdi, rsi, rbp, rbx, rdx, rcx, rax, rsp,
                // r8-r15, ip, flags, fs_base, gs_base
                // Total: 160 bytes
                let mut buf = [0u8; 160];
                vmo.read(0, &mut buf)?;
                let regs = ctx.general_mut();
                regs.rdi = u64::from_le_bytes(buf[0..8].try_into().unwrap()) as usize;
                regs.rsi = u64::from_le_bytes(buf[8..16].try_into().unwrap()) as usize;
                regs.rbp = u64::from_le_bytes(buf[16..24].try_into().unwrap()) as usize;
                regs.rbx = u64::from_le_bytes(buf[24..32].try_into().unwrap()) as usize;
                regs.rdx = u64::from_le_bytes(buf[32..40].try_into().unwrap()) as usize;
                regs.rcx = u64::from_le_bytes(buf[40..48].try_into().unwrap()) as usize;
                regs.rax = u64::from_le_bytes(buf[48..56].try_into().unwrap()) as usize;
                regs.rsp = u64::from_le_bytes(buf[56..64].try_into().unwrap()) as usize;
                regs.r8 = u64::from_le_bytes(buf[64..72].try_into().unwrap()) as usize;
                regs.r9 = u64::from_le_bytes(buf[72..80].try_into().unwrap()) as usize;
                regs.r10 = u64::from_le_bytes(buf[80..88].try_into().unwrap()) as usize;
                regs.r11 = u64::from_le_bytes(buf[88..96].try_into().unwrap()) as usize;
                regs.r12 = u64::from_le_bytes(buf[96..104].try_into().unwrap()) as usize;
                regs.r13 = u64::from_le_bytes(buf[104..112].try_into().unwrap()) as usize;
                regs.r14 = u64::from_le_bytes(buf[112..120].try_into().unwrap()) as usize;
                regs.r15 = u64::from_le_bytes(buf[120..128].try_into().unwrap()) as usize;
                regs.rip = u64::from_le_bytes(buf[128..136].try_into().unwrap()) as usize;
                regs.rflags = u64::from_le_bytes(buf[136..144].try_into().unwrap()) as usize;
                regs.fsbase = u64::from_le_bytes(buf[144..152].try_into().unwrap()) as usize;
                regs.gsbase = u64::from_le_bytes(buf[152..160].try_into().unwrap()) as usize;
            } else if #[cfg(any(target_arch = "riscv64", target_arch = "riscv32"))] {
                // Minimal: PC + SP + 32 GPRs = 34 * 8 = 272 bytes
                let mut buf = [0u8; 272];
                vmo.read(0, &mut buf)?;
                let pc = u64::from_le_bytes(buf[0..8].try_into().unwrap());
                ctx.set_field(UserContextField::InstrPointer, pc as usize);
                let sp = u64::from_le_bytes(buf[8..16].try_into().unwrap());
                ctx.set_field(UserContextField::StackPointer, sp as usize);
                // TODO: load remaining GPRs for riscv
            }
        }
        Ok(())
    }

    /// Save restricted register state from a UserContext back to the VMO.
    #[cfg(not(any(target_arch = "riscv64", target_arch = "riscv32")))]
    fn save_restricted_state(vmo: &VmObject, ctx: &mut UserContext) -> ZxResult {
        cfg_if::cfg_if! {
            if #[cfg(target_arch = "aarch64")] {
                let mut buf = [0u8; 280];
                let regs = ctx.general();
                let xs: [usize; 31] = [
                    regs.x0, regs.x1, regs.x2, regs.x3, regs.x4, regs.x5,
                    regs.x6, regs.x7, regs.x8, regs.x9, regs.x10, regs.x11,
                    regs.x12, regs.x13, regs.x14, regs.x15, regs.x16, regs.x17,
                    regs.x18, regs.x19, regs.x20, regs.x21, regs.x22, regs.x23,
                    regs.x24, regs.x25, regs.x26, regs.x27, regs.x28, regs.x29,
                    regs.x30,
                ];
                for (i, &val) in xs.iter().enumerate() {
                    buf[i * 8..(i + 1) * 8].copy_from_slice(&(val as u64).to_le_bytes());
                }
                let sp = ctx.get_field(UserContextField::StackPointer);
                let pc = ctx.get_field(UserContextField::InstrPointer);
                let tpidr = ctx.get_field(UserContextField::ThreadPointer);
                let cpsr = ctx.get_spsr();
                // Upstream field order: sp, pc, tpidr_el0, cpsr
                buf[248..256].copy_from_slice(&(sp as u64).to_le_bytes());
                buf[256..264].copy_from_slice(&(pc as u64).to_le_bytes());
                buf[264..272].copy_from_slice(&(tpidr as u64).to_le_bytes());
                buf[272..280].copy_from_slice(&(cpsr as u64).to_le_bytes());
                vmo.write(0, &buf)?;
            } else if #[cfg(target_arch = "x86_64")] {
                // Upstream field order: rdi, rsi, rbp, rbx, rdx, rcx, rax, rsp
                let mut buf = [0u8; 160];
                let regs = ctx.general();
                buf[0..8].copy_from_slice(&(regs.rdi as u64).to_le_bytes());
                buf[8..16].copy_from_slice(&(regs.rsi as u64).to_le_bytes());
                buf[16..24].copy_from_slice(&(regs.rbp as u64).to_le_bytes());
                buf[24..32].copy_from_slice(&(regs.rbx as u64).to_le_bytes());
                buf[32..40].copy_from_slice(&(regs.rdx as u64).to_le_bytes());
                buf[40..48].copy_from_slice(&(regs.rcx as u64).to_le_bytes());
                buf[48..56].copy_from_slice(&(regs.rax as u64).to_le_bytes());
                buf[56..64].copy_from_slice(&(regs.rsp as u64).to_le_bytes());
                buf[64..72].copy_from_slice(&(regs.r8 as u64).to_le_bytes());
                buf[72..80].copy_from_slice(&(regs.r9 as u64).to_le_bytes());
                buf[80..88].copy_from_slice(&(regs.r10 as u64).to_le_bytes());
                buf[88..96].copy_from_slice(&(regs.r11 as u64).to_le_bytes());
                buf[96..104].copy_from_slice(&(regs.r12 as u64).to_le_bytes());
                buf[104..112].copy_from_slice(&(regs.r13 as u64).to_le_bytes());
                buf[112..120].copy_from_slice(&(regs.r14 as u64).to_le_bytes());
                buf[120..128].copy_from_slice(&(regs.r15 as u64).to_le_bytes());
                buf[128..136].copy_from_slice(&(regs.rip as u64).to_le_bytes());
                buf[136..144].copy_from_slice(&(regs.rflags as u64).to_le_bytes());
                buf[144..152].copy_from_slice(&(regs.fsbase as u64).to_le_bytes());
                buf[152..160].copy_from_slice(&(regs.gsbase as u64).to_le_bytes());
                vmo.write(0, &buf)?;
            } else if #[cfg(any(target_arch = "riscv64", target_arch = "riscv32"))] {
                let mut buf = [0u8; 272];
                let pc = ctx.get_field(UserContextField::InstrPointer);
                let sp = ctx.get_field(UserContextField::StackPointer);
                buf[0..8].copy_from_slice(&(pc as u64).to_le_bytes());
                buf[8..16].copy_from_slice(&(sp as u64).to_le_bytes());
                // TODO: save remaining GPRs for riscv
                vmo.write(0, &buf)?;
            }
        }
        Ok(())
    }

    /// Force a thread out of restricted mode.
    ///
    /// The target thread will exit restricted mode with reason
    /// `ZX_RESTRICTED_REASON_KICK`. If the thread is not currently
    /// in restricted mode, the kick is pended for the next enter.
    ///
    /// Options must be 0. Requires MANAGE_THREAD right on the handle.
    pub fn sys_restricted_kick(&self, handle: HandleValue, options: u32) -> ZxResult {
        info!("restricted.kick: handle={:#x}, options={}", handle, options);
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let thread = proc.get_object_with_rights::<Thread>(handle, Rights::MANAGE_THREAD)?;
        thread.restricted_kick()
        // The kick flag is checked on restricted_enter and after
        // enter_uspace returns (on the next timer interrupt or syscall).
        // Full IPI-based immediate kick requires per-CPU thread tracking
        // which will be added in a follow-up.
    }
}

// Restricted mode exit reason codes.
#[cfg(not(any(target_arch = "riscv64", target_arch = "riscv32")))]
const ZX_RESTRICTED_REASON_SYSCALL: u64 = 0;
#[cfg(not(any(target_arch = "riscv64", target_arch = "riscv32")))]
const ZX_RESTRICTED_REASON_EXCEPTION: u64 = 1;
const ZX_RESTRICTED_REASON_KICK: u64 = 2;
