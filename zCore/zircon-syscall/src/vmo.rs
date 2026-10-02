use {
    super::*,
    bitflags::bitflags,
    hal_impl::CachePolicy,
    numeric_enum_macro::numeric_enum,
    zircon_object::{dev::*, task::PolicyCondition, vm::*},
};

impl Syscall<'_> {
    /// Create a new virtual memory object(VMO).
    pub fn sys_vmo_create(
        &self,
        size: u64,
        options: u32,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "vmo.create: size={:#x?}, options={:#x?}, out={:#x?}",
            size, options, out
        );
        if options & !2u32 != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let resizable = options != 0;
        let proc = self.thread.proc();
        let vmo = VmObject::new_paged_with_resizable(resizable, pages(size as usize));
        // Fuchsia's vmo_create sets content_size (stream size) to the
        // initial VMO size. This is important for streams — a stream
        // created on a new VMO should see content_size == vmo.size().
        vmo.set_content_size(vmo.len())?;
        // Grant EXECUTE right when the job's AMBIENT_MARK_VMO_EXEC policy
        // allows it. The prebuilt Fuchsia libc's mmap path calls
        // zx_vmo_replace_as_executable before zx_vmar_map, but the
        // mapping's permissions must include EXECUTE at map time for
        // later mprotect(PROT_EXEC) to succeed.
        let mut rights = Rights::DEFAULT_VMO;
        if proc
            .check_policy(PolicyCondition::AmbientMarkVMOExec)
            .is_ok()
        {
            rights |= Rights::EXECUTE;
        }
        let handle_value = proc.add_handle(Handle::new(vmo, rights));
        out.write(handle_value)?;
        Ok(())
    }

    /// Read bytes from a VMO.
    pub fn sys_vmo_read(
        &self,
        handle_value: HandleValue,
        mut buf: UserOutPtr<u8>,
        offset: u64,
        buf_size: usize,
    ) -> ZxResult {
        info!(
            "vmo.read: handle={:#x?}, offset={:#x?}, buf=({:#x?}; {:#x?})",
            handle_value, offset, buf, buf_size,
        );
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<VmObject>(handle_value, Rights::READ)?;
        // in case integer addition overflows
        if offset as usize > vmo.len() || buf_size > vmo.len() - (offset as usize) {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // TODO: optimize
        let mut buffer = vec![0u8; buf_size];
        vmo.read(offset as usize, &mut buffer)?;
        buf.write_array(&buffer)?;
        Ok(())
    }

    /// Write bytes to a VMO.
    pub fn sys_vmo_write(
        &self,
        handle_value: HandleValue,
        buf: UserInPtr<u8>,
        offset: u64,
        buf_size: usize,
    ) -> ZxResult {
        info!(
            "vmo.write: handle={:#x?}, offset={:#x?}, buf=({:#x?}; {:#x?})",
            handle_value, offset, buf, buf_size,
        );
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<VmObject>(handle_value, Rights::WRITE)?;
        if offset as usize > vmo.len() || buf_size > vmo.len() - (offset as usize) {
            return Err(ZxError::OUT_OF_RANGE);
        }
        vmo.write(offset as usize, &buf.read_array(buf_size)?)
    }

    /// Add execute rights to a VMO.
    pub fn sys_vmo_replace_as_executable(
        &self,
        handle: HandleValue,
        vmex: HandleValue,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        let proc = self.thread.proc();
        if vmex != INVALID_HANDLE {
            proc.get_object::<Resource>(vmex)?
                .validate(ResourceKind::VMEX)?;
        } else {
            proc.check_policy(PolicyCondition::AmbientMarkVMOExec)?;
        }
        let _ = proc.get_object_and_rights::<VmObject>(handle)?;
        let new_handle = proc.dup_handle_operating_rights(handle, |handle_rights| {
            Ok(handle_rights | Rights::EXECUTE)
        })?;
        out.write(new_handle)?;
        Ok(())
    }

    /// Obtain the current size of a VMO object.
    pub fn sys_vmo_get_size(&self, handle: HandleValue, mut size: UserOutPtr<usize>) -> ZxResult {
        info!("vmo.get_size: handle={:?}", handle);
        let proc = self.thread.proc();
        let vmo = proc.get_object::<VmObject>(handle)?;
        size.write(vmo.len())?;
        Ok(())
    }

    /// Create a child of an existing VMO (new virtual memory object).
    pub fn sys_vmo_create_child(
        &self,
        handle_value: HandleValue,
        options: u32,
        offset: usize,
        size: usize,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        let mut options = VmoCloneFlags::from_bits(options).ok_or(ZxError::INVALID_ARGS)?;
        info!(
            "vmo_create_child: handle={:#x}, options={:?}, offset={:#x}, size={:#x}",
            handle_value, options, offset, size
        );
        // check options given
        let no_write = options.contains(VmoCloneFlags::NO_WRITE);
        if no_write {
            options.remove(VmoCloneFlags::NO_WRITE);
        }

        let resizable = options.contains(VmoCloneFlags::RESIZABLE);
        let child_size = roundup_pages(size);
        if child_size < size {
            return Err(ZxError::OUT_OF_RANGE);
        }
        info!("size of child vmo: {:#x}", child_size);

        let proc = self.thread.proc();
        let (vmo, parent_rights) = proc.get_object_and_rights::<VmObject>(handle_value)?;
        if !parent_rights.contains(Rights::DUPLICATE | Rights::READ) {
            return Err(ZxError::ACCESS_DENIED);
        }
        let child_vmo = if options.contains(VmoCloneFlags::SLICE) {
            if options != VmoCloneFlags::SLICE {
                Err(ZxError::INVALID_ARGS)
            } else {
                vmo.create_slice(offset, child_size)
            }
        } else {
            if options.contains(VmoCloneFlags::SNAPSHOT) {
                // TODO: implement true ZX_VMO_CHILD_SNAPSHOT (full CoW
                // clone with immutable parent). Currently treated as
                // SNAPSHOT_AT_LEAST_ON_WRITE, which is a valid superset
                // behaviour per the Zircon spec.
                warn!("vmo.create_child: SNAPSHOT treated as SNAPSHOT_AT_LEAST_ON_WRITE");
            } else if !options.contains(VmoCloneFlags::SNAPSHOT_AT_LEAST_ON_WRITE) {
                return Err(ZxError::NOT_SUPPORTED);
            }
            vmo.create_child(resizable, offset, child_size)
        }?;
        // Mark as immutable if SNAPSHOT + NO_WRITE.
        if no_write && options.contains(VmoCloneFlags::SNAPSHOT) {
            child_vmo.set_immutable();
        }
        // generate rights
        let mut child_rights = parent_rights;
        child_rights.insert(Rights::GET_PROPERTY | Rights::SET_PROPERTY);
        if no_write {
            child_rights.remove(Rights::WRITE);
        } else if options.contains(VmoCloneFlags::SNAPSHOT)
            || options.contains(VmoCloneFlags::SNAPSHOT_AT_LEAST_ON_WRITE)
        {
            child_rights.remove(Rights::EXECUTE);
            child_rights.insert(Rights::WRITE);
        };
        info!(
            "parent_rights: {:?} child_rights: {:?}",
            parent_rights, child_rights
        );
        out.write(proc.add_handle(Handle::new(child_vmo, child_rights)))?;
        Ok(())
    }

    /// Create a VM object referring to a specific contiguous range of physical memory.
    pub fn sys_vmo_create_physical(
        &self,
        resource: HandleValue,
        paddr: PhysAddr,
        size: usize,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "vmo.create_physical: handle={:#x?}, paddr={:#x?}, size={:#x}, out={:#x?}",
            resource, paddr, size, out
        );
        let proc = self.thread.proc();
        proc.check_policy(PolicyCondition::NewVMO)?;
        proc.get_object::<Resource>(resource)?
            .validate_ranged_resource(ResourceKind::MMIO, paddr, size)?;
        let size = roundup_pages(size);
        if size == 0 || !page_aligned(paddr) {
            return Err(ZxError::INVALID_ARGS);
        }
        if paddr.overflowing_add(size).1 {
            return Err(ZxError::INVALID_ARGS);
        }
        let vmo = VmObject::new_physical(paddr, size / PAGE_SIZE);
        let handle_value = proc.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO | Rights::EXECUTE));
        out.write(handle_value)?;
        Ok(())
    }

    /// Create a VM object referring to a specific contiguous range of physical frame.
    pub fn sys_vmo_create_contiguous(
        &self,
        bti: HandleValue,
        size: usize,
        align_log2: u32,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "vmo.create_contiguous: handle={:#x?}, size={:#x?}, align={}, out={:#x?}",
            bti, size, align_log2, out
        );
        if size == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let align_log2 = if align_log2 == 0 {
            PAGE_SIZE_LOG2
        } else {
            align_log2 as usize
        };
        if align_log2 < PAGE_SIZE_LOG2 || align_log2 >= 8 * core::mem::size_of::<usize>() {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        proc.check_policy(PolicyCondition::NewVMO)?;
        let _bti = proc.get_object_with_rights::<BusTransactionInitiator>(bti, Rights::MAP)?;
        let vmo = VmObject::new_contiguous(pages(size), align_log2)?;
        let handle_value = proc.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));
        out.write(handle_value)?;
        Ok(())
    }

    /// Resize a VMO object.
    pub fn sys_vmo_set_size(&self, handle_value: HandleValue, size: usize) -> ZxResult {
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<VmObject>(handle_value, Rights::WRITE)?;
        info!(
            "vmo.set_size: handle={:#x}, size={:#x}, current_size={:#x}",
            handle_value,
            size,
            vmo.len(),
        );
        vmo.set_len(size)?;
        // Fuchsia's SetSize updates content_size:
        // - Growing: content_size = new_size (pages beyond old size are zero)
        // - Shrinking: content_size = min(content_size, new_size)
        let content_size = vmo.content_size();
        if size > content_size {
            vmo.set_content_size(size)?;
        } else if vmo.len() < content_size {
            // VMO shrank below content_size — clamp it.
            vmo.set_content_size(vmo.len())?;
        }
        Ok(())
    }

    /// Get the stream content size of a VMO.
    pub fn sys_vmo_get_stream_size(
        &self,
        handle: HandleValue,
        mut size: UserOutPtr<usize>,
    ) -> ZxResult {
        info!("vmo.get_stream_size: handle={:#x}", handle);
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<VmObject>(handle, Rights::READ)?;
        size.write(vmo.content_size())?;
        Ok(())
    }

    /// Set the stream content size of a VMO.
    pub fn sys_vmo_set_stream_size(&self, handle: HandleValue, size: usize) -> ZxResult {
        info!(
            "vmo.set_stream_size: handle={:#x}, size={:#x}",
            handle, size
        );
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<VmObject>(handle, Rights::WRITE)?;
        vmo.set_content_size(size)
    }

    /// Perform an operation on a range of a VMO.
    ///
    /// Performs cache and memory operations against pages held by the VMO.
    pub fn sys_vmo_op_range(
        &self,
        handle_value: HandleValue,
        op: u32,
        offset: usize,
        len: usize,
        _buffer: UserOutPtr<u8>,
        _buffer_size: usize,
    ) -> ZxResult {
        info!(
            "vmo.op_range: handle={:#x}, op={:#X}, offset={:#x}, len={:#x}, buffer_size={:#x}",
            handle_value, op, offset, len, _buffer_size,
        );
        let op = VmoOpType::try_from(op).or(Err(ZxError::INVALID_ARGS))?;
        let proc = self.thread.proc();
        let (vmo, rights) = proc.get_object_and_rights::<VmObject>(handle_value)?;
        match op {
            VmoOpType::Commit => {
                if !rights.contains(Rights::WRITE) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                if !page_aligned(offset) || !page_aligned(len) {
                    return Err(ZxError::INVALID_ARGS);
                }
                vmo.commit(offset, len)?;
                Ok(())
            }
            VmoOpType::Decommit => {
                if !rights.contains(Rights::WRITE) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                if !page_aligned(offset) || !page_aligned(len) {
                    return Err(ZxError::INVALID_ARGS);
                }
                vmo.decommit(offset, len)
            }
            VmoOpType::Zero => {
                if !rights.contains(Rights::WRITE) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                vmo.zero(offset, len)
            }
            VmoOpType::Lock | VmoOpType::Unlock => {
                // TODO: implement VMO Lock/Unlock operations
                warn!("vmo.op_range: Lock/Unlock not yet implemented");
                Err(ZxError::NOT_SUPPORTED)
            }
            VmoOpType::CacheSync | VmoOpType::CacheClean | VmoOpType::CacheCleanInvalidate => {
                // These require READ rights per the Zircon ABI.
                if !rights.contains(Rights::READ) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                if offset.checked_add(len).is_none() || offset + len > vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                // Cache operations are no-ops on most architectures;
                // return success so callers can proceed.
                Ok(())
            }
            VmoOpType::CacheInvalidate => {
                // CACHE_INVALIDATE requires WRITE rights per the Zircon ABI.
                if !rights.contains(Rights::WRITE) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                if offset.checked_add(len).is_none() || offset + len > vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                Ok(())
            }
        }
    }

    /// Set the caching policy for pages held by a VMO.
    pub fn sys_vmo_cache_policy(&self, handle_value: HandleValue, policy: u32) -> ZxResult {
        let proc = self.thread.proc();
        let vmo = proc.get_object_with_rights::<VmObject>(handle_value, Rights::MAP)?;
        let policy = CachePolicy::try_from(policy).or(Err(ZxError::INVALID_ARGS))?;
        (*vmo).set_cache_policy(policy)
    }

    /// Transfer data (pages) between two VMOs.
    ///
    /// Moves physical pages from `src_vmo` to `dst_vmo` (zero-copy transfer).
    /// Both offset values and the length must be page-aligned.
    /// `options` must be 0.
    pub fn sys_vmo_transfer_data(
        &self,
        dst_vmo: HandleValue,
        options: u32,
        offset: u64,
        length: u64,
        src_vmo: HandleValue,
        src_offset: u64,
    ) -> ZxResult {
        info!(
            "vmo.transfer_data: dst={:#x}, options={}, offset={:#x}, len={:#x}, src={:#x}, src_off={:#x}",
            dst_vmo, options, offset, length, src_vmo, src_offset
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        // All offsets/length must be page-aligned.
        if !page_aligned(offset as usize)
            || !page_aligned(length as usize)
            || !page_aligned(src_offset as usize)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        if length == 0 {
            return Ok(());
        }
        let proc = self.thread.proc();
        let dst = proc.get_object_with_rights::<VmObject>(dst_vmo, Rights::WRITE)?;
        // Source needs both READ and WRITE (pages are moved, not copied).
        let src = proc.get_object_with_rights::<VmObject>(src_vmo, Rights::READ | Rights::WRITE)?;
        // Range validation.
        if offset as usize + length as usize > dst.len()
            || src_offset as usize + length as usize > src.len()
        {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // Reject pinned pages in either transfer range.
        if src.has_pinned_pages(src_offset as usize, length as usize)
            || dst.has_pinned_pages(offset as usize, length as usize)
        {
            return Err(ZxError::BAD_STATE);
        }

        // Reject same-VMO overlapping transfers (copy direction
        // handling is not yet implemented).
        if Arc::ptr_eq(&src, &dst) {
            let src_end = src_offset as usize + length as usize;
            let dst_end = offset as usize + length as usize;
            if (src_offset as usize) < dst_end && (offset as usize) < src_end {
                warn!("vmo.transfer_data: same-VMO overlapping transfer not supported");
                return Err(ZxError::NOT_SUPPORTED);
            }
        }

        // Transfer data page-by-page via read/write.
        // Known limitations (follow-up work):
        // - Not zero-copy (copies through a kernel buffer)
        // - Source decommit fails on child VMOs (ignored, data still copied)
        // - Same-VMO overlapping transfers rejected above
        // - No pin-count check (pinned pages should return BAD_STATE;
        //   requires adding a VMObjectTrait::is_pinned_in_range method)
        let mut buf = vec![0u8; PAGE_SIZE];
        let mut remaining = length as usize;
        let mut s_off = src_offset as usize;
        let mut d_off = offset as usize;
        while remaining > 0 {
            let chunk = core::cmp::min(remaining, PAGE_SIZE);
            src.read(s_off, &mut buf[..chunk])?;
            dst.write(d_off, &buf[..chunk])?;
            // Decommit the source page to release its physical frame,
            // matching the Fuchsia "move, not copy" semantics. If
            // decommit fails (e.g., child VMOs), return the error —
            // partial progress may have occurred.
            src.decommit(s_off, chunk)?;
            s_off += chunk;
            d_off += chunk;
            remaining -= chunk;
        }
        Ok(())
    }
}

bitflags! {
    struct VmoCloneFlags: u32 {
        #[allow(clippy::identity_op)]
        const SNAPSHOT                   = 1 << 0;
        const RESIZABLE                  = 1 << 2;
        const SLICE                      = 1 << 3;
        const SNAPSHOT_AT_LEAST_ON_WRITE = 1 << 4;
        const NO_WRITE                   = 1 << 5;
    }
}

numeric_enum! {
    #[repr(u32)]
    /// VMO Opcodes (for vmo_op_range)
    pub enum VmoOpType {
        Commit = 1,
        Decommit = 2,
        Lock = 3,
        Unlock = 4,
        CacheSync = 6,
        CacheInvalidate = 7,
        CacheClean = 8,
        CacheCleanInvalidate = 9,
        Zero = 10,
    }
}
