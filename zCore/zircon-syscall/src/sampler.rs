use super::*;
use zircon_object::dev::*;
use zircon_object::ipc::IoBuffer;

impl Syscall<'_> {
    /// Create a thread sampler session.
    ///
    /// Creates an IOBuffer-based sampler that periodically captures
    /// thread state (PC, callstack) into per-CPU IOB regions.
    ///
    /// Requires a system resource with debug access and
    /// `kernel.enable-debugging-syscalls=true`.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_sampler_create(
        &self,
        resource: HandleValue,
        options: u64,
        _config_ptr: usize,
        _config_size: usize,
        _iob_out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "sampler.create: resource={:#x}, options={}",
            resource, options
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let res = proc.get_object::<Resource>(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_DEBUG_BASE, 1)?;
        }
        // TODO: parse zx_sampler_config_t, create sampling session with IOB output.
        warn!("sampler.create: validated but sampling engine not implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Start periodic sampling on a sampler IOBuffer.
    pub fn sys_sampler_start(&self, handle: HandleValue) -> ZxResult {
        info!("sampler.start: handle={:#x}", handle);
        let proc = self.thread.proc();
        let _iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::WRITE)?;
        // TODO: start timer-interrupt-driven sampling into IOB regions.
        warn!("sampler.start: validated but not implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Stop periodic sampling on a sampler IOBuffer.
    pub fn sys_sampler_stop(&self, handle: HandleValue) -> ZxResult {
        info!("sampler.stop: handle={:#x}", handle);
        let proc = self.thread.proc();
        let _iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::WRITE)?;
        // TODO: stop sampling timer.
        warn!("sampler.stop: validated but not implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Read available samples from a sampler.
    pub fn sys_sampler_read(
        &self,
        handle: HandleValue,
        _data: UserOutPtr<u8>,
        _data_size: usize,
        _actual: UserOutPtr<usize>,
    ) -> ZxResult {
        info!("sampler.read: handle={:#x}", handle);
        let proc = self.thread.proc();
        let _iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::READ)?;
        // TODO: read collected samples from IOB regions.
        warn!("sampler.read: validated but not implemented");
        Err(ZxError::NOT_SUPPORTED)
    }
}
