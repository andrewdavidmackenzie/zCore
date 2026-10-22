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
        // ZX_DEFAULT_PAGER_RIGHTS = INSPECT | TRANSFER | ATTACH_VMO | MANAGE_VMO
        let pager_rights =
            Rights::INSPECT | Rights::TRANSFER | Rights::ATTACH_VMO | Rights::MANAGE_VMO;
        let handle = proc.add_handle(Handle::new(pager, pager_rights));
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
        let mut vmo_rights = Rights::DEFAULT_VMO;
        if options & (1 << 1) != 0 {
            // ZX_VMO_RESIZABLE = 1 << 1
            vmo_rights.insert(Rights::RESIZE);
        }
        let handle = proc.add_handle(Handle::new(vmo, vmo_rights));
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
        // Fuchsia requires WRITE right on the VMO handle for detach.
        let vmo = proc.get_object_with_rights::<VmObject>(vmo_handle, Rights::WRITE)?;
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
        // Fuchsia requires WRITE right on the pager VMO handle for supply.
        let vmo = proc.get_object_with_rights::<VmObject>(vmo_handle, Rights::WRITE)?;
        // Validate VMO ownership before looking up aux_vmo handle.
        // Fuchsia checks ownership before validating aux_vmo.
        if !pager.owns_vmo(&vmo) {
            return Err(ZxError::INVALID_ARGS);
        }
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
        let pager = proc.get_object_with_rights::<Pager>(pager_handle, Rights::MANAGE_VMO)?;
        let vmo = proc.get_object_with_rights::<VmObject>(vmo_handle, Rights::WRITE)?;
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
        buffer: usize,
        buffer_size: usize,
        mut actual: UserOutPtr<usize>,
        mut avail: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "pager.query_dirty_ranges: pager={:#x}, vmo={:#x}, offset={:#x}, len={:#x}",
            pager_handle, vmo_handle, offset, length
        );
        let proc = self.thread.proc();
        // Pager needs MANAGE_VMO right.
        let pager = proc.get_object_with_rights::<Pager>(pager_handle, Rights::MANAGE_VMO)?;
        // VMO needs READ right.
        let vmo = proc.get_object_with_rights::<VmObject>(vmo_handle, Rights::READ)?;
        // VMO must be (or have been) pager-backed. Query is valid
        // even after the pager is detached.
        if !vmo.was_pager_backed() {
            return Err(ZxError::INVALID_ARGS);
        }
        // VMO must belong to this pager. After detach, the VMO is
        // removed from the pager's list but queries are still valid.
        if vmo.is_pager_backed() && !pager.owns_vmo(&vmo) {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate range doesn't overflow.
        let query_end = offset.checked_add(length).ok_or(ZxError::OUT_OF_RANGE)?;
        if query_end as usize > vmo.len() {
            return Err(ZxError::OUT_OF_RANGE);
        }

        // Each range is 24 bytes: { offset: u64, length: u64, options: u64 }
        const RANGE_SIZE: usize = 24;

        // Null buffer with non-zero capacity is invalid.
        let max_entries = buffer_size / RANGE_SIZE;
        if buffer == 0 && max_entries > 0 {
            return Err(ZxError::INVALID_ARGS);
        }

        let ranges = vmo.query_dirty_ranges(offset as usize, length as usize);
        let avail_count = ranges.len();
        let write_count = avail_count.min(max_entries);
        if write_count > 0 && buffer != 0 {
            let mut out = UserOutPtr::<u8>::from(buffer);
            for &(off, len, opts) in ranges.iter().take(write_count) {
                out.write_array(&off.to_ne_bytes())?;
                out = UserOutPtr::from(out.as_addr() + 8);
                out.write_array(&len.to_ne_bytes())?;
                out = UserOutPtr::from(out.as_addr() + 8);
                out.write_array(&opts.to_ne_bytes())?;
                out = UserOutPtr::from(out.as_addr() + 8);
            }
        }

        actual.write_if_not_null(write_count)?;
        avail.write_if_not_null(avail_count)?;
        Ok(())
    }

    /// Query statistics about a pager-backed VMO.
    ///
    /// Returns `zx_pager_vmo_stats_t` with the `modified` field.
    /// Without dirty tracking, report modified=0 (not modified).
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
        const ZX_PAGER_RESET_VMO_STATS: u32 = 1;
        if options & !ZX_PAGER_RESET_VMO_STATS != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        // Pager needs MANAGE_VMO right.
        let pager = proc.get_object_with_rights::<Pager>(pager_handle, Rights::MANAGE_VMO)?;
        // VMO needs READ right.
        let vmo = proc.get_object_with_rights::<VmObject>(vmo_handle, Rights::READ)?;
        if !vmo.was_pager_backed() {
            return Err(ZxError::INVALID_ARGS);
        }
        // VMO must belong to this pager. After detach, the VMO is
        // removed from the pager's list but queries are still valid.
        if vmo.is_pager_backed() && !pager.owns_vmo(&vmo) {
            return Err(ZxError::INVALID_ARGS);
        }
        // zx_pager_vmo_stats_t is { uint32_t modified; }
        if buffer_size < 4 {
            return Err(ZxError::BUFFER_TOO_SMALL);
        }
        // Null buffer is invalid.
        if buffer == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let reset = options & ZX_PAGER_RESET_VMO_STATS != 0;
        let modified = vmo.query_modified(reset);
        const ZX_PAGER_VMO_STATS_MODIFIED: u32 = 1;
        let mut stats_ptr = UserOutPtr::<u32>::from(buffer);
        stats_ptr.write(if modified {
            ZX_PAGER_VMO_STATS_MODIFIED
        } else {
            0
        })?;
        Ok(())
    }
}
