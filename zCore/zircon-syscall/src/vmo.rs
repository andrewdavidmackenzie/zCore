use {
    super::*,
    bitflags::bitflags,
    core::time::Duration,
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
        // Accept known VMO creation flags.
        const ZX_VMO_RESIZABLE: u32 = 1 << 1; // 0x02
        const ZX_VMO_DISCARDABLE: u32 = 1 << 2; // 0x04
        const ZX_VMO_UNBOUNDED: u32 = 1 << 4; // 0x10 (matches Fuchsia ABI)
        if options & !(ZX_VMO_RESIZABLE | ZX_VMO_DISCARDABLE | ZX_VMO_UNBOUNDED) != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let resizable = options & ZX_VMO_RESIZABLE != 0;
        let unbounded = options & ZX_VMO_UNBOUNDED != 0;
        // UNBOUNDED and RESIZABLE are mutually exclusive.
        if unbounded && resizable {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let size_usize = size as usize;
        let rounded = roundup_pages(size_usize);
        // Reject sizes that overflow when rounded up to page boundary.
        if rounded < size_usize {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // UNBOUNDED VMOs have maximum capacity; the create size is
        // only the initial content_size (stream size).
        let vmo_pages = if unbounded {
            // Use a large but finite capacity (256K pages = 1GB).
            // Pages are allocated lazily on demand.
            256 * 1024
        } else {
            pages(size_usize)
        };
        let vmo = VmObject::new_paged_with_resizable(resizable, vmo_pages);
        if options & ZX_VMO_DISCARDABLE != 0 {
            vmo.set_discardable();
        }
        // Set content_size to the exact user-requested size.
        vmo.set_content_size(size_usize)?;
        // Default VMO rights do not include EXECUTE. The caller must
        // use zx_vmo_replace_as_executable to add EXECUTE rights.
        // RESIZE right is only granted when the VMO is resizable.
        let mut vmo_rights = Rights::DEFAULT_VMO;
        if resizable {
            vmo_rights.insert(Rights::RESIZE);
        }
        let handle_value = proc.add_handle(Handle::new(vmo, vmo_rights));
        out.write(handle_value)?;
        Ok(())
    }

    /// Read bytes from a VMO.
    ///
    /// For pager-backed VMOs, this will demand-page any missing pages
    /// by sending `ZX_PAGER_VMO_READ` requests to the pager's port
    /// and waiting for the pager to supply the pages.
    pub async fn sys_vmo_read(
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
        // Check VMO bounds first — Fuchsia returns OUT_OF_RANGE for
        // reads beyond the VMO size, even if the buffer is also bad.
        if offset as usize > vmo.len() || buf_size > vmo.len() - (offset as usize) {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // Fuchsia returns NOT_FOUND for null, kernel-space, or
        // boundary-spanning user buffers.
        if buf_size > 0 && buf.is_null() {
            return Err(ZxError::NOT_FOUND);
        }
        if buf.as_addr() >= zircon_object::vm::KERNEL_ASPACE_BASE as usize {
            return Err(ZxError::NOT_FOUND);
        }
        match buf.as_addr().checked_add(buf_size) {
            Some(end) if end > zircon_object::vm::KERNEL_ASPACE_BASE as usize => {
                return Err(ZxError::NOT_FOUND);
            }
            None => return Err(ZxError::NOT_FOUND),
            _ => {}
        }
        // For pager-backed VMOs, interleave demand-paging with data copy.
        // Send one batched request for all uncommitted pages, then wait
        // and copy page by page. This allows "early wake" — data becomes
        // available in the user buffer as pages are supplied.
        if (vmo.is_pager_backed() || vmo.is_in_pager_tree()) && buf_size > 0 {
            use zircon_object::task::ThreadState;
            use zircon_object::vm::PAGE_SIZE;
            let start = offset as usize;
            let end = start + buf_size;
            let start_page = start / PAGE_SIZE;
            let end_page = end.div_ceil(PAGE_SIZE);

            // For clones in a pager tree, route requests to the root
            // pager VMO. For direct pager VMOs, use the VMO itself.
            let pager_vmo_opt = if vmo.is_pager_backed() {
                Some(vmo.clone())
            } else {
                vmo.find_root_pager_vmo()
            };
            // If we can't find a pager VMO, fall through to the normal read.
            let Some(pager_vmo) = pager_vmo_opt else {
                // fall through to non-pager read below
                let mut vmo_offset2 = offset as usize;
                buf.for_each_chunk_mut::<ZxError>(buf_size, |chunk| {
                    vmo.read(vmo_offset2, chunk)?;
                    let n = chunk.len();
                    vmo_offset2 += n;
                    Ok(n)
                })
                .map_err(|e| {
                    if e == ZxError::INVALID_ARGS {
                        ZxError::ACCESS_DENIED
                    } else {
                        e
                    }
                })?;
                return Ok(());
            };

            // Find all uncommitted pages and send one batched request.
            // Check the pager VMO for committed pages (supply goes there).
            let first_uncommitted =
                (start_page..end_page).find(|&p| pager_vmo.committed_pages_in_range(p, p + 1) == 0);
            let last_uncommitted = (start_page..end_page)
                .rfind(|&p| pager_vmo.committed_pages_in_range(p, p + 1) == 0);

            let needs_pager = first_uncommitted.is_some();
            if needs_pager {
                let req_start = first_uncommitted.unwrap();
                let req_end = last_uncommitted.unwrap() + 1;
                let req_offset = req_start * PAGE_SIZE;
                let req_length = (req_end - req_start) * PAGE_SIZE;
                match pager_vmo.request_pages(req_offset, req_length) {
                    Err(ZxError::SHOULD_WAIT) => {} // normal
                    Err(e) => return Err(e),
                    Ok(()) => {}
                }
            }

            // Read page by page, waiting for each page as needed.
            // Data is copied to user buffer as soon as each page is ready.
            let mut vmo_offset = start;
            let mut buf_written = 0usize;
            for page_idx in start_page..end_page {
                // Wait for this page if uncommitted in the pager VMO.
                // Retry on pager errors (IO, etc.) — the kernel re-sends
                // a request and waits again, matching Fuchsia's behavior.
                if pager_vmo.committed_pages_in_range(page_idx, page_idx + 1) == 0 {
                    let mut last_err = None;
                    // Try twice: initial wait (from batched request), then
                    // one re-request after failure (matching Fuchsia kernel
                    // behavior for vmo_read partial failure).
                    for retry in 0..2 {
                        if retry > 0 {
                            // After failure, clear the marker and re-send.
                            pager_vmo.clear_failed_page(page_idx);
                            let _ = pager_vmo.request_pages(page_idx * PAGE_SIZE, PAGE_SIZE);
                        }
                        let future = pager_vmo.wait_for_page(page_idx * PAGE_SIZE);
                        pin_mut!(future);
                        self.thread.set_blocking_state(ThreadState::BlockedPager);
                        let result = self
                            .thread
                            .blocking_run_suspendable(
                                future,
                                ThreadState::BlockedPager,
                                Duration::from_nanos(u64::MAX),
                            )
                            .await;
                        match result {
                            Ok(()) => {
                                last_err = None;
                                break;
                            }
                            Err(ZxError::STOP) | Err(ZxError::CANCELED) => {
                                return Err(ZxError::STOP);
                            }
                            Err(ZxError::OUT_OF_RANGE) => return Err(ZxError::OUT_OF_RANGE),
                            Err(ZxError::BAD_STATE) => return Err(ZxError::BAD_STATE),
                            Err(e) => {
                                last_err = Some(e);
                                // Retry — re-send request
                            }
                        }
                    }
                    if let Some(e) = last_err {
                        return Err(e);
                    }
                }
                // Copy this page's data to user buffer.
                let page_start = page_idx * PAGE_SIZE;
                let page_end = (page_idx + 1) * PAGE_SIZE;
                let copy_start = vmo_offset.max(page_start);
                let copy_end = end.min(page_end);
                if copy_start < copy_end {
                    let chunk_len = copy_end - copy_start;
                    let mut tmp = alloc::vec![0u8; chunk_len];
                    vmo.read(copy_start, &mut tmp)?;
                    let mut dst = buf.add(buf_written);
                    dst.write_array(&tmp).map_err(|_| ZxError::ACCESS_DENIED)?;
                    buf_written += chunk_len;
                    vmo_offset = copy_end;
                }
            }
            return Ok(());
        }
        // Non-pager VMO: read all at once.
        let mut vmo_offset = offset as usize;
        buf.for_each_chunk_mut::<ZxError>(buf_size, |chunk| {
            vmo.read(vmo_offset, chunk)?;
            let n = chunk.len();
            vmo_offset += n;
            Ok(n)
        })
        .map_err(|e| {
            if e == ZxError::INVALID_ARGS {
                ZxError::ACCESS_DENIED
            } else {
                e
            }
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
        // Check VMO bounds first.
        if offset as usize > vmo.len() || buf_size > vmo.len() - (offset as usize) {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if buf_size > 0 && buf.is_null() {
            return Err(ZxError::NOT_FOUND);
        }
        if buf.as_addr() >= zircon_object::vm::KERNEL_ASPACE_BASE as usize {
            return Err(ZxError::NOT_FOUND);
        }
        match buf.as_addr().checked_add(buf_size) {
            Some(end) if end > zircon_object::vm::KERNEL_ASPACE_BASE as usize => {
                return Err(ZxError::NOT_FOUND);
            }
            None => return Err(ZxError::NOT_FOUND),
            _ => {}
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
        })
        .map_err(|e| {
            if e == ZxError::INVALID_ARGS {
                ZxError::NOT_FOUND
            } else {
                e
            }
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
                // Wrong kind → WRONG_TYPE; right kind wrong sub-base → OUT_OF_RANGE.
                res.validate(ResourceKind::SYSTEM)
                    .map_err(|_| ZxError::WRONG_TYPE)?;
                res.validate_ranged_resource(
                    ResourceKind::SYSTEM,
                    zircon_object::dev::ZX_RSRC_SYSTEM_VMEX_BASE,
                    1,
                )
                .map_err(|_| ZxError::OUT_OF_RANGE)?;
            }
        } else {
            proc.check_policy(PolicyCondition::AmbientMarkVMOExec)?;
        }
        let _ = proc.get_object_and_rights::<VmObject>(handle)?;
        // Replace: create new handle with EXECUTE, remove old handle.
        let new_handle = proc.dup_handle_operating_rights(handle, |handle_rights| {
            Ok(handle_rights | Rights::EXECUTE)
        })?;
        proc.remove_handle(handle)?;
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
        // NO_WRITE + RESIZABLE is not allowed for snapshot children.
        if no_write && resizable {
            return Err(ZxError::INVALID_ARGS);
        }
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
        // Discardable VMOs cannot have children.
        if vmo.is_discardable() {
            return Err(ZxError::NOT_SUPPORTED);
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
                // SNAPSHOT is not supported on pager-backed VMOs
                // or any VMO in a pager-backed clone tree.
                if vmo.is_in_pager_tree() {
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
            } else if vmo.is_slice_vmo() {
                // SLICE VMOs: create the COW child on the slice's parent,
                // adjusting the offset by the slice's parent_offset.
                // This matches Fuchsia behavior where SNAPSHOT_AT_LEAST_ON_WRITE
                // on a SLICE creates the child in the parent's COW tree.
                if let Some(parent) = vmo.parent() {
                    let end = offset
                        .checked_add(child_size)
                        .ok_or(ZxError::OUT_OF_RANGE)?;
                    if end > vmo.len() {
                        return Err(ZxError::OUT_OF_RANGE);
                    }
                    let adjusted_offset = offset
                        .checked_add(vmo.parent_offset())
                        .ok_or(ZxError::OUT_OF_RANGE)?;
                    let child = parent.create_child(resizable, adjusted_offset, child_size)?;
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
        // Grant RESIZE only if the child is resizable.
        if resizable {
            child_rights.insert(Rights::RESIZE);
        } else {
            child_rights.remove(Rights::RESIZE);
        }
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
        let rsrc = proc.get_resource(resource)?;
        // Validate kind (MMIO) — ROOT bypasses this check.
        rsrc.validate(ResourceKind::MMIO)
            .map_err(|_| ZxError::ACCESS_DENIED)?;
        if size == 0 || !page_aligned(size) || !page_aligned(paddr) {
            return Err(ZxError::INVALID_ARGS);
        }
        if paddr.overflowing_add(size).1 {
            return Err(ZxError::INVALID_ARGS);
        }
        // For non-root resources, validate that [paddr, paddr+size) fits
        // within the page-rounded bounds of the resource. Fuchsia's kernel
        // rounds the resource range to page boundaries for VMO creation
        // (unlike sys_resource_create which uses strict byte-level checks).
        // ROOT and ranged-root resources bypass range checks entirely.
        if !rsrc.is_ranged_root(ResourceKind::MMIO) {
            let rsrc_base = rsrc.addr();
            let rsrc_size = rsrc.len();
            let rsrc_page_base = rsrc_base & !(PAGE_SIZE - 1);
            let rsrc_page_end = rsrc_base
                .saturating_add(rsrc_size)
                .saturating_add(PAGE_SIZE - 1)
                & !(PAGE_SIZE - 1);
            if paddr < rsrc_page_base || paddr + size > rsrc_page_end {
                return Err(ZxError::OUT_OF_RANGE);
            }
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
        if size == 0 || !page_aligned(size) {
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
        let (vmo, rights) = proc.get_object_and_rights::<VmObject>(handle_value)?;
        // Check resizability FIRST — non-resizable VMOs always return
        // UNAVAILABLE regardless of handle rights.
        if !vmo.is_resizable() {
            return Err(ZxError::UNAVAILABLE);
        }
        // Then check rights — need both WRITE and RESIZE.
        if !rights.contains(Rights::WRITE | Rights::RESIZE) {
            return Err(ZxError::ACCESS_DENIED);
        }
        info!(
            "vmo.set_size: handle={:#x}, size={:#x}, current_size={:#x}",
            handle_value,
            size,
            vmo.len(),
        );
        vmo.set_len(size)?;
        // Fuchsia's SetSize sets content_size to the unrounded request.
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
        let vmo = proc.get_object::<VmObject>(handle)?;
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
        // Physical and contiguous VMOs don't support stream size.
        if !vmo.is_paged() || vmo.is_contiguous() {
            return Err(ZxError::NOT_SUPPORTED);
        }
        if size > vmo.len() {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // When changing stream_size, zero the range between the old
        // and new sizes. On decrease: zero [new, vmo_len). On
        // increase: zero [old, new) so data beyond old stream_size
        // reads as zero even if previously written.
        let old_size = vmo.content_size();
        if size != old_size {
            let zero_start = size.min(old_size);
            let zero_end = if size < old_size { vmo.len() } else { size };
            if zero_end > zero_start {
                let _ = vmo.zero(zero_start, zero_end - zero_start);
            }
        }
        vmo.set_content_size(size)
    }

    /// Perform an operation on a range of a VMO.
    ///
    /// Performs cache and memory operations against pages held by the VMO.
    /// Async wrapper for sys_vmo_op_range that demand-pages pager-backed
    /// VMOs on COMMIT. Other ops delegate to the sync version.
    pub async fn sys_vmo_op_range_async(
        &self,
        handle_value: HandleValue,
        op: u32,
        offset: usize,
        len: usize,
        buffer: UserOutPtr<u8>,
        buffer_size: usize,
    ) -> ZxResult {
        // ZX_VMO_OP_COMMIT = 1, ZX_VMO_OP_PREFETCH = 14, ZX_VMO_OP_ALWAYS_NEED = 13
        // All trigger demand-paging for pager-backed VMOs (and clones).
        if op == 1 || op == 14 || op == 13 {
            let proc = self.thread.proc();
            let is_pager = proc
                .get_object::<VmObject>(handle_value)
                .is_ok_and(|v| v.is_pager_backed() || v.is_in_pager_tree());
            if is_pager {
                let (vmo, rights) = proc.get_object_and_rights::<VmObject>(handle_value)?;
                // COMMIT requires WRITE; PREFETCH/ALWAYS_NEED only require READ.
                let required = if op == 1 { Rights::WRITE } else { Rights::READ };
                if !rights.contains(required) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                if offset.checked_add(len).is_none() || offset + len > vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                // For clones, route requests to the root pager VMO.
                let pager_vmo = if vmo.is_pager_backed() {
                    vmo.clone()
                } else {
                    vmo.find_root_pager_vmo().unwrap_or_else(|| vmo.clone())
                };
                let start = offset & !(PAGE_SIZE - 1);
                let end = (offset + len + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
                let start_page = start / PAGE_SIZE;
                let end_page = end / PAGE_SIZE;
                // Send separate requests per consecutive uncommitted run,
                // then wait page-by-page. Fuchsia splits requests around
                // already-committed pages.
                {
                    use zircon_object::task::ThreadState;
                    let mut p = start_page;
                    while p < end_page {
                        if p >= pager_vmo.len() / PAGE_SIZE {
                            break;
                        }
                        // Skip committed pages and pages with pending waiters
                        // (another thread already requested them).
                        if pager_vmo.committed_pages_in_range(p, p + 1) > 0
                            || pager_vmo.has_page_waiter(p)
                        {
                            p += 1;
                            continue;
                        }
                        let run_start = p;
                        while p < end_page
                            && p < pager_vmo.len() / PAGE_SIZE
                            && pager_vmo.committed_pages_in_range(p, p + 1) == 0
                            && !pager_vmo.has_page_waiter(p)
                        {
                            p += 1;
                        }
                        let req_off = run_start * PAGE_SIZE;
                        let req_len = (p - run_start) * PAGE_SIZE;
                        match pager_vmo.request_pages(req_off, req_len) {
                            Err(ZxError::SHOULD_WAIT) => {
                                for pg in run_start..p {
                                    if pg >= pager_vmo.len() / PAGE_SIZE {
                                        break;
                                    }
                                    if pager_vmo.committed_pages_in_range(pg, pg + 1) == 0 {
                                        let future = pager_vmo.wait_for_page(pg * PAGE_SIZE);
                                        pin_mut!(future);
                                        self.thread
                                            .set_blocking_state(ThreadState::BlockedPager);
                                        match self
                                            .thread
                                            .blocking_run_suspendable(
                                                future,
                                                ThreadState::BlockedPager,
                                                Duration::from_nanos(u64::MAX),
                                            )
                                            .await
                                        {
                                            Ok(()) => {}
                                            Err(ZxError::STOP) | Err(ZxError::CANCELED) => {
                                                return Err(ZxError::STOP);
                                            }
                                            Err(ZxError::OUT_OF_RANGE) => continue,
                                            Err(e) => return Err(e),
                                        }
                                    }
                                }
                            }
                            Err(e) => return Err(e),
                            Ok(()) => {}
                        }
                    }
                }
                return Ok(());
            }
        }
        self.sys_vmo_op_range(handle_value, op, offset, len, buffer, buffer_size)
    }

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
        // Check for offset+len overflow or beyond VMO size.
        if offset.checked_add(len).is_none() || offset + len > vmo.len() {
            return Err(ZxError::OUT_OF_RANGE);
        }
        match op {
            VmoOpType::Commit => {
                if !rights.contains(Rights::WRITE) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                // Fuchsia rounds commit range to page boundaries.
                let page_mask = PAGE_SIZE - 1;
                let aligned_offset = offset & !page_mask;
                let aligned_end = (offset + len + page_mask) & !page_mask;
                let aligned_len = aligned_end - aligned_offset;
                vmo.commit(aligned_offset, aligned_len)?;
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
            VmoOpType::Lock | VmoOpType::TryLock => {
                if !vmo.is_discardable() {
                    return Err(ZxError::NOT_SUPPORTED);
                }
                if !rights.intersects(Rights::READ | Rights::WRITE) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                // Lock/Unlock must cover the entire VMO.
                if offset != 0 || len != vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                if matches!(op, VmoOpType::Lock) && (_buffer_size < 32 || _buffer.is_null()) {
                    return Err(ZxError::INVALID_ARGS);
                }
                let is_try = matches!(op, VmoOpType::TryLock);
                vmo.lock_discardable_ext(is_try)?;
                // Write zx_vmo_lock_state_t to buffer (LOCK only, not TRY_LOCK):
                if matches!(op, VmoOpType::Lock) {
                    let mut buf = _buffer;
                    let mut state = [0u8; 32];
                    state[0..8].copy_from_slice(&(offset as u64).to_le_bytes());
                    state[8..16].copy_from_slice(&(len as u64).to_le_bytes());
                    if let Err(e) = buf.write_array(&state) {
                        // Undo the lock if the output write fails.
                        let _ = vmo.unlock_discardable();
                        return Err(e.into());
                    }
                }
                Ok(())
            }
            VmoOpType::Unlock => {
                if !vmo.is_discardable() {
                    return Err(ZxError::NOT_SUPPORTED);
                }
                if !rights.intersects(Rights::READ | Rights::WRITE) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                // Unlock must cover the entire VMO.
                if offset != 0 || len != vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                vmo.unlock_discardable()
            }
            VmoOpType::CacheSync | VmoOpType::CacheClean | VmoOpType::CacheCleanInvalidate => {
                // Physical VMOs do not support kernel-managed cache ops.
                if vmo.is_physical() {
                    return Err(ZxError::NOT_SUPPORTED);
                }
                if !rights.contains(Rights::READ) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                if len == 0 {
                    return Err(ZxError::INVALID_ARGS);
                }
                if offset.checked_add(len).is_none() || offset + len > vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                Ok(())
            }
            VmoOpType::CacheInvalidate => {
                if vmo.is_physical() {
                    return Err(ZxError::NOT_SUPPORTED);
                }
                if !rights.contains(Rights::WRITE) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                if len == 0 {
                    return Err(ZxError::INVALID_ARGS);
                }
                if offset.checked_add(len).is_none() || offset + len > vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                Ok(())
            }
            VmoOpType::AlwaysNeed | VmoOpType::DontNeed => {
                // Hinting operations. Require READ rights and valid range.
                if !rights.contains(Rights::READ) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                let end = offset.checked_add(len).ok_or(ZxError::OUT_OF_RANGE)?;
                if end > vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                // No-op: we don't implement eviction hints.
                Ok(())
            }
            VmoOpType::Prefetch => {
                // Prefetch: commit pages eagerly. For pager-backed VMOs
                // this triggers demand-paging via the async path in
                // sys_vmo_op_range_async (same as COMMIT).
                // For non-pager VMOs, commit pages synchronously.
                if !rights.contains(Rights::READ) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                let end = offset.checked_add(len).ok_or(ZxError::OUT_OF_RANGE)?;
                if end > vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                vmo.commit(offset, len)?;
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
        // Physical, contiguous, and pager-backed VMOs cannot participate
        // in transfer_data.
        if src.is_physical()
            || dst.is_physical()
            || src.is_contiguous()
            || dst.is_contiguous()
            || src.is_pager_backed()
            || dst.is_pager_backed()
        {
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
                // Release the source page. Decommit frees the physical
                // frame for root VMOs. For COW children, decommit may
                // succeed but the page is still readable from the parent
                // chain. Zero the range to ensure the source reads as
                // zeros after transfer, matching Fuchsia's "move" semantics.
                let _ = src.decommit(s_off, chunk);
                let _ = src.zero(s_off, chunk);
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
        TryLock = 11,
        DontNeed = 12,
        AlwaysNeed = 13,
        Prefetch = 14,
    }
}
