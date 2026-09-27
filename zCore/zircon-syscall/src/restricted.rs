use super::*;
use zircon_object::task::Thread;
use zircon_object::vm::VmObject;

/// Size of `zx_restricted_state_t` — architecture-dependent.
/// For aarch64: 31 GPRs + SP + PC + CPSR + TPIDR_EL0 = 35 * 8 = 280 bytes
/// For x86_64: 16 GPRs + RIP + RFLAGS + FS_BASE + GS_BASE = 20 * 8 = 160 bytes
/// We use the larger of the two as the VMO size to be safe.
const RESTRICTED_STATE_VMO_SIZE: usize = 280;

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

        // Create a VMO for the restricted state.
        let pages = RESTRICTED_STATE_VMO_SIZE.div_ceil(4096);
        let vmo = VmObject::new_paged(pages);
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
    /// Loads register state from the bound VMO and begins execution in
    /// restricted mode. On exit (syscall, exception, or kick), the
    /// register state is saved back to the VMO and execution resumes
    /// at `vector_table_ptr` with the exit reason.
    ///
    /// Options must be 0.
    pub fn sys_restricted_enter(
        &self,
        options: u32,
        _vector_table_ptr: usize,
        _context: usize,
    ) -> ZxResult {
        info!("restricted.enter: options={}", options);
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        if !self.thread.has_restricted_state() {
            return Err(ZxError::BAD_STATE);
        }
        // Check for pending kick before entering.
        if self.thread.check_restricted_kick() {
            // Would normally jump to vector_table_ptr with KICK reason.
            // For now, return BAD_STATE since we can't actually enter.
            return Err(ZxError::BAD_STATE);
        }
        // TODO: implement arch-specific context switch into restricted mode.
        // This requires:
        // 1. Loading register state from the bound VMO
        // 2. Switching to EL0/Ring3 restricted execution
        // 3. On exit (syscall/exception/kick), saving state back to VMO
        // 4. Jumping to vector_table_ptr with (context, reason)
        warn!("restricted.enter: validated but context switch not implemented");
        Err(ZxError::NOT_SUPPORTED)
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
    }
}
