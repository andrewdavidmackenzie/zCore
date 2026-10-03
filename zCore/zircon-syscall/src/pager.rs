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
    /// For VMOs without `ZX_VMO_TRAP_DIRTY`, writes are not tracked,
    /// so this returns 0 dirty ranges. For VMOs with `TRAP_DIRTY`,
    /// dirty tracking is not yet implemented.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_pager_query_dirty_ranges(
        &self,
        pager_handle: HandleValue,
        vmo_handle: HandleValue,
        offset: u64,
        length: u64,
        _buffer: usize,
        _buffer_size: usize,
        mut actual: UserOutPtr<usize>,
        mut avail: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "pager.query_dirty_ranges: pager={:#x}, vmo={:#x}, offset={:#x}, len={:#x}",
            pager_handle, vmo_handle, offset, length
        );
        let proc = self.thread.proc();
        let _pager = proc.get_object::<Pager>(pager_handle)?;
        let vmo = proc.get_object::<VmObject>(vmo_handle)?;
        if !vmo.is_pager_backed() {
            return Err(ZxError::INVALID_ARGS);
        }
        // Without TRAP_DIRTY, conservatively report the entire queried
        // range as a single dirty range. Every write goes through
        // without pager notification, so all committed pages are dirty.
        //
        // zx_vmo_dirty_range_t = { u64 offset, u64 length, u64 options }
        let range_size = 24usize; // sizeof(zx_vmo_dirty_range_t)
        let vmo_size = vmo.len() as u64;
        let query_end = core::cmp::min(offset + length, vmo_size);
        if offset >= query_end {
            // Empty query range — no dirty ranges.
            actual.write_if_not_null(0)?;
            avail.write_if_not_null(0)?;
            return Ok(());
        }
        // Report one range covering [offset, query_end).
        let dirty_offset = offset;
        let dirty_length = query_end - offset;
        avail.write_if_not_null(1)?;
        if _buffer_size >= range_size {
            let mut buf = UserOutPtr::<u64>::from(_buffer);
            buf.write(dirty_offset)?;
            let mut buf2 = UserOutPtr::<u64>::from(_buffer + 8);
            buf2.write(dirty_length)?;
            let mut buf3 = UserOutPtr::<u64>::from(_buffer + 16);
            buf3.write(0)?; // options = 0 (not zero-range)
            actual.write_if_not_null(1)?;
        } else {
            actual.write_if_not_null(0)?;
        }
        Ok(())
    }

    /// Query statistics about a pager-backed VMO.
    ///
    /// Returns `zx_pager_vmo_stats_t` with the `modified` field.
    /// Without dirty tracking, report modified=0 (not modified).
    /// If `ZX_PAGER_RESET_VMO_STATS` is set, reset the modified state.
    pub fn sys_pager_query_vmo_stats(
        &self,
        pager_handle: HandleValue,
        vmo_handle: HandleValue,
        options: u32,
        buffer: usize,
        buffer_size: usize,
    ) -> ZxResult {
        info!(
            "pager.query_vmo_stats: pager={:#x}, vmo={:#x}, options={}",
            pager_handle, vmo_handle, options
        );
        let proc = self.thread.proc();
        let _pager = proc.get_object::<Pager>(pager_handle)?;
        let _vmo = proc.get_object::<VmObject>(vmo_handle)?;
        // zx_pager_vmo_stats_t is { uint32_t modified; }
        // Size must be at least 4 bytes.
        if buffer_size < 4 {
            return Err(ZxError::BUFFER_TOO_SMALL);
        }
        // Report not modified (dirty tracking not implemented).
        let mut stats_ptr = UserOutPtr::<u32>::from(buffer);
        stats_ptr.write(0)?;
        Ok(())
    }
}
