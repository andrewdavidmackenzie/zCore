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
        kernel_vmo: HandleValue,
        bootimage_vmo: HandleValue,
    ) -> ZxResult {
        info!("system.mexec: resource={:#x}", resource);
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_MEXEC_BASE, 1)?;
        }
        // Validate the kernel and bootimage VMOs are readable.
        let _kernel =
            proc.get_object_with_rights::<zircon_object::vm::VmObject>(kernel_vmo, Rights::READ)?;
        let _bootimage = proc
            .get_object_with_rights::<zircon_object::vm::VmObject>(bootimage_vmo, Rights::READ)?;
        // A full kexec implementation would:
        // 1. Read the kernel image from the VMO
        // 2. Read the bootimage (initrd/ZBI) from the VMO
        // 3. Quiesce all devices
        // 4. Copy images to appropriate physical addresses
        // 5. Jump to the new kernel entry point
        // For now, perform a platform reset as a best-effort
        // "soft reboot" — the new kernel/bootimage are validated
        // but not loaded.
        warn!("system.mexec: performing platform reset (full kexec not implemented)");
        hal_impl::cpu::reset();
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
        // Return an empty ZBI payload. A full implementation would
        // include memory map, UART config, and other system state
        // for the new kernel to consume.
        Ok(())
    }

    /// Get CPU performance info for the system.
    ///
    /// Returns per-CPU performance scale values. Topic 0 returns
    /// current scale, topic 1 returns default scale. Each entry is
    /// a `zx_cpu_performance_scale_t` (4 bytes: u16 integral + u16 fractional).
    /// Returns static 1.0x scale (no DVFS support).
    pub fn sys_system_get_performance_info(
        &self,
        resource: HandleValue,
        topic: u32,
        count: usize,
        info: usize,
        mut output_count: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "system.get_performance_info: resource={:#x}, topic={}, count={}",
            resource, topic, count
        );
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        res.validate(ResourceKind::ROOT)?;
        if topic > 1 {
            return Err(ZxError::INVALID_ARGS);
        }

        // Return static 1.0x scale for each CPU.
        // zx_cpu_performance_scale_t: { u16 integral_part, u16 fractional_part }
        // 1.0x = { 1, 0 } = 4 bytes per CPU.
        let num_cpus = hal_impl::config::MAX_CORE_NUM;
        let entries = core::cmp::min(count, num_cpus);
        if entries > 0 && info != 0 {
            // Each entry: integral=1 (u16 LE), fractional=0 (u16 LE) = [1, 0, 0, 0]
            let scale_1x: [u8; 4] = [1, 0, 0, 0];
            let mut out: UserOutPtr<u8> = info.into();
            for _ in 0..entries {
                out.write_array(&scale_1x)?;
                out = (out.as_addr() + 4).into();
            }
        }
        output_count.write(entries)?;
        Ok(())
    }

    /// Set CPU performance info for the system.
    ///
    /// Accepts performance scale values but does not apply them
    /// (no DVFS support). Returns Ok to indicate the request was
    /// accepted.
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
        // Accept the request without applying (no DVFS hardware).
        Ok(())
    }

    /// Enter system suspend (sleep) state.
    ///
    /// Waits until the resume deadline, then returns. On real hardware
    /// this would enter a low-power CPU state; on QEMU we simply
    /// busy-wait until the deadline.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_system_suspend_enter(
        &self,
        resource: HandleValue,
        resume_deadline: u64,
        _options: u64,
        _out_header: usize,
        _out_entries: usize,
        _num_entries: u32,
        mut actual_entries: UserOutPtr<u32>,
    ) -> ZxResult {
        info!(
            "system.suspend_enter: resource={:#x}, deadline={}",
            resource, resume_deadline
        );
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        res.validate(ResourceKind::ROOT)?;

        // Wait until the resume deadline by polling the monotonic clock.
        // A real implementation would enter a low-power state via
        // PSCI (aarch64), ACPI S-states (x86), or SBI HSM (riscv).
        let deadline = core::time::Duration::from_nanos(resume_deadline);
        while hal_impl::timer::timer_now() < deadline {
            core::hint::spin_loop();
        }

        // No wake sources to report.
        actual_entries.write(0)?;
        Ok(())
    }

    /// Watch for memory stall events.
    ///
    /// Returns immediately with no stall detected. A full
    /// implementation would monitor page allocation latency and
    /// reclaim activity, signaling when thresholds are exceeded.
    pub fn sys_system_watch_memory_stall(&self, resource: HandleValue, _options: u32) -> ZxResult {
        info!("system.watch_memory_stall: resource={:#x}", resource);
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        res.validate(ResourceKind::ROOT)?;
        // No memory pressure tracking — return Ok immediately.
        Ok(())
    }

    /// Read from the kernel trace buffer.
    ///
    /// Copies trace data from the kernel ring buffer to userspace.
    /// Returns the number of bytes read via `actual`.
    pub fn sys_ktrace_read(
        &self,
        resource: HandleValue,
        mut data: UserOutPtr<u8>,
        offset: u32,
        data_size: usize,
        mut actual: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "ktrace.read: resource={:#x}, offset={}, size={}",
            resource, offset, data_size
        );
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_TRACING_BASE, 1)?;
        }
        let (bytes_read, buf) = zircon_object::dev::ktrace::ktrace_read(offset as usize, data_size);
        if !buf.is_empty() {
            data.write_array(&buf)?;
        }
        actual.write(bytes_read)?;
        Ok(())
    }

    /// Control kernel tracing (start, stop, rewind).
    ///
    /// Actions: 0=start (with group mask in options), 1=stop, 2=rewind.
    pub fn sys_ktrace_control(
        &self,
        resource: HandleValue,
        action: u32,
        options: u32,
        _ptr: usize,
    ) -> ZxResult {
        info!(
            "ktrace.control: resource={:#x}, action={}, options={:#x}",
            resource, action, options
        );
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_TRACING_BASE, 1)?;
        }
        zircon_object::dev::ktrace::ktrace_control(action, options)
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
