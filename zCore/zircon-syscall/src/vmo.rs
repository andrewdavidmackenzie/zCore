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
        // Default VMO rights do not include EXECUTE. The caller must
        // use zx_vmo_replace_as_executable to add EXECUTE rights.
        let handle_value = proc.add_handle(Handle::new(vmo, Rights::DEFAULT_VMO));
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
        // Chunked VMO read: read page-sized chunks from the VMO and
        // copy each chunk to user space, avoiding a single huge kernel
        // allocation for the entire read.
        let mut vmo_offset = offset as usize;
        buf.for_each_chunk_mut::<ZxError>(buf_size, |chunk| {
            vmo.read(vmo_offset, chunk)?;
            let n = chunk.len();
            vmo_offset += n;
            Ok(n)
        })?;
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
        // Chunked VMO write: copy user data in page-sized chunks and
        // write each chunk to the VMO, avoiding a single huge kernel
        // allocation for the entire write.
        let mut vmo_offset = offset as usize;
        buf.for_each_chunk::<ZxError>(buf_size, |chunk| {
            vmo.write(vmo_offset, chunk)?;
            let n = chunk.len();
            vmo_offset += n;
            Ok(n)
        })?;
        Ok(())
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
            let res = proc.get_resource(vmex)?;
            if res.validate(ResourceKind::ROOT).is_err() {
                res.validate_ranged_resource(
                    ResourceKind::SYSTEM,
                    zircon_object::dev::ZX_RSRC_SYSTEM_VMEX_BASE,
                    1,
                )?;
            }
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
        let child_vmo = if options.contains(VmoCloneFlags::REFERENCE) {
            // A reference child is a transparent alias that shares
            // pages with the parent. offset and size must both be 0.
            if offset != 0 || size != 0 {
                return Err(ZxError::INVALID_ARGS);
            }
            // Resizable REFERENCE requires a resizable parent AND
            // the parent handle must have RESIZE right.
            if resizable && (!vmo.is_resizable() || !parent_rights.contains(Rights::RESIZE)) {
                return Err(ZxError::ACCESS_DENIED);
            }
            let mut remaining = options - VmoCloneFlags::REFERENCE;
            if no_write {
                remaining -= VmoCloneFlags::NO_WRITE;
            }
            if resizable {
                remaining -= VmoCloneFlags::RESIZABLE;
            }
            if !remaining.is_empty() {
                return Err(ZxError::INVALID_ARGS);
            }
            // Implement as a slice over the entire VMO.
            vmo.create_reference_slice(0, vmo.len(), resizable)
        } else if options.contains(VmoCloneFlags::SLICE) {
            if options != VmoCloneFlags::SLICE {
                Err(ZxError::INVALID_ARGS)
            } else {
                vmo.create_slice(offset, child_size)
            }
        } else {
            if options.contains(VmoCloneFlags::SNAPSHOT) {
                // SNAPSHOT is not supported on pager-backed VMOs.
                if vmo.has_pager() {
                    return Err(ZxError::NOT_SUPPORTED);
                }
                // TODO: implement true ZX_VMO_CHILD_SNAPSHOT (full CoW
                // clone with immutable parent). Currently treated as
                // SNAPSHOT_AT_LEAST_ON_WRITE for non-pager VMOs.
                warn!("vmo.create_child: SNAPSHOT treated as SNAPSHOT_AT_LEAST_ON_WRITE");
            } else if !options.contains(VmoCloneFlags::SNAPSHOT_AT_LEAST_ON_WRITE)
                && !options.contains(VmoCloneFlags::SNAPSHOT_MODIFIED)
            {
                return Err(ZxError::NOT_SUPPORTED);
            }
            // If the VMO is a REFERENCE (transparent alias), create
            // the child on the parent VMO instead. This ensures the
            // COW tree is built on the real VMO, not the alias.
            // The child's parent_koid should point to the reference,
            // not the underlying parent (matching Fuchsia semantics).
            if vmo.is_reference() {
                if let Some(parent) = vmo.parent() {
                    let child = parent.create_child(resizable, offset, child_size)?;
                    child.set_parent_ref(&vmo);
                    Ok(child)
                } else {
                    Err(ZxError::BAD_STATE)
                }
            } else {
                vmo.create_child(resizable, offset, child_size)
            }
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
        } else if options.contains(VmoCloneFlags::REFERENCE) {
            // Reference children inherit parent rights.
        } else if options.contains(VmoCloneFlags::SNAPSHOT)
            || options.contains(VmoCloneFlags::SNAPSHOT_AT_LEAST_ON_WRITE)
            || options.contains(VmoCloneFlags::SNAPSHOT_MODIFIED)
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
        proc.get_resource(resource)?
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
        // Fuchsia's SetSize always updates content_size to the
        // user-requested size (which may be non-page-aligned).
        vmo.set_content_size(size)?;
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
        // Pager-backed VMOs cannot participate in transfer_data.
        if src.is_pager_backed() || dst.is_pager_backed() {
            return Err(ZxError::NOT_SUPPORTED);
        }
        // Reject pinned pages in either transfer range.
        if src.has_pinned_pages(src_offset as usize, length as usize)
            || dst.has_pinned_pages(offset as usize, length as usize)
        {
            return Err(ZxError::BAD_STATE);
        }

        // Transfer data page-by-page via read/write.
        // Known limitations (follow-up work):
        // - Not zero-copy (copies through a kernel buffer)
        // - Source decommit fails on child VMOs (ignored below)
        // - No pin-count check (pinned pages should return BAD_STATE;
        //   requires adding a VMObjectTrait::is_pinned_in_range method)
        // - Aliasing via slice/reference children is not detected:
        //   Arc::ptr_eq only catches the exact same VmObject, not
        //   slice/reference children that share underlying pages.
        //   A complete fix needs VmObject::root_vmo_id() comparison.
        let same_vmo = Arc::ptr_eq(&src, &dst);
        if same_vmo {
            // Same-VMO transfer: read all source data first, then write
            // to the destination and decommit. This handles overlapping
            // ranges correctly.
            let len = length as usize;
            let mut buf = {
                let mut v = alloc::vec::Vec::new();
                v.try_reserve(len).map_err(|_| ZxError::INVALID_ARGS)?;
                v.resize(len, 0u8);
                v
            };
            src.read(src_offset as usize, &mut buf)?;
            dst.write(offset as usize, &buf)?;
            // Decommit source pages. For same-VMO, only decommit pages
            // that don't overlap with the destination range.
            let s_start = src_offset as usize;
            let s_end = s_start + len;
            let d_start = offset as usize;
            let d_end = d_start + len;
            // Non-overlapping part before destination range.
            if s_start < d_start {
                let _ = src.decommit(s_start, core::cmp::min(d_start, s_end) - s_start);
            }
            // Non-overlapping part after destination range.
            if s_end > d_end {
                let decommit_start = core::cmp::max(d_end, s_start);
                let _ = src.decommit(decommit_start, s_end - decommit_start);
            }
        } else {
            let mut buf = vec![0u8; PAGE_SIZE];
            let mut remaining = length as usize;
            let mut s_off = src_offset as usize;
            let mut d_off = offset as usize;
            while remaining > 0 {
                let chunk = core::cmp::min(remaining, PAGE_SIZE);
                src.read(s_off, &mut buf[..chunk])?;
                dst.write(d_off, &buf[..chunk])?;
                // Decommit the source page to release its physical frame,
                // matching the Fuchsia "move, not copy" semantics.
                // Ignore decommit errors for child VMOs.
                let _ = src.decommit(s_off, chunk);
                s_off += chunk;
                d_off += chunk;
                remaining -= chunk;
            }
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
        const REFERENCE                  = 1 << 6;
        const SNAPSHOT_MODIFIED          = 1 << 7;
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
