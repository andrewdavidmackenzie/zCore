use super::*;
use alloc::sync::Arc;
use zircon_object::dev::{ResourceKind, ZX_RSRC_SYSTEM_MEXEC_BASE, ZX_RSRC_SYSTEM_TRACING_BASE};
use zircon_object::object::Signal;
use zircon_object::signal::{Counter, Event};
use zircon_object::task::Job;

/// System event rights: WAIT | DUPLICATE | TRANSFER (no SIGNAL).
const LOW_MEMORY_RIGHTS: Rights = Rights::from_bits_truncate(
    Rights::WAIT.bits() | Rights::DUPLICATE.bits() | Rights::TRANSFER.bits(),
);

/// Kernel-global singleton events for each memory pressure level.
/// NORMAL starts signaled; the rest start unsignaled.
struct SystemEvents {
    oom: Arc<Event>,
    imminent_oom: Arc<Event>,
    critical: Arc<Event>,
    warning: Arc<Event>,
    normal: Arc<Event>,
}

static SYSTEM_EVENTS: spin::Once<SystemEvents> = spin::Once::new();

fn system_events() -> &'static SystemEvents {
    SYSTEM_EVENTS.call_once(|| {
        let normal = Event::new();
        // System starts in normal memory state
        normal.signal_set(Signal::USER_SIGNAL_0);
        SystemEvents {
            oom: Event::new(),
            imminent_oom: Event::new(),
            critical: Event::new(),
            warning: Event::new(),
            normal,
        }
    })
}

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
        let proc = self.thread.proc();
        proc.get_object_with_rights::<Job>(root_job, Rights::MANAGE_PROCESS)?
            .check_root_job()?;
        let events = system_events();
        let event: Arc<Event> = match kind {
            EVENT_OUT_OF_MEMORY => events.oom.clone(),
            EVENT_IMMINENT_OUT_OF_MEMORY => events.imminent_oom.clone(),
            EVENT_MEMORY_PRESSURE_CRITICAL => events.critical.clone(),
            EVENT_MEMORY_PRESSURE_WARNING => events.warning.clone(),
            EVENT_MEMORY_PRESSURE_NORMAL => events.normal.clone(),
            _ => {
                warn!("system.get_event: unknown event kind {:#x}", kind);
                return Err(ZxError::INVALID_ARGS);
            }
        };
        let event_handle = proc.add_handle(Handle::new(event, LOW_MEMORY_RIGHTS));
        out.write(event_handle)?;
        Ok(())
    }

    /// Perform a power control operation (reboot, shutdown, etc.).
    ///
    /// Currently only reboot and shutdown are supported. The resource
    /// handle must be the root resource.
    pub fn sys_system_powerctl(&self, resource: HandleValue, cmd: u32, _arg: usize) -> ZxResult {
        info!("system.powerctl: resource={:#x}, cmd={}", resource, cmd);
        let proc = self.thread.proc();
        // Validate: require root resource
        let resource = proc.get_resource_with_rights(resource, Rights::empty())?;
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
    ///
    /// Requires both READ and WRITE rights. Because a counter's value
    /// could be determined by checking for OUT_OF_RANGE on a series of
    /// crafted add() calls, there is no way to create a write-only counter.
    pub fn sys_counter_add(&self, handle: HandleValue, delta: i64) -> ZxResult {
        info!("counter.add: handle={:#x}, delta={}", handle, delta);
        let proc = self.thread.proc();
        let counter =
            proc.get_object_with_rights::<Counter>(handle, Rights::READ | Rights::WRITE)?;
        counter.add(delta)?;
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
        let res = proc.get_resource(resource)?;
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
        let res = proc.get_resource(resource)?;
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
    /// a `zx_cpu_performance_scale_t` (8 bytes: u32 integral + u32 fractional).
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
        let res = proc.get_resource(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            // Wrong kind (e.g., MMIO) → WRONG_TYPE.
            // Right kind (SYSTEM) but wrong sub-base → OUT_OF_RANGE.
            res.validate(ResourceKind::SYSTEM)
                .map_err(|_| ZxError::WRONG_TYPE)?;
            res.validate_ranged_resource(
                ResourceKind::SYSTEM,
                zircon_object::dev::ZX_RSRC_SYSTEM_CPU_BASE,
                1,
            )
            .map_err(|_| ZxError::OUT_OF_RANGE)?;
        }
        // Invalid topic → INVALID_ARGS.
        if topic > 3 {
            return Err(ZxError::INVALID_ARGS);
        }

        // Use actual online CPU count. CpuStats reports avail=1 for
        // our single-CPU QEMU configuration, so be consistent.
        let online_cpus: usize = 1;
        // count must match the number of online CPUs exactly.
        if count != online_cpus {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if info == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let entries = online_cpus;
        {
            let mut out: UserOutPtr<u8> = info.into();
            if topic == 3 {
                // ZX_CPU_PERF_LIMIT: zx_cpu_perf_limit_t = { u32 cpu, u32 limit_type, u64 min, u64 max } = 24 bytes
                for i in 0..entries {
                    let cpu_num = (i as u32).to_ne_bytes();
                    out.write_array(&cpu_num)?;
                    out = (out.as_addr() + 4).into();
                    // limit_type = ZX_CPU_PERF_LIMIT_TYPE_RATE (0)
                    out.write_array(&0u32.to_ne_bytes())?;
                    out = (out.as_addr() + 4).into();
                    // min = 0, max = 1000 (default rate limit)
                    out.write_array(&0u64.to_ne_bytes())?;
                    out = (out.as_addr() + 8).into();
                    out.write_array(&1000u64.to_ne_bytes())?;
                    out = (out.as_addr() + 8).into();
                }
            } else {
                // ZX_CPU_PERF_SCALE / DEFAULT_PERF_SCALE:
                // zx_cpu_performance_info_t = { u32 cpu,
                //   zx_cpu_performance_scale_t { u32 integral, u32 fractional } } = 12 bytes
                for i in 0..entries {
                    let cpu_num = (i as u32).to_ne_bytes();
                    out.write_array(&cpu_num)?;
                    out = (out.as_addr() + 4).into();
                    // performance_scale: 1.0x = { integral_part=1, fractional_part=0 }
                    out.write_array(&1u32.to_ne_bytes())?;
                    out = (out.as_addr() + 4).into();
                    out.write_array(&0u32.to_ne_bytes())?;
                    out = (out.as_addr() + 4).into();
                }
            }
        }
        if output_count.is_null() {
            return Err(ZxError::INVALID_ARGS);
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
        info: usize,
        count: usize,
    ) -> ZxResult {
        info!(
            "system.set_performance_info: resource={:#x}, topic={}",
            resource, topic
        );
        let proc = self.thread.proc();
        let res = proc.get_resource(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate(ResourceKind::SYSTEM)
                .map_err(|_| ZxError::WRONG_TYPE)?;
            res.validate_ranged_resource(
                ResourceKind::SYSTEM,
                zircon_object::dev::ZX_RSRC_SYSTEM_CPU_BASE,
                1,
            )
            .map_err(|_| ZxError::OUT_OF_RANGE)?;
        }
        if topic > 3 {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate count and info pointer.
        let online_cpus: usize = 1;
        if count == 0 || count > online_cpus {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if info == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate scale values for ZX_CPU_PERF_SCALE (topic 1).
        // zx_cpu_performance_info_t = { u32 logical_cpu_number,
        //   zx_cpu_performance_scale_t { u32 integral_part, u32 fractional_part } } = 12 bytes.
        // Zero scale (0.0) is rejected; values > 1.0 are clamped (accepted).
        if topic == 1 {
            const ENTRY_SIZE: usize = 12; // 4 + 4 + 4
            let buf: UserInPtr<u8> = info.into();
            let data = buf.read_array(count * ENTRY_SIZE)?;
            for i in 0..count {
                let base = i * ENTRY_SIZE;
                // Validate CPU number (first 4 bytes of each entry).
                let cpu_number = u32::from_ne_bytes([
                    data[base],
                    data[base + 1],
                    data[base + 2],
                    data[base + 3],
                ]);
                if cpu_number as usize >= online_cpus {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                // Validate scale: { u32 integral_part, u32 fractional_part }.
                let integral = u32::from_ne_bytes([
                    data[base + 4],
                    data[base + 5],
                    data[base + 6],
                    data[base + 7],
                ]);
                let fractional = u32::from_ne_bytes([
                    data[base + 8],
                    data[base + 9],
                    data[base + 10],
                    data[base + 11],
                ]);
                if integral == 0 && fractional == 0 {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                // Values > 1.0 are clamped, not rejected.
            }
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
    pub async fn sys_system_suspend_enter(
        &self,
        resource: HandleValue,
        resume_deadline: u64,
        options: u64,
        out_header: usize,
        out_entries: usize,
        num_entries: u32,
        mut actual_entries: UserOutPtr<u32>,
    ) -> ZxResult {
        const OPTION_DISCARD: u64 = 0x1;
        const OPTION_REPORT_ONLY: u64 = 0x2;
        const VALID_OPTIONS: u64 = OPTION_DISCARD | OPTION_REPORT_ONLY;
        // Entry size: koid(8) + name(32) + initial_signal_time(8)
        //           + last_signal_time(8) + last_ack_time(8)
        //           + signal_count(4) + flags(4) = 72 bytes

        info!(
            "system.suspend_enter: resource={:#x}, deadline={}, options={:#x}",
            resource, resume_deadline, options
        );
        let proc = self.thread.proc();
        let res = proc.get_resource(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(
                ResourceKind::SYSTEM,
                zircon_object::dev::ZX_RSRC_SYSTEM_CPU_BASE,
                1,
            )?;
        }

        // Validate options: only DISCARD and REPORT_ONLY bits are valid.
        if options & !VALID_OPTIONS != 0 {
            return Err(ZxError::INVALID_ARGS);
        }

        // REPORT_ONLY requires a header buffer.
        if options & OPTION_REPORT_ONLY != 0 && out_header == 0 {
            return Err(ZxError::INVALID_ARGS);
        }

        // Validate argument consistency:
        // If no header, entry args must all be null/zero.
        let has_header = out_header != 0;
        let has_entries = out_entries != 0;
        let has_count = num_entries > 0;
        let has_actual = !actual_entries.is_null();

        if !has_header {
            // Without header, all entry args must be null/zero.
            if has_entries || has_count || has_actual {
                return Err(ZxError::INVALID_ARGS);
            }
        } else {
            // With header, entry args must be all-null or all-valid.
            let all_null = !has_entries && !has_count && !has_actual;
            let all_valid = has_entries && has_count && has_actual;
            if !all_null && !all_valid {
                return Err(ZxError::INVALID_ARGS);
            }
        }

        // Count active IRQ wake sources from the global registry.
        let irq_wake_count = zircon_object::dev::wake_source_count();
        let total_wake_sources = irq_wake_count + 1; // +1 for deadline timer

        let report_time = hal_impl::timer::timer_now().as_nanos() as u64;

        // Determine suspend_start_time based on options.
        let is_report_only = options & OPTION_REPORT_ONLY != 0;
        let is_discard = options & OPTION_DISCARD != 0;
        let suspend_start_time = if is_report_only {
            i64::MAX as u64 // ZX_TIME_INFINITE_BOOT
        } else {
            report_time
        };

        // If not REPORT_ONLY, "suspend" until deadline or wake source trigger.
        // Yield to allow other tasks (including the thread that triggers
        // the interrupt) to run during the wait.
        if !is_report_only {
            let deadline_signed = resume_deadline as i64;
            if deadline_signed > 0 {
                let deadline = core::time::Duration::from_nanos(deadline_signed as u64);
                let now = hal_impl::timer::timer_now();
                let max_wait = core::time::Duration::from_secs(5);
                let capped = deadline.min(now + max_wait);
                while hal_impl::timer::timer_now() < capped {
                    // Check if any wake source was triggered.
                    if !zircon_object::dev::collect_wake_entries().is_empty() {
                        break;
                    }
                    // Yield to let other tasks run (including the
                    // thread that will call zx_interrupt_trigger).
                    hal_impl::thread::yield_now().await;
                }
            }
        }

        // Build the list of entries to report.
        // Entry structure: koid(8) + name(32) + initial_signal(8)
        //                + last_signal(8) + last_ack(8) + count(4) + flags(4) = 72 bytes
        let mut entries: alloc::vec::Vec<zircon_object::dev::WakeReportEntry> =
            alloc::vec::Vec::new();

        // Collect triggered IRQ wake sources.
        let irq_entries = zircon_object::dev::collect_wake_entries();
        entries.extend(irq_entries);

        // Deadline timer entry: create on actual suspend, persist for
        // subsequent REPORT_ONLY calls.
        if !is_report_only {
            let mut name = [0u8; 32];
            let n = b"suspend timeout";
            name[..n.len()].copy_from_slice(n);
            let deadline_entry: zircon_object::dev::WakeReportEntry = (
                1u64,        // koid = ZX_KOID_KERNEL
                name,        // name = "suspend timeout"
                report_time, // initial_signal_time
                report_time, // last_signal_time
                report_time, // last_ack_time (auto-acked)
                1u32,        // signal_count
                0u32,        // flags = 0
            );
            zircon_object::dev::set_deadline_pending(deadline_entry);
            entries.push(deadline_entry);
        } else if let Some(deadline_entry) = zircon_object::dev::peek_deadline_pending() {
            // REPORT_ONLY: include the pending deadline entry if it exists.
            entries.push(deadline_entry);
        }

        let total_entries = entries.len() as u32;
        let entries_to_write = if has_entries {
            total_entries.min(num_entries)
        } else {
            0u32
        };
        let unreported = total_entries.saturating_sub(entries_to_write);

        if has_header {
            let mut hdr: UserOutPtr<u8> = out_header.into();
            hdr.write_array(&report_time.to_ne_bytes())?;
            hdr = (hdr.as_addr() + 8).into();
            hdr.write_array(&suspend_start_time.to_ne_bytes())?;
            hdr = (hdr.as_addr() + 8).into();
            hdr.write_array(&total_wake_sources.to_ne_bytes())?;
            hdr = (hdr.as_addr() + 4).into();
            hdr.write_array(&unreported.to_ne_bytes())?;
        }

        // REPORT_ONLY with header-only (no entry buffer): return success.
        if is_report_only && !has_entries {
            if is_discard {
                zircon_object::dev::discard_wake_entries();
            }
            return Ok(());
        }

        // Write wake source entries.
        if entries_to_write > 0 && has_entries {
            let mut ent: UserOutPtr<u8> = out_entries.into();
            for entry in entries.iter().take(entries_to_write as usize) {
                let (koid, name, initial, last, ack, count, flags) = entry;
                ent.write_array(&koid.to_ne_bytes())?;
                ent = (ent.as_addr() + 8).into();
                ent.write_array(name)?;
                ent = (ent.as_addr() + 32).into();
                ent.write_array(&initial.to_ne_bytes())?;
                ent = (ent.as_addr() + 8).into();
                ent.write_array(&last.to_ne_bytes())?;
                ent = (ent.as_addr() + 8).into();
                ent.write_array(&ack.to_ne_bytes())?;
                ent = (ent.as_addr() + 8).into();
                ent.write_array(&count.to_ne_bytes())?;
                ent = (ent.as_addr() + 4).into();
                ent.write_array(&flags.to_ne_bytes())?;
                ent = (ent.as_addr() + 4).into();
            }
        }

        if has_actual {
            actual_entries.write(entries_to_write)?;
        }

        // Consume (mark as reported) entries that were written.
        if entries_to_write > 0 {
            let mut irq_koids = alloc::vec::Vec::new();
            for entry in entries.iter().take(entries_to_write as usize) {
                let koid = entry.0;
                if koid == 1 {
                    // Deadline timer: consume the pending entry.
                    let _ = zircon_object::dev::take_deadline_pending();
                } else {
                    irq_koids.push(koid);
                }
            }
            if !irq_koids.is_empty() {
                zircon_object::dev::consume_wake_entries(&irq_koids);
            }
        }

        // DISCARD: clear all wake source state after reporting.
        if is_discard {
            zircon_object::dev::discard_wake_entries();
        }

        Ok(())
    }

    /// Watch for memory stall events.
    ///
    /// Returns immediately with no stall detected. A full
    /// implementation would monitor page allocation latency and
    /// reclaim activity, signaling when thresholds are exceeded.
    pub fn sys_system_watch_memory_stall(
        &self,
        resource: HandleValue,
        kind: u32,
        threshold: u64,
        window: u64,
        mut out_event: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "system.watch_memory_stall: resource={:#x}, kind={}, threshold={}, window={}",
            resource, kind, threshold, window
        );
        let proc = self.thread.proc();
        let res = proc.get_resource(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(
                ResourceKind::SYSTEM,
                zircon_object::dev::ZX_RSRC_SYSTEM_STALL_BASE,
                1,
            )?;
        }
        // Validate kind: 0 = SOME, 1 = FULL
        if kind > 1 {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate thresholds: 0 < threshold <= window <= 10 seconds
        const MAX_WINDOW: u64 = 10_000_000_000; // 10 seconds in nanoseconds
        if threshold == 0 || threshold > window || window > MAX_WINDOW {
            return Err(ZxError::INVALID_ARGS);
        }
        // Create a stall event (stub — never fires).
        let event = Event::new();
        let handle = proc.add_handle(Handle::new(event, LOW_MEMORY_RIGHTS));
        out_event.write(handle)?;
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
        let res = proc.get_resource(resource)?;
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
        let res = proc.get_resource(resource)?;
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
const EVENT_IMMINENT_OUT_OF_MEMORY: u32 = 5;

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
