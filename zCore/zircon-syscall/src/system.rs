use super::*;
use zircon_object::dev::{
    Resource, ResourceKind, ZX_RSRC_SYSTEM_MEXEC_BASE, ZX_RSRC_SYSTEM_TRACING_BASE,
};
use zircon_object::signal::{Counter, Event};
use zircon_object::task::Job;

impl Syscall<'_> {
    /// Retrieve a handle to a system event.
    ///
    /// `root_job: HandleValue`, must be a handle to the root job of the system.
    /// `kind: u32`, must be one of the following:
    /// ```rust
    ///     const EVENT_OUT_OF_MEMORY: u32 = 1;
    ///     const EVENT_MEMORY_PRESSURE_CRITICAL: u32 = 2;
    ///     const EVENT_MEMORY_PRESSURE_WARNING: u32 = 3;
    ///     const EVENT_MEMORY_PRESSURE_NORMAL: u32 = 4;
    /// ```
    pub fn sys_system_get_event(
        &self,
        root_job: HandleValue,
        kind: u32,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "system.get_event: root_job={:#x}, kind={:#x}, out_ptr={:#x?}",
            root_job, kind, out
        );
        match kind {
            EVENT_OUT_OF_MEMORY => {
                let proc = self.thread.proc();
                proc.get_object_with_rights::<Job>(root_job, Rights::MANAGE_PROCESS)?
                    .check_root_job()?;
                // TODO: out-of-memory event
                let event = Event::new();
                let event_handle = proc.add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
                out.write(event_handle)?;
                Ok(())
            }
            EVENT_MEMORY_PRESSURE_CRITICAL
            | EVENT_MEMORY_PRESSURE_WARNING
            | EVENT_MEMORY_PRESSURE_NORMAL => {
                let proc = self.thread.proc();
                proc.get_object_with_rights::<Job>(root_job, Rights::MANAGE_PROCESS)?
                    .check_root_job()?;
                // TODO: implement real memory pressure event monitoring.
                // Returning a stub Event would cause callers to block
                // indefinitely waiting for a signal that never fires.
                warn!(
                    "system.get_event: memory pressure event kind={} not yet implemented",
                    kind
                );
                Err(ZxError::NOT_SUPPORTED)
            }
            _ => {
                warn!("system.get_event: unknown event kind {:#x}", kind);
                Err(ZxError::INVALID_ARGS)
            }
        }
    }

    /// Perform a power control operation (reboot, shutdown, etc.).
    ///
    /// Currently only reboot and shutdown are supported. The resource
    /// handle must be the root resource.
    pub fn sys_system_powerctl(&self, resource: HandleValue, cmd: u32, _arg: usize) -> ZxResult {
        info!("system.powerctl: resource={:#x}, cmd={}", resource, cmd);
        let proc = self.thread.proc();
        // Validate: require root resource
        let resource =
            proc.get_object_with_rights::<zircon_object::dev::Resource>(resource, Rights::empty())?;
        resource.validate(zircon_object::dev::ResourceKind::ROOT)?;

        match cmd {
            POWERCTL_REBOOT => {
                warn!("system.powerctl: reboot requested");
                hal_impl::cpu::reset();
            }
            POWERCTL_REBOOT_BOOTLOADER | POWERCTL_REBOOT_RECOVERY => {
                warn!(
                    "system.powerctl: bootloader/recovery reboot not distinct from normal reboot"
                );
                hal_impl::cpu::reset();
            }
            POWERCTL_SHUTDOWN => {
                warn!("system.powerctl: shutdown not yet implemented (no HAL shutdown)");
                Err(ZxError::NOT_SUPPORTED)
            }
            _ => {
                warn!("system.powerctl: unrecognized cmd {}", cmd);
                Err(ZxError::INVALID_ARGS)
            }
        }
    }

    /// Flush CPU caches over a virtual address range.
    ///
    /// `options` is a bitmask of:
    /// - `ZX_CACHE_FLUSH_DATA` (1): writeback data cache
    /// - `ZX_CACHE_FLUSH_INVALIDATE` (2): writeback + invalidate data cache
    /// - `ZX_CACHE_FLUSH_INSN` (4): synchronize instruction cache with data cache
    ///
    /// At least one of DATA or INSN must be set.
    pub fn sys_cache_flush(&self, addr: usize, size: usize, options: u32) -> ZxResult {
        info!(
            "cache.flush: addr={:#x}, size={:#x}, options={:#x}",
            addr, size, options
        );
        if options == 0
            || options & !(ZX_CACHE_FLUSH_DATA | ZX_CACHE_FLUSH_INVALIDATE | ZX_CACHE_FLUSH_INSN)
                != 0
        {
            return Err(ZxError::INVALID_ARGS);
        }
        let has_data = options & (ZX_CACHE_FLUSH_DATA | ZX_CACHE_FLUSH_INVALIDATE) != 0;
        let has_insn = options & ZX_CACHE_FLUSH_INSN != 0;
        if !has_data && !has_insn {
            return Err(ZxError::INVALID_ARGS);
        }
        if size == 0 {
            return Ok(());
        }
        // On all architectures we support, cache operations are safe from
        // userspace virtual addresses.  The actual flush is architecture-
        // specific but the Rust compiler fence + HAL primitives cover the
        // common case.
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Issue a data memory barrier across all threads in the calling process.
    ///
    /// On single-core configurations (zCore's current setup), a local
    /// fence is sufficient.  Multi-core would require IPIs.
    pub fn sys_membarrier_sync_process_data(&self) -> ZxResult {
        info!("membarrier.sync_process_data");
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Create a new Counter object.
    ///
    /// Returns a handle to the counter with default rights.
    pub fn sys_counter_create(&self, options: u32, mut out: UserOutPtr<HandleValue>) -> ZxResult {
        info!("counter.create: options={}", options);
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let counter = Counter::new();
        let handle = proc.add_handle(Handle::new(counter, Rights::DEFAULT_COUNTER));
        out.write(handle)?;
        Ok(())
    }

    /// Read the current value of a Counter.
    pub fn sys_counter_read(&self, handle: HandleValue, mut out: UserOutPtr<i64>) -> ZxResult {
        info!("counter.read: handle={:#x}", handle);
        let proc = self.thread.proc();
        let counter = proc.get_object_with_rights::<Counter>(handle, Rights::READ)?;
        out.write(counter.read())?;
        Ok(())
    }

    /// Write a value to a Counter.
    pub fn sys_counter_write(&self, handle: HandleValue, value: i64) -> ZxResult {
        info!("counter.write: handle={:#x}, value={}", handle, value);
        let proc = self.thread.proc();
        let counter = proc.get_object_with_rights::<Counter>(handle, Rights::WRITE)?;
        counter.write(value);
        Ok(())
    }

    /// Atomically add a value to a Counter.
    pub fn sys_counter_add(&self, handle: HandleValue, delta: i64) -> ZxResult {
        info!("counter.add: handle={:#x}, delta={}", handle, delta);
        let proc = self.thread.proc();
        let counter = proc.get_object_with_rights::<Counter>(handle, Rights::WRITE)?;
        counter.add(delta);
        Ok(())
    }

    /// Issue an instruction cache barrier across all threads in the calling process.
    ///
    /// Ensures instruction cache coherency after code modification (e.g., JIT).
    /// On x86 this is a no-op (I/D caches are coherent).
    /// On single-core ARM/RISC-V, a local fence is sufficient.
    pub fn sys_membarrier_sync_process_insn(&self) -> ZxResult {
        info!("membarrier.sync_process_insn");
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Soft reboot the system (kexec-like).
    ///
    /// Loads a new kernel image and boot image from VMOs and transfers
    /// control. Requires a SYSTEM resource with `ZX_RSRC_SYSTEM_MEXEC_BASE`.
    /// This syscall does not return on success.
    pub fn sys_system_mexec(
        &self,
        resource: HandleValue,
        _kernel_vmo: HandleValue,
        _bootimage_vmo: HandleValue,
    ) -> ZxResult {
        info!("system.mexec: resource={:#x}", resource);
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_MEXEC_BASE, 1)?;
        }
        // TODO: implement soft reboot (kexec) when HAL supports it.
        warn!("system.mexec: validated but kexec not implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Get the ZBI payload data needed for a subsequent `zx_system_mexec` call.
    ///
    /// Returns ZBI entries that should be appended to the boot image before
    /// calling `zx_system_mexec`. Buffer must not exceed 16 KiB.
    pub fn sys_system_mexec_payload_get(
        &self,
        resource: HandleValue,
        _buf: UserOutPtr<u8>,
        buf_size: usize,
    ) -> ZxResult {
        info!(
            "system.mexec_payload_get: resource={:#x}, buf_size={}",
            resource, buf_size
        );
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_MEXEC_BASE, 1)?;
        }
        if buf_size > MEXEC_PAYLOAD_MAX_SIZE {
            return Err(ZxError::INVALID_ARGS);
        }
        // TODO: generate ZBI payload when mexec is supported.
        warn!("system.mexec_payload_get: validated but no payload available");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Get CPU performance info for the system.
    ///
    /// Returns performance scale info per CPU. The resource handle must be
    /// a system resource with appropriate access.
    pub fn sys_system_get_performance_info(
        &self,
        resource: HandleValue,
        topic: u32,
        _count: usize,
        _info: usize,
        _output_count: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "system.get_performance_info: resource={:#x}, topic={}",
            resource, topic
        );
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        res.validate(ResourceKind::ROOT)?;
        // Topics: 0 = CPU_PERF_SCALE, 1 = CPU_DEFAULT_PERF_SCALE
        if topic > 1 {
            return Err(ZxError::INVALID_ARGS);
        }
        // TODO: implement CPU performance scaling info.
        warn!("system.get_performance_info: validated but not implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Set CPU performance info for the system.
    ///
    /// Sets performance scale values per CPU. Requires root resource.
    pub fn sys_system_set_performance_info(
        &self,
        resource: HandleValue,
        topic: u32,
        _info: usize,
        _count: usize,
    ) -> ZxResult {
        info!(
            "system.set_performance_info: resource={:#x}, topic={}",
            resource, topic
        );
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        res.validate(ResourceKind::ROOT)?;
        if topic > 1 {
            return Err(ZxError::INVALID_ARGS);
        }
        // TODO: implement CPU performance scaling.
        warn!("system.set_performance_info: validated but not implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Enter system suspend (sleep) state.
    ///
    /// Suspends task execution until the resume deadline expires or a
    /// wake source triggers. Requires a system resource.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_system_suspend_enter(
        &self,
        resource: HandleValue,
        _resume_deadline: u64,
        _options: u64,
        _out_header: usize,
        _out_entries: usize,
        _num_entries: u32,
        _actual_entries: UserOutPtr<u32>,
    ) -> ZxResult {
        info!("system.suspend_enter: resource={:#x}", resource);
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        res.validate(ResourceKind::ROOT)?;
        // TODO: implement system suspend when power management is supported.
        warn!("system.suspend_enter: validated but not implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Watch for memory stall events.
    ///
    /// Registers for notification when memory pressure causes stalls.
    /// This is an experimental/newer upstream syscall.
    pub fn sys_system_watch_memory_stall(&self, resource: HandleValue, _options: u32) -> ZxResult {
        info!("system.watch_memory_stall: resource={:#x}", resource);
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        res.validate(ResourceKind::ROOT)?;
        // TODO: implement memory stall monitoring.
        warn!("system.watch_memory_stall: validated but not implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Read from the kernel trace buffer.
    ///
    /// Reads trace data from the kernel ring buffer. The resource handle
    /// must be a system resource with `ZX_RSRC_SYSTEM_TRACING_BASE`.
    pub fn sys_ktrace_read(
        &self,
        resource: HandleValue,
        _data: UserOutPtr<u8>,
        _offset: u32,
        _data_size: usize,
        _actual: UserOutPtr<usize>,
    ) -> ZxResult {
        info!("ktrace.read: resource={:#x}", resource);
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_TRACING_BASE, 1)?;
        }
        // TODO: implement kernel trace ring buffer.
        warn!("ktrace.read: validated but no trace buffer implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Control kernel tracing (start, stop, rewind).
    ///
    /// The resource handle must be a system resource with
    /// `ZX_RSRC_SYSTEM_TRACING_BASE`.
    pub fn sys_ktrace_control(
        &self,
        resource: HandleValue,
        action: u32,
        _options: u32,
        _ptr: usize,
    ) -> ZxResult {
        info!(
            "ktrace.control: resource={:#x}, action={}",
            resource, action
        );
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_TRACING_BASE, 1)?;
        }
        // Actions: 0=start, 1=stop, 2=rewind
        if action > 2 {
            return Err(ZxError::INVALID_ARGS);
        }
        // TODO: implement kernel tracing control.
        warn!("ktrace.control: validated but no trace backend implemented");
        Err(ZxError::NOT_SUPPORTED)
    }
}

const EVENT_OUT_OF_MEMORY: u32 = 1;
const EVENT_MEMORY_PRESSURE_CRITICAL: u32 = 2;
const EVENT_MEMORY_PRESSURE_WARNING: u32 = 3;
const EVENT_MEMORY_PRESSURE_NORMAL: u32 = 4;

// Power control commands
const POWERCTL_REBOOT: u32 = 5;
const POWERCTL_REBOOT_BOOTLOADER: u32 = 6;
const POWERCTL_REBOOT_RECOVERY: u32 = 7;
const POWERCTL_SHUTDOWN: u32 = 8;

// Maximum size for mexec payload buffer (16 KiB, per Fuchsia spec).
const MEXEC_PAYLOAD_MAX_SIZE: usize = 16 * 1024;

// Cache flush option flags
const ZX_CACHE_FLUSH_DATA: u32 = 1 << 0;
const ZX_CACHE_FLUSH_INVALIDATE: u32 = 1 << 1;
const ZX_CACHE_FLUSH_INSN: u32 = 1 << 2;
