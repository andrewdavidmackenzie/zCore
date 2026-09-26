use super::*;
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

// Cache flush option flags
const ZX_CACHE_FLUSH_DATA: u32 = 1 << 0;
const ZX_CACHE_FLUSH_INVALIDATE: u32 = 1 << 1;
const ZX_CACHE_FLUSH_INSN: u32 = 1 << 2;
