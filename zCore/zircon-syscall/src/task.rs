use alloc::{string::ToString, vec::Vec};
use core::convert::TryFrom;
use hal_impl::context::UserContextField;
use {super::*, zircon_object::task::*};

impl Syscall<'_> {
    /// Create a new process.
    ///
    /// Upon success, handles for the new process and the root of its address space are returned.
    pub fn sys_process_create(
        &self,
        job: HandleValue,
        name: UserInPtr<u8>,
        name_size: usize,
        options: u32,
        mut proc_handle: UserOutPtr<HandleValue>,
        mut vmar_handle: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        // Fuchsia accepts arbitrarily long name buffers but truncates
        // at ZX_MAX_NAME_LEN (32 bytes including null terminator).
        // Read raw bytes and truncate safely at a UTF-8 boundary.
        let name_len = name_size.min(ZX_MAX_NAME_LEN);
        let raw = name.read_array(name_len)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let truncated = &raw[..end.min(ZX_MAX_NAME_LEN)];
        let name = core::str::from_utf8(truncated)
            .unwrap_or_else(|e| core::str::from_utf8(&truncated[..e.valid_up_to()]).unwrap_or(""))
            .to_string();
        info!(
            "proc.create: job={:#x?}, name={:?}, options={:#x?}",
            job, name, options,
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let job = proc
            .get_object_with_rights::<Job>(job, Rights::MANAGE_PROCESS)
            .or_else(|_| proc.get_object_with_rights::<Job>(job, Rights::WRITE))?;
        proc.check_policy(PolicyCondition::NewProcess)?;
        let new_proc = Process::create(&job, &name)?;
        let new_vmar = new_proc.vmar();
        let proc_handle_value = proc.add_handle(Handle::new(new_proc, Rights::DEFAULT_PROCESS));
        let vmar_handle_value = proc.add_handle(Handle::new(
            new_vmar,
            Rights::DEFAULT_VMAR | Rights::READ | Rights::WRITE | Rights::EXECUTE,
        ));
        proc_handle.write(proc_handle_value)?;
        vmar_handle.write(vmar_handle_value)?;
        Ok(())
    }

    /// Exits the currently running process.
    pub fn sys_process_exit(&mut self, code: i64) -> ZxResult {
        info!("proc.exit: code={:?}", code);
        let proc = self.thread.proc();
        proc.exit(code);
        Ok(())
    }

    /// Creates a thread within the specified process.
    ///
    /// Upon success a handle for the new thread is returned.
    pub fn sys_thread_create(
        &self,
        proc_handle: HandleValue,
        name: UserInPtr<u8>,
        name_size: usize,
        options: u32,
        mut thread_handle: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        let name_len = name_size.min(ZX_MAX_NAME_LEN);
        let raw = name.read_array(name_len)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let truncated = &raw[..end.min(ZX_MAX_NAME_LEN)];
        let name = core::str::from_utf8(truncated)
            .unwrap_or_else(|e| core::str::from_utf8(&truncated[..e.valid_up_to()]).unwrap_or(""))
            .to_string();
        info!(
            "thread.create: proc={:#x?}, name={:?}, options={:#x?}",
            proc_handle, name, options,
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let process = proc.get_object_with_rights::<Process>(proc_handle, Rights::MANAGE_THREAD)?;
        let thread = Thread::create(&process, &name)?;
        let handle = proc.add_handle(Handle::new(thread, Rights::DEFAULT_THREAD));
        thread_handle.write(handle)?;
        Ok(())
    }

    /// Start execution on a process.
    ///
    /// This system call is similar to `zx_thread_start()`, but is used for the purpose of starting the first thread in a process.
    pub fn sys_process_start(
        &self,
        proc_handle: HandleValue,
        thread_handle: HandleValue,
        entry: usize,
        stack: usize,
        arg1_handle: HandleValue,
        arg2: usize,
    ) -> ZxResult {
        info!("process.start: proc_handle={:?}, thread_handle={:?}, entry={:?}, stack={:?}, arg1_handle={:?}, arg2={:?}",
            proc_handle, thread_handle, entry, stack, arg1_handle, arg2
        );
        let proc = self.thread.proc();
        let process = proc.get_object_with_rights::<Process>(proc_handle, Rights::WRITE)?;
        let thread = proc.get_object_with_rights::<Thread>(thread_handle, Rights::WRITE)?;
        if !Arc::ptr_eq(thread.proc(), &process) {
            return Err(ZxError::ACCESS_DENIED);
        }
        // Consume arg1_handle (Fuchsia closes it on both success and failure).
        let arg1 = if arg1_handle != INVALID_HANDLE {
            let arg1 = proc.remove_handle(arg1_handle)?;
            if !arg1.rights.contains(Rights::TRANSFER) {
                return Err(ZxError::ACCESS_DENIED);
            }
            Some(arg1)
        } else {
            None
        };
        // Reject non-userspace entry points and stack pointers.
        // Validation is after handle consumption so arg1 is properly closed.
        if !is_user_address(entry) || !is_user_address(stack) {
            return Err(ZxError::INVALID_ARGS);
        }
        process.start(&thread, entry, stack, arg1, arg2, self.thread_fn)?;
        Ok(())
    }

    /// Read one aspect of thread state.
    ///
    /// The thread state may only be written when the thread is halted for an exception or the thread is suspended.
    pub fn sys_thread_read_state(
        &self,
        handle: HandleValue,
        kind: u32,
        mut buffer: UserOutPtr<u8>,
        buffer_size: usize,
    ) -> ZxResult {
        let kind = ThreadStateKind::try_from(kind).map_err(|_| ZxError::INVALID_ARGS)?;
        info!(
            "thread.read_state: handle={:#x?}, kind={:#x?}, buf=({:#x?}; {:#x?})",
            handle, kind, buffer, buffer_size,
        );
        let proc = self.thread.proc();
        let thread = proc.get_object_with_rights::<Thread>(handle, Rights::READ)?;
        //TODO: Remove allocation
        let mut buf = {
            let mut v = alloc::vec::Vec::new();
            v.try_reserve(buffer_size)
                .map_err(|_| ZxError::INVALID_ARGS)?;
            v.resize(buffer_size, 0u8);
            v
        };
        thread.read_state(kind, &mut buf)?;
        buffer.write_array(&buf[..])?;
        Ok(())
    }

    /// Write one aspect of thread state.
    ///
    /// The thread state may only be written when the thread is halted for an exception or the thread is suspended.
    pub fn sys_thread_write_state(
        &self,
        handle: HandleValue,
        kind: u32,
        buffer: UserInPtr<u8>,
        buffer_size: usize,
    ) -> ZxResult {
        let kind = ThreadStateKind::try_from(kind).map_err(|_| ZxError::INVALID_ARGS)?;
        info!(
            "thread.write_state: handle={:#x?}, kind={:#x?}, buf=({:#x?}; {:#x?})",
            handle, kind, buffer, buffer_size,
        );
        self.thread
            .proc()
            .get_object_with_rights::<Thread>(handle, Rights::WRITE)?
            .write_state(kind, &buffer.read_array(buffer_size)?)
    }

    /// Sets process as critical to job.
    ///
    /// When process terminates, job will be terminated as if `zx_task_kill()` was called on it.
    pub fn sys_job_set_critical(
        &self,
        job_handle: HandleValue,
        options: u32,
        process_handle: HandleValue,
    ) -> ZxResult {
        info!(
            "job.set_critical: job={:#x?}, options={:#x}, process={:#x?}",
            job_handle, options, process_handle,
        );
        let retcode_nonzero = if options == 1 {
            true
        } else if options == 0 {
            false
        } else {
            return Err(ZxError::INVALID_ARGS);
        };
        let proc = self.thread.proc();
        let job = proc.get_object_with_rights::<Job>(job_handle, Rights::DESTROY)?;
        let process = proc.get_object_with_rights::<Process>(process_handle, Rights::WAIT)?;
        process.set_critical_at_job(&job, retcode_nonzero)?;
        Ok(())
    }

    /// Start execution on a thread.
    pub fn sys_thread_start(
        &self,
        handle_value: HandleValue,
        entry: usize,
        stack: usize,
        arg1: usize,
        arg2: usize,
    ) -> ZxResult {
        info!(
            "thread.start: handle={:#x?}, entry={:#x}, stack={:#x}, arg1={:#x} arg2={:#x}",
            handle_value, entry, stack, arg1, arg2
        );
        let proc = self.thread.proc();
        let thread = proc.get_object_with_rights::<Thread>(handle_value, Rights::MANAGE_THREAD)?;
        if thread.proc().status() != Status::Running {
            return Err(ZxError::BAD_STATE);
        }
        thread.start_with_entry(entry, stack, arg1, arg2, self.thread_fn)?;
        Ok(())
    }

    /// Start execution on a thread, with explicit thread pointer and ABI register.
    ///
    /// This is the newer form of `zx_thread_start` (Fuchsia API level 31+).
    #[allow(clippy::too_many_arguments)]
    /// The extra `tp` argument sets the thread pointer (fsbase on x86_64,
    /// tpidr_el0 on aarch64) before the thread begins executing.
    /// `abi_reg` is reserved for ABI-specific use (e.g. shadow call stack on aarch64).
    pub fn sys_thread_start_regs(
        &self,
        handle_value: HandleValue,
        entry: usize,
        stack: usize,
        arg1: usize,
        arg2: usize,
        tp: usize,
        _abi_reg: usize,
    ) -> ZxResult {
        info!(
            "thread.start_regs: handle={:#x?}, entry={:#x}, stack={:#x}, arg1={:#x}, arg2={:#x}, tp={:#x}",
            handle_value, entry, stack, arg1, arg2, tp
        );
        let proc = self.thread.proc();
        let thread = proc.get_object_with_rights::<Thread>(handle_value, Rights::MANAGE_THREAD)?;
        if thread.proc().status() != Status::Running {
            return Err(ZxError::BAD_STATE);
        }
        // Set up entry, stack, and args, then set the thread pointer.
        thread.with_context(|ctx| {
            ctx.setup_uspace(entry, stack, &[arg1, arg2, 0]);
            if tp != 0 {
                ctx.set_field(UserContextField::ThreadPointer, tp);
            }
        })?;
        thread.start(self.thread_fn)?;
        Ok(())
    }

    /// Terminate the current running thread.
    ///
    /// Causes the currently running thread to cease running and exit.
    pub fn sys_thread_exit(&mut self) -> ZxResult {
        info!("thread.exit:");
        self.thread.exit();
        Ok(())
    }

    /// Suspend the given task.
    ///
    /// > This function replaces task_suspend. When all callers are updated, `zx_task_suspend()` will be deleted and this function will be renamed ```zx_task_suspend()```.
    pub fn sys_task_suspend_token(
        &self,
        handle: HandleValue,
        mut token: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!("task.suspend_token: handle={:?}, token={:?}", handle, token);
        let proc = self.thread.proc();
        if let Ok(thread) = proc.get_object_with_rights::<Thread>(handle, Rights::WRITE) {
            if Arc::ptr_eq(&thread, self.thread) {
                return Err(ZxError::NOT_SUPPORTED);
            }
            if thread.state() == ThreadState::Dying || thread.state() == ThreadState::Dead {
                return Err(ZxError::BAD_STATE);
            }
            let thread: Arc<dyn Task> = thread;
            let token_handle =
                Handle::new(SuspendToken::create(&thread), Rights::DEFAULT_SUSPEND_TOKEN);
            token.write(proc.add_handle(token_handle))?;
            return Ok(());
        }
        if let Ok(process) = proc.get_object_with_rights::<Process>(handle, Rights::WRITE) {
            let process: Arc<dyn Task> = process;
            let token_handle = Handle::new(
                SuspendToken::create(&process),
                Rights::DEFAULT_SUSPEND_TOKEN,
            );
            token.write(proc.add_handle(token_handle))?;
            return Ok(());
        }
        Err(ZxError::WRONG_TYPE)
    }

    /// Kill the provided task (job, process, or thread).
    pub fn sys_task_kill(&mut self, handle: HandleValue) -> ZxResult {
        info!("task.kill: handle={:?}", handle);
        let proc = self.thread.proc();

        if let Ok(job) = proc.get_object_with_rights::<Job>(handle, Rights::DESTROY) {
            job.kill();
        } else if let Ok(process) = proc.get_object_with_rights::<Process>(handle, Rights::DESTROY)
        {
            process.kill();
        } else if let Ok(thread) = proc.get_object_with_rights::<Thread>(handle, Rights::DESTROY) {
            thread.kill();
        } else {
            return Err(ZxError::WRONG_TYPE);
        }
        Ok(())
    }

    /// Create a new child job object given a parent job.
    pub fn sys_job_create(
        &self,
        parent: HandleValue,
        options: u32,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "job.create: parent={:#x}, options={:#x}, out={:#x?}",
            parent, options, out
        );
        if options != 0 {
            Err(ZxError::INVALID_ARGS)
        } else {
            let proc = self.thread.proc();
            let parent_job = proc
                .get_object_with_rights::<Job>(parent, Rights::MANAGE_JOB)
                .or_else(|_| proc.get_object_with_rights::<Job>(parent, Rights::WRITE))?;
            let child = parent_job.create_child()?;
            out.write(proc.add_handle(Handle::new(child, Rights::DEFAULT_JOB)))?;
            Ok(())
        }
    }

    /// Sets one or more security and/or resource policies to an empty job.
    pub fn sys_job_set_policy(
        &self,
        handle: HandleValue,
        options: u32,
        topic: u32,
        policy: usize,
        count: u32,
    ) -> ZxResult {
        info!(
            "job.set_policy: handle={:#x}, options={:#x}, topic={:#x}, policy={:#x?}, count={:#x}",
            handle, options, topic, policy, count,
        );
        let proc = self.thread.proc();
        let job = proc.get_object_with_rights::<Job>(handle, Rights::SET_POLICY)?;
        match topic {
            JOB_POL_BASE_V1 => {
                let policy_option = match options {
                    JOB_POL_RELATIVE => SetPolicyOptions::Relative,
                    JOB_POL_ABSOLUTE => SetPolicyOptions::Absolute,
                    _ => return Err(ZxError::INVALID_ARGS),
                };
                // Read raw 8-byte records and validate discriminants.
                let raw_policies: Vec<BasicPolicyRaw> =
                    UserInPtr::from(policy).read_array(count as usize)?;
                let mut v1_policies = Vec::with_capacity(raw_policies.len());
                for raw in &raw_policies {
                    v1_policies.push(raw.validate()?);
                }
                job.set_policy_basic(policy_option, &v1_policies)
            }
            JOB_POL_BASE_V2 => {
                let policy_option = match options {
                    JOB_POL_RELATIVE => SetPolicyOptions::Relative,
                    JOB_POL_ABSOLUTE => SetPolicyOptions::Absolute,
                    _ => return Err(ZxError::INVALID_ARGS),
                };
                if count == 0 {
                    return Err(ZxError::INVALID_ARGS);
                }
                // Read raw 12-byte records and validate discriminants.
                let raw_policies: Vec<BasicPolicyV2Raw> =
                    UserInPtr::from(policy).read_array(count as usize)?;
                let mut v2_policies = Vec::with_capacity(raw_policies.len());
                for raw in &raw_policies {
                    v2_policies.push(raw.validate()?);
                }
                job.set_policy_basic_v2(policy_option, &v2_policies)
            }
            JOB_POL_TIMER_SLACK => {
                if options != JOB_POL_RELATIVE {
                    return Err(ZxError::INVALID_ARGS);
                }
                if count != 1 {
                    return Err(ZxError::INVALID_ARGS);
                }
                let timer_policy = UserInPtr::<TimerSlackPolicy>::from(policy).read()?;
                job.set_policy_timer_slack(timer_policy)
            }
            _ => Err(ZxError::INVALID_ARGS),
        }
    }

    /// Read from the given process's address space.
    ///
    /// > This function will eventually be replaced with something vmo-centric.
    pub fn sys_process_read_memory(
        &self,
        handle_value: HandleValue,
        vaddr: usize,
        mut buffer: UserOutPtr<u8>,
        buffer_size: usize,
        mut actual: UserOutPtr<usize>,
    ) -> ZxResult {
        if buffer.is_null() || buffer_size == 0 || buffer_size > MAX_BLOCK {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let process =
            proc.get_object_with_rights::<Process>(handle_value, Rights::READ | Rights::WRITE)?;
        let mut data = {
            let mut v = alloc::vec::Vec::new();
            v.try_reserve(buffer_size)
                .map_err(|_| ZxError::INVALID_ARGS)?;
            v.resize(buffer_size, 0u8);
            v
        };
        let len = process.vmar().read_memory(vaddr, &mut data)?;
        buffer.write_array(&data[..len])?;
        actual.write(len)?;
        Ok(())
    }

    /// Write into the given process's address space.
    pub fn sys_process_write_memory(
        &self,
        handle_value: HandleValue,
        vaddr: usize,
        buffer: UserInPtr<u8>,
        buffer_size: usize,
        mut actual: UserOutPtr<usize>,
    ) -> ZxResult {
        if buffer.is_null() || buffer_size == 0 || buffer_size > MAX_BLOCK {
            Err(ZxError::INVALID_ARGS)
        } else {
            let len = self
                .thread
                .proc()
                .get_object_with_rights::<Process>(handle_value, Rights::READ | Rights::WRITE)?
                .vmar()
                .write_memory(vaddr, &buffer.read_array(buffer_size)?)?;
            actual.write(len)?;
            Ok(())
        }
    }

    /// Create a scheduling/affinity profile.
    ///
    /// The `resource` handle must be the root resource.
    /// `options` must be 0.
    pub fn sys_profile_create(
        &self,
        resource: HandleValue,
        options: u32,
        profile_info: UserInPtr<ProfileInfo>,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "profile.create: resource={:#x}, options={}",
            resource, options,
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        // Validate: accept ROOT or SYSTEM resource with PROFILE_BASE
        let rsrc = proc.get_resource_with_rights(resource, Rights::empty())?;
        if rsrc.validate(zircon_object::dev::ResourceKind::ROOT).is_err() {
            rsrc.validate_ranged_resource(
                zircon_object::dev::ResourceKind::SYSTEM,
                zircon_object::dev::ZX_RSRC_SYSTEM_PROFILE_BASE,
                1,
            )?;
        }
        // Check job policy
        proc.check_policy(PolicyCondition::NewProfile)?;
        // Read and validate profile info
        let info = profile_info.read()?;
        let profile = Profile::create(info)?;
        let handle = proc.add_handle(Handle::new(profile, Rights::DEFAULT_PROFILE));
        out.write(handle)?;
        Ok(())
    }

    /// Apply a profile to a thread.
    ///
    /// Currently a stub: validates handles and rights but does not
    /// change the thread's scheduling parameters (the kernel does
    /// not yet support runtime priority changes).
    pub fn sys_object_set_profile(
        &self,
        target: HandleValue,
        profile: HandleValue,
        options: u32,
    ) -> ZxResult {
        info!(
            "object.set_profile: target={:#x}, profile={:#x}, options={}",
            target, profile, options,
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        // Validate target is a thread with MANAGE_THREAD
        let _thread = proc.get_object_with_rights::<Thread>(target, Rights::MANAGE_THREAD)?;
        // Validate profile handle with APPLY_PROFILE
        let _profile = proc.get_object_with_rights::<Profile>(profile, Rights::APPLY_PROFILE)?;
        // TODO: actually apply scheduling parameters to the thread.
        // For now, accept the call without error (the profile is valid
        // but runtime priority changes are not implemented).
        Ok(())
    }

    /// Programmatically raise an exception on the calling thread.
    ///
    /// The exception is delivered through the thread's exception channel
    /// using the standard exception handler chain (process debugger →
    /// thread → process → job chain). The calling thread blocks until
    /// the exception is handled or the thread is killed.
    ///
    /// `options` must be `ZX_EXCEPTION_TARGET_JOB_DEBUGGER` (1).
    /// `excp_type` must be `ZX_EXCP_USER` (0x8309).
    pub async fn sys_thread_raise_exception(
        &self,
        options: u32,
        excp_type: u32,
        context_ptr: usize,
    ) -> ZxResult {
        info!(
            "thread.raise_exception: options={}, type={:#x}, context={:#x}",
            options, excp_type, context_ptr
        );
        if options != 1 {
            return Err(ZxError::INVALID_ARGS);
        }
        if excp_type != 0x8309 {
            return Err(ZxError::INVALID_ARGS);
        }
        // The context pointer must be non-null and point to a valid
        // zx_exception_context_t. Layout: arch (24 bytes) + synth_code
        // (u32) + synth_data (u32) = 32 bytes total on all arches.
        if context_ptr == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        const ARCH_SIZE: usize = 24;
        const CTX_SIZE: usize = ARCH_SIZE + 8; // + synth_code + synth_data
        let ctx_buf: UserInPtr<u8> = context_ptr.into();
        let context_data = ctx_buf.read_array(CTX_SIZE)?;
        // synth_code and synth_data are after the arch-specific payload.
        let synth_code =
            u32::from_ne_bytes(context_data[ARCH_SIZE..ARCH_SIZE + 4].try_into().unwrap());
        let synth_data = u32::from_ne_bytes(
            context_data[ARCH_SIZE + 4..ARCH_SIZE + 8]
                .try_into()
                .unwrap(),
        );
        // Deliver a User exception with synth fields through the
        // exception handler chain. Blocks until handled.
        self.thread
            .handle_exception_user(synth_code, synth_data)
            .await;
        Ok(())
    }

    /// Register a restartable sequence (rseq) area for the calling thread.
    ///
    /// The VMO contains the `zx_rseq_t` structure. Pass `ZX_HANDLE_INVALID`
    /// to unregister. Offset and size specify the region within the VMO.
    pub fn sys_thread_set_rseq(&self, vmo_handle: HandleValue, offset: u64, size: u64) -> ZxResult {
        info!(
            "thread.set_rseq: vmo={:#x}, offset={:#x}, size={:#x}",
            vmo_handle, offset, size
        );
        if vmo_handle == INVALID_HANDLE {
            self.thread.clear_rseq();
            return Ok(());
        }
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<zircon_object::vm::VmObject>(
            vmo_handle,
            Rights::READ | Rights::WRITE | Rights::DUPLICATE,
        )?;
        // zx_rseq_t is 32 bytes (4 fields * 8 bytes each).
        const RSEQ_STRUCT_SIZE: u64 = 32;
        if size != RSEQ_STRUCT_SIZE {
            return Err(ZxError::INVALID_ARGS);
        }
        // Offset must be aligned to 8 bytes (natural alignment of u64 fields).
        if !offset.is_multiple_of(8) {
            return Err(ZxError::INVALID_ARGS);
        }
        // Check range without overflow.
        let end = offset.checked_add(size).ok_or(ZxError::OUT_OF_RANGE)?;
        if end as usize > vmo.len() {
            return Err(ZxError::OUT_OF_RANGE);
        }
        self.thread.set_rseq(vmo, offset)?;
        Ok(())
    }

    /// Create a process that shares its address space with an existing process.
    ///
    /// The new process uses the same VMAR root as `shared_proc`, enabling
    /// shared memory without explicit mapping. Used by Starnix for lightweight
    /// Linux process creation.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_process_create_shared(
        &self,
        shared_proc: HandleValue,
        options: u32,
        name: UserInPtr<u8>,
        name_size: usize,
        mut proc_handle: UserOutPtr<HandleValue>,
        mut restricted_vmar_handle: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "process.create_shared: shared_proc={:#x}, options={}",
            shared_proc, options
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        if name_size > 32 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let source = proc.get_object_with_rights::<Process>(
            shared_proc,
            Rights::MANAGE_PROCESS | Rights::GET_PROPERTY,
        )?;
        // Source must itself be a shared process (created via
        // create_shared or with ZX_PROCESS_SHARED).
        if !source.is_shared() {
            return Err(ZxError::INVALID_ARGS);
        }
        let name_str = name.read_string(name_size)?;

        let (new_proc, restricted_vmar) = Process::create_shared(&source, &name_str)?;

        let proc_hv = proc.add_handle(Handle::new(new_proc, Rights::DEFAULT_PROCESS));
        let vmar_hv = proc.add_handle(Handle::new(restricted_vmar, Rights::DEFAULT_VMAR));

        if proc_handle.write(proc_hv).is_err() {
            proc.remove_handle(proc_hv).ok();
            proc.remove_handle(vmar_hv).ok();
            return Err(ZxError::INVALID_ARGS);
        }
        if restricted_vmar_handle.write(vmar_hv).is_err() {
            proc.remove_handle(proc_hv).ok();
            proc.remove_handle(vmar_hv).ok();
            return Err(ZxError::INVALID_ARGS);
        }
        Ok(())
    }
}

/// Check if an address is in the user-space range (canonical lower half).
/// Fuchsia rejects addresses in the kernel half for process_start entry/stack.
fn is_user_address(addr: usize) -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        addr < 0x0000_8000_0000_0000
    }
    #[cfg(target_arch = "aarch64")]
    {
        addr < 0x0001_0000_0000_0000
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = addr;
        true // permissive on other architectures
    }
}

const JOB_POL_BASE_V1: u32 = 0;
const JOB_POL_BASE_V2: u32 = 0x0100_0000;
const JOB_POL_TIMER_SLACK: u32 = 1;

const JOB_POL_RELATIVE: u32 = 0;
const JOB_POL_ABSOLUTE: u32 = 1;

const MAX_BLOCK: usize = 64 * 1024 * 1024; //64M

/// Maximum length of an object name in Fuchsia (ZX_MAX_NAME_LEN).
/// Includes the null terminator, so the usable string is 31 bytes.
const ZX_MAX_NAME_LEN: usize = 32;
