use {
    super::*,
    zircon_object::{signal::*, vm::*},
};

impl Syscall<'_> {
    /// Create a pager object.
    pub fn sys_pager_create(&self, options: u32, mut out: UserOutPtr<HandleValue>) -> ZxResult {
        info!("pager.create: options={:#x}", options);
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let pager = Pager::new();
        let proc = self.thread.proc();
        let handle = proc.add_handle(Handle::new(pager, Rights::DEFAULT_CHANNEL));
        out.write(handle)?;
        Ok(())
    }

    /// Create a VMO backed by a pager.
    pub fn sys_pager_create_vmo(
        &self,
        pager_handle: HandleValue,
        options: u32,
        port_handle: HandleValue,
        key: u64,
        size: u64,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "pager.create_vmo: pager={:#x}, options={:#x}, port={:#x}, key={:#x}, size={:#x}",
            pager_handle, options, port_handle, key, size
        );
        let proc = self.thread.proc();
        let pager = proc.get_object::<Pager>(pager_handle)?;
        let port = proc.get_object_with_rights::<Port>(port_handle, Rights::WRITE)?;
        let vmo = pager.create_vmo(options, &port, key, size)?;
        let handle = proc.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));
        out.write(handle)?;
        Ok(())
    }

    /// Detach a VMO from its pager.
    pub fn sys_pager_detach_vmo(
        &self,
        pager_handle: HandleValue,
        vmo_handle: HandleValue,
    ) -> ZxResult {
        info!(
            "pager.detach_vmo: pager={:#x}, vmo={:#x}",
            pager_handle, vmo_handle
        );
        let proc = self.thread.proc();
        let pager = proc.get_object::<Pager>(pager_handle)?;
        let vmo = proc.get_object::<VmObject>(vmo_handle)?;
        pager.detach_vmo(&vmo)
    }

    /// Supply pages to a pager-backed VMO from an auxiliary VMO.
    pub fn sys_pager_supply_pages(
        &self,
        pager_handle: HandleValue,
        vmo_handle: HandleValue,
        offset: u64,
        length: u64,
        aux_vmo_handle: HandleValue,
        aux_offset: u64,
    ) -> ZxResult {
        info!(
            "pager.supply_pages: pager={:#x}, vmo={:#x}, offset={:#x}, len={:#x}, aux={:#x}, aux_off={:#x}",
            pager_handle, vmo_handle, offset, length, aux_vmo_handle, aux_offset
        );
        let proc = self.thread.proc();
        let pager = proc.get_object::<Pager>(pager_handle)?;
        let vmo = proc.get_object::<VmObject>(vmo_handle)?;
        let aux_vmo =
            proc.get_object_with_rights::<VmObject>(aux_vmo_handle, Rights::READ | Rights::WRITE)?;
        pager.supply_pages(&vmo, offset, length, &aux_vmo, aux_offset)
    }

    /// Perform an operation on a range of a pager-backed VMO.
    pub fn sys_pager_op_range(
        &self,
        pager_handle: HandleValue,
        op: u32,
        vmo_handle: HandleValue,
        offset: u64,
        length: u64,
        data: u64,
    ) -> ZxResult {
        info!(
            "pager.op_range: pager={:#x}, op={:#x}, vmo={:#x}, offset={:#x}, len={:#x}, data={:#x}",
            pager_handle, op, vmo_handle, offset, length, data
        );
        let proc = self.thread.proc();
        let pager = proc.get_object::<Pager>(pager_handle)?;
        let vmo = proc.get_object::<VmObject>(vmo_handle)?;
        pager.op_range(op, &vmo, offset, length, data)
    }

    /// Query dirty page ranges of a pager-backed VMO.
    ///
    /// Returns ranges of pages that have been modified since the last
    /// writeback. Currently returns NOT_SUPPORTED as dirty page tracking
    /// is not yet implemented in the VMO subsystem.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_pager_query_dirty_ranges(
        &self,
        pager_handle: HandleValue,
        vmo_handle: HandleValue,
        offset: u64,
        length: u64,
        _buffer: usize,
        _buffer_size: usize,
        _actual: UserOutPtr<usize>,
        _avail: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "pager.query_dirty_ranges: pager={:#x}, vmo={:#x}, offset={:#x}, len={:#x}",
            pager_handle, vmo_handle, offset, length
        );
        let proc = self.thread.proc();
        let _pager = proc.get_object::<Pager>(pager_handle)?;
        let _vmo = proc.get_object::<VmObject>(vmo_handle)?;
        // TODO: implement dirty page tracking in VMO subsystem
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Query statistics about a pager-backed VMO.
    ///
    /// Returns statistics like committed bytes and populated bytes.
    /// Currently returns NOT_SUPPORTED as VMO statistics tracking
    /// is not yet implemented.
    pub fn sys_pager_query_vmo_stats(
        &self,
        pager_handle: HandleValue,
        options: u32,
        _buffer: usize,
        _buffer_size: usize,
    ) -> ZxResult {
        info!(
            "pager.query_vmo_stats: pager={:#x}, options={}",
            pager_handle, options
        );
        let proc = self.thread.proc();
        let _pager = proc.get_object::<Pager>(pager_handle)?;
        // TODO: implement VMO statistics tracking
        Err(ZxError::NOT_SUPPORTED)
    }
}
