use {super::*, bitflags::bitflags, zircon_object::vm::*};

fn amount_of_alignments(options: u32) -> ZxResult<usize> {
    let mut align_pow2 = (options >> 24) as usize;
    if align_pow2 == 0 {
        align_pow2 = PAGE_SIZE_LOG2;
    }
    if !(PAGE_SIZE_LOG2..=32).contains(&align_pow2) {
        Err(ZxError::INVALID_ARGS)
    } else {
        Ok(1 << align_pow2)
    }
}

impl Syscall<'_> {
    /// Allocate a new subregion.
    ///
    /// Creates a new VMAR within the one specified by `parent_vmar`.
    pub fn sys_vmar_allocate(
        &self,
        parent_vmar: HandleValue,
        options: u32,
        offset: u64,
        size: u64,
        mut out_child_vmar: UserOutPtr<HandleValue>,
        mut out_child_addr: UserOutPtr<usize>,
    ) -> ZxResult {
        let vm_options = VmOptions::from_bits(options).ok_or(ZxError::INVALID_ARGS)?;
        info!(
            "vmar.allocate: parent={:#x?}, options={:#x?}, offset={:#x?}, size={:#x?}",
            parent_vmar, options, offset, size,
        );
        // try to get parent_vmar
        let perm_rights = vm_options.to_rights();
        let proc = self.thread.proc();
        let parent = proc.get_object_with_rights::<VmAddressRegion>(parent_vmar, perm_rights)?;

        if vm_options.intersects(VmOptions::PERM_RXW | VmOptions::MAP_RANGE) {
            return Err(ZxError::INVALID_ARGS);
        }
        // Size must be page-aligned.
        if !(size as usize).is_multiple_of(PAGE_SIZE) {
            return Err(ZxError::INVALID_ARGS);
        }
        // OFFSET_IS_UPPER_LIMIT is mutually exclusive with SPECIFIC and SPECIFIC_OVERWRITE.
        if vm_options.contains(VmOptions::OFFSET_IS_UPPER_LIMIT)
            && vm_options.intersects(VmOptions::SPECIFIC | VmOptions::SPECIFIC_OVERWRITE)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        // get vmar_flags
        let vmar_flags = vm_options.to_flags();
        if vmar_flags.intersects(
            !(VmarFlags::SPECIFIC
                | VmarFlags::CAN_MAP_SPECIFIC
                | VmarFlags::COMPACT
                | VmarFlags::CAN_MAP_RXW),
        ) {
            return Err(ZxError::INVALID_ARGS);
        }

        // get align
        let align = amount_of_alignments(options)?;

        // get offest with options
        let offset = if vm_options.contains(VmOptions::SPECIFIC) {
            Some(offset as usize)
        } else if vm_options.contains(VmOptions::SPECIFIC_OVERWRITE) {
            // SPECIFIC_OVERWRITE is only valid for vmar_map, not vmar_allocate.
            return Err(ZxError::INVALID_ARGS);
        } else {
            if offset != 0 {
                return Err(ZxError::INVALID_ARGS);
            }
            None
        };

        let size = roundup_pages(size as usize);
        // check `size`
        if size == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let child = parent.allocate(offset, size, vmar_flags, align)?;
        let child_addr = child.addr();
        let child_handle = proc.add_handle(Handle::new(child, Rights::DEFAULT_VMAR | perm_rights));
        out_child_vmar.write(child_handle)?;
        out_child_addr.write(child_addr)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    /// Add a memory mapping.
    ///
    /// Maps the given VMO into the given virtual memory address region.
    pub fn sys_vmar_map(
        &self,
        vmar_handle: HandleValue,
        options: u32,
        vmar_offset: usize,
        vmo_handle: HandleValue,
        vmo_offset: usize,
        len: usize,
        mut mapped_addr: UserOutPtr<VirtAddr>,
    ) -> ZxResult {
        let options = match VmOptions::from_bits(options) {
            Some(o) => o,
            None => {
                return Err(ZxError::INVALID_ARGS);
            }
        };
        let proc = self.thread.proc();
        let (vmar, vmar_rights) = proc.get_object_and_rights::<VmAddressRegion>(vmar_handle)?;
        let (vmo, vmo_rights) = proc.get_object_and_rights::<VmObject>(vmo_handle)?;
        if !vmo_rights.contains(Rights::MAP) {
            return Err(ZxError::ACCESS_DENIED);
        };
        if options
            .intersects(VmOptions::CAN_MAP_RXW | VmOptions::CAN_MAP_SPECIFIC | VmOptions::COMPACT)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        if options.contains(VmOptions::REQUIRE_NON_RESIZABLE) && vmo.is_resizable() {
            return Err(ZxError::NOT_SUPPORTED);
        }
        // FAULT_BEYOND_STREAM_SIZE requires ALLOW_FAULTS.
        if options.contains(VmOptions::FAULT_BEYOND_STREAM_SIZE)
            && !options.contains(VmOptions::ALLOW_FAULTS)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        // OFFSET_IS_UPPER_LIMIT is mutually exclusive with SPECIFIC/SPECIFIC_OVERWRITE.
        if options.contains(VmOptions::OFFSET_IS_UPPER_LIMIT)
            && options.intersects(VmOptions::SPECIFIC | VmOptions::SPECIFIC_OVERWRITE)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        // check SPECIFIC options with offset
        let is_specific = options.contains(VmOptions::SPECIFIC)
            || options.contains(VmOptions::SPECIFIC_OVERWRITE);
        if !is_specific && vmar_offset != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        // SPECIFIC/SPECIFIC_OVERWRITE requires CAN_MAP_SPECIFIC on the VMAR.
        if is_specific && !vmar.flags().contains(VmarFlags::CAN_MAP_SPECIFIC) {
            return Err(ZxError::ACCESS_DENIED);
        }
        if !vmar_rights.contains(options.to_required_rights()) {
            return Err(ZxError::ACCESS_DENIED);
        }
        let mut permissions = MMUFlags::empty();
        permissions.set(MMUFlags::READ, vmo_rights.contains(Rights::READ));
        permissions.set(MMUFlags::WRITE, vmo_rights.contains(Rights::WRITE));
        permissions.set(MMUFlags::EXECUTE, vmo_rights.contains(Rights::EXECUTE));
        let mut mapping_flags = MMUFlags::USER;
        mapping_flags.set(MMUFlags::READ, options.contains(VmOptions::PERM_READ));
        mapping_flags.set(MMUFlags::WRITE, options.contains(VmOptions::PERM_WRITE));
        mapping_flags.set(MMUFlags::EXECUTE, options.contains(VmOptions::PERM_EXECUTE));
        // WRITE without READ is invalid (Fuchsia ABI requirement).
        if mapping_flags.contains(MMUFlags::WRITE) && !mapping_flags.contains(MMUFlags::READ) {
            return Err(ZxError::INVALID_ARGS);
        }
        let overwrite = options.contains(VmOptions::SPECIFIC_OVERWRITE);
        // The explicit MAP_RANGE flag is incompatible with SPECIFIC_OVERWRITE.
        // (Implicit eager commit from bare-metal mode is fine.)
        if overwrite && options.contains(VmOptions::MAP_RANGE) {
            return Err(ZxError::INVALID_ARGS);
        }
        let map_range = if cfg!(any(feature = "deny-page-fault", not(target_os = "none"))) {
            // On platforms that don't support page faults, reject
            // FAULT_BEYOND_STREAM_SIZE since it requires lazy mapping.
            if options.contains(VmOptions::FAULT_BEYOND_STREAM_SIZE) {
                return Err(ZxError::NOT_SUPPORTED);
            }
            true
        } else if options.contains(VmOptions::ALLOW_FAULTS) {
            // ALLOW_FAULTS: lazy commit, pages faulted in on demand
            false
        } else {
            // Default: eagerly commit pages to avoid stale page table issues
            true
        };

        info!(
            "mmuflags: {:?}, is_specific {:?}, overwrite {:?}, map_range {:?}",
            mapping_flags, is_specific, overwrite, map_range
        );
        // Note: SPECIFIC_OVERWRITE with eager commit is valid — the VMAR
        // layer removes existing mappings before creating the new one.
        // Note: we should reject non-page-aligned length here,
        // but since zCore use different memory layout from zircon,
        // we should not reject them and round up them instead
        // TODO: reject non-page-aligned length after we have the same memory layout with zircon
        let len = roundup_pages(len);
        if len == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let vmar_offset = if is_specific { Some(vmar_offset) } else { None };
        let fault_beyond = options.contains(VmOptions::FAULT_BEYOND_STREAM_SIZE);
        let vaddr = vmar.map_ext(
            vmar_offset,
            vmo.clone(),
            vmo_offset,
            len,
            permissions,
            mapping_flags,
            overwrite,
            map_range,
            fault_beyond,
        )?;
        mapped_addr.write(vaddr)?;
        Ok(())
    }

    /// Destroy a virtual memory address region.
    ///
    /// Unmaps all mappings within the given region, and destroys all sub-regions of the region.
    /// > This operation is logically recursive.
    pub fn sys_vmar_destroy(&self, handle_value: HandleValue) -> ZxResult {
        info!("vmar.destroy: handle={:#x?}", handle_value);
        let proc = self.thread.proc();
        let vmar =
            proc.get_object_with_rights::<VmAddressRegion>(handle_value, Rights::OP_CHILDREN)?;
        vmar.destroy()?;
        Ok(())
    }

    /// Set protection of virtual memory pages.
    pub fn sys_vmar_protect(
        &self,
        handle_value: HandleValue,
        options: u32,
        addr: u64,
        len: u64,
    ) -> ZxResult {
        let options = VmOptions::from_bits(options).ok_or(ZxError::INVALID_ARGS)?;
        info!(
            "vmar.protect: handle={:#x}, options={:#x}, addr={:#x}, len={:#x}",
            handle_value, options, addr, len
        );
        let proc = self.thread.proc();
        // Fuchsia checks handle rights: the VMAR handle must have
        // READ/WRITE/EXECUTE rights matching the PERM_* flags.
        let required_rights = options.to_required_rights();
        let (vmar, vmar_rights) = proc.get_object_and_rights::<VmAddressRegion>(handle_value)?;
        if !vmar_rights.contains(required_rights) {
            return Err(ZxError::ACCESS_DENIED);
        }
        if options.intersects(!VmOptions::PERM_RXW) {
            return Err(ZxError::INVALID_ARGS);
        }
        let mut mapping_flags = MMUFlags::empty();
        mapping_flags.set(MMUFlags::READ, options.contains(VmOptions::PERM_READ));
        mapping_flags.set(MMUFlags::WRITE, options.contains(VmOptions::PERM_WRITE));
        mapping_flags.set(MMUFlags::EXECUTE, options.contains(VmOptions::PERM_EXECUTE));
        // Hardware page tables don't support write-only or execute-only
        // pages (x86_64, aarch64, riscv64 all require READ for WRITE/EXECUTE).
        if (mapping_flags.contains(MMUFlags::WRITE) || mapping_flags.contains(MMUFlags::EXECUTE))
            && !mapping_flags.contains(MMUFlags::READ)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        info!("mmuflags: {:?}", mapping_flags);
        let len = roundup_pages(len as usize);
        if len == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let op_children = vmar_rights.contains(Rights::OP_CHILDREN);
        vmar.protect_ext(addr as usize, len, mapping_flags, op_children, false)?;
        Ok(())
    }

    /// Perform an operation on VMOs mapped within the given address range.
    pub fn sys_vmar_op_range(
        &self,
        handle_value: HandleValue,
        op: u32,
        addr: u64,
        size: u64,
        _buffer: usize,
        buffer_size: usize,
    ) -> ZxResult {
        let op = VmarOpType::try_from_raw(op)?;
        info!(
            "vmar.op_range: handle={:#x}, op={:?}, addr={:#x}, size={:#x}",
            handle_value, op, addr, size,
        );
        if buffer_size != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let required_rights = match op {
            VmarOpType::Commit | VmarOpType::Decommit | VmarOpType::Zero => Rights::WRITE,
            VmarOpType::MapRange | VmarOpType::Prefetch => Rights::READ,
            // DONT_NEED and ALWAYS_NEED require no handle rights.
            VmarOpType::DontNeed | VmarOpType::AlwaysNeed => Rights::empty(),
        };
        let (vmar, vmar_rights) = proc.get_object_and_rights::<VmAddressRegion>(handle_value)?;
        if !vmar_rights.contains(required_rights) {
            return Err(ZxError::ACCESS_DENIED);
        }
        // Operations on ranges that span child VMARs require OP_CHILDREN.
        if vmar.has_children_in_range(addr as usize, size as usize)
            && !vmar_rights.contains(Rights::OP_CHILDREN)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        vmar.op_range(op, addr as usize, size as usize)?;
        Ok(())
    }

    /// Unmap virtual memory pages.
    pub fn sys_vmar_unmap(&self, handle_value: HandleValue, addr: usize, len: usize) -> ZxResult {
        info!(
            "vmar.unmap: handle_value={:#x}, addr={:#x}, len={:#x}",
            handle_value, addr, len
        );
        let proc = self.thread.proc();
        let (vmar, vmar_rights) = proc.get_object_and_rights::<VmAddressRegion>(handle_value)?;
        let len = pages(len) * PAGE_SIZE;
        // If the unmap range spans child VMARs, require OP_CHILDREN.
        if vmar.has_children_in_range(addr, len) && !vmar_rights.contains(Rights::OP_CHILDREN) {
            return Err(ZxError::INVALID_ARGS);
        }
        vmar.unmap(addr, len)?;
        Ok(())
    }

    /// Map a kernel clock object's transformation state into user address space.
    ///
    /// The mapping is read-only (PERM_WRITE and PERM_EXECUTE are rejected).
    /// This allows userspace to read the clock without a syscall.
    /// Map a kernel clock object's transformation state into user address space.
    ///
    /// Creates a VMO from the clock's state and maps it read-only
    /// into the VMAR. Userspace can then read the clock without a
    /// syscall by computing: `clock = (mono - ref) * rate + offset`.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_vmar_map_clock(
        &self,
        handle: HandleValue,
        options: u32,
        vmar_offset: u64,
        clock_handle: HandleValue,
        len: u64,
        mut mapped_addr: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "vmar.map_clock: vmar={:#x}, options={:#x}, clock={:#x}",
            handle, options, clock_handle
        );
        let options = VmOptions::from_bits(options).ok_or(ZxError::INVALID_ARGS)?;
        // Reject write/execute permissions — clock mapping is read-only.
        if options.contains(VmOptions::PERM_WRITE) || options.contains(VmOptions::PERM_EXECUTE) {
            return Err(ZxError::INVALID_ARGS);
        }
        // FAULT_BEYOND_STREAM_SIZE is not valid for clock mappings —
        // clocks are not streams and don't have a content size.
        if options.contains(VmOptions::FAULT_BEYOND_STREAM_SIZE) {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        // Handle pseudo-handle for root VMAR (ZX_HANDLE_VMAR_ROOT_SELF).
        let vmar = if let Some(obj) = self.resolve_pseudo_handle(handle) {
            obj.downcast_arc::<VmAddressRegion>()
                .map_err(|_| ZxError::WRONG_TYPE)?
        } else {
            proc.get_object::<VmAddressRegion>(handle)?
        };
        let clock = proc.get_object_with_rights::<zircon_object::signal::Clock>(
            clock_handle,
            Rights::READ | Rights::MAP,
        )?;

        // Create a VMO with the clock's transformation state.
        let vmo = clock.create_state_vmo()?;
        let map_len = if len > 0 {
            roundup_pages(len as usize)
        } else {
            PAGE_SIZE
        };

        // Map read-only into the VMAR.
        let mapping_flags = MMUFlags::USER | MMUFlags::READ;
        let is_specific = options.contains(VmOptions::SPECIFIC)
            || options.contains(VmOptions::SPECIFIC_OVERWRITE);
        if is_specific && !vmar.flags().contains(VmarFlags::CAN_MAP_SPECIFIC) {
            return Err(ZxError::ACCESS_DENIED);
        }
        let vmar_off = if is_specific {
            Some(vmar_offset as usize)
        } else {
            None
        };
        let overwrite = options.contains(VmOptions::SPECIFIC_OVERWRITE);
        let vaddr = vmar.map_ext(
            vmar_off,
            vmo,
            0,
            map_len,
            mapping_flags,
            mapping_flags,
            overwrite,
            true,
            false,
        )?;
        info!("vmar.map_clock: mapped at {:#x}", vaddr);
        mapped_addr.write(vaddr)?;
        Ok(())
    }

    /// Map an IOBuffer region into user address space.
    ///
    /// Maps the VMO backing a specific IOB region into the process's
    /// address space, subject to the endpoint's per-region access flags.
    /// Map an IOBuffer region into user address space.
    ///
    /// Extracts the VMO backing the specified IOB region and maps it
    /// into the VMAR, subject to the endpoint's per-region access
    /// flags and the requested VM options.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_vmar_map_iob(
        &self,
        handle: HandleValue,
        options: u32,
        vmar_offset: usize,
        iob_handle: HandleValue,
        region_index: u32,
        region_len: usize,
        mut addr_out: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "vmar.map_iob: vmar={:#x}, options={:#x}, iob={:#x}, region={}, offset={:#x}",
            handle, options, iob_handle, region_index, vmar_offset
        );
        let options = VmOptions::from_bits(options).ok_or(ZxError::INVALID_ARGS)?;
        let proc = self.thread.proc();
        let vmar = proc.get_object::<VmAddressRegion>(handle)?;
        let iob =
            proc.get_object_with_rights::<zircon_object::ipc::IoBuffer>(iob_handle, Rights::MAP)?;

        // Get the region's VMO, size, and access flags.
        let (vmo, region_size, access) = iob.get_region(region_index as usize)?;
        let ep_idx = iob.endpoint_index();

        // Check endpoint access permissions against requested options.
        let can_read = if ep_idx == 0 {
            access & 0x01 != 0 // EP0_CAN_MAP_READ
        } else {
            access & 0x10 != 0 // EP1_CAN_MAP_READ
        };
        let can_write = if ep_idx == 0 {
            access & 0x02 != 0 // EP0_CAN_MAP_WRITE
        } else {
            access & 0x20 != 0 // EP1_CAN_MAP_WRITE
        };
        if options.contains(VmOptions::PERM_READ) && !can_read {
            return Err(ZxError::ACCESS_DENIED);
        }
        if options.contains(VmOptions::PERM_WRITE) && !can_write {
            return Err(ZxError::ACCESS_DENIED);
        }
        // Execute permission is never allowed on IOB regions.
        if options.contains(VmOptions::PERM_EXECUTE) {
            return Err(ZxError::ACCESS_DENIED);
        }

        // Determine the mapping length.
        let len = if region_len > 0 {
            roundup_pages(region_len)
        } else {
            roundup_pages(region_size)
        };
        if len == 0 {
            return Err(ZxError::INVALID_ARGS);
        }

        // Build MMU flags from the requested options.
        let mut mapping_flags = MMUFlags::USER;
        mapping_flags.set(MMUFlags::READ, options.contains(VmOptions::PERM_READ));
        mapping_flags.set(MMUFlags::WRITE, options.contains(VmOptions::PERM_WRITE));

        // Determine if specific placement is requested.
        let is_specific = options.contains(VmOptions::SPECIFIC)
            || options.contains(VmOptions::SPECIFIC_OVERWRITE);
        if is_specific && !vmar.flags().contains(VmarFlags::CAN_MAP_SPECIFIC) {
            return Err(ZxError::ACCESS_DENIED);
        }
        let vmar_offset = if is_specific { Some(vmar_offset) } else { None };
        let overwrite = options.contains(VmOptions::SPECIFIC_OVERWRITE);

        // Map the region's VMO into the VMAR.
        let permissions = mapping_flags;
        let vaddr = vmar.map_ext(
            vmar_offset,
            vmo,
            0, // vmo_offset: always map from the start of the region VMO
            len,
            permissions,
            mapping_flags,
            overwrite,
            true,  // map_range: commit pages immediately
            false, // fault_beyond_stream_size
        )?;
        info!(
            "vmar.map_iob: mapped region {} at {:#x}",
            region_index, vaddr
        );
        addr_out.write(vaddr)?;
        Ok(())
    }
}

bitflags! {
    struct VmOptions: u32 {
        #[allow(clippy::identity_op)]
        const PERM_READ             = 1 << 0;
        const PERM_WRITE            = 1 << 1;
        const PERM_EXECUTE          = 1 << 2;
        const COMPACT               = 1 << 3;
        const SPECIFIC              = 1 << 4;
        const SPECIFIC_OVERWRITE    = 1 << 5;
        const CAN_MAP_SPECIFIC      = 1 << 6;
        const CAN_MAP_READ          = 1 << 7;
        const CAN_MAP_WRITE         = 1 << 8;
        const CAN_MAP_EXECUTE       = 1 << 9;
        const MAP_RANGE             = 1 << 10;
        const REQUIRE_NON_RESIZABLE = 1 << 11;
        const ALLOW_FAULTS          = 1 << 12;
        const OFFSET_IS_UPPER_LIMIT = 1 << 13;
        const PERM_READ_IF_XOM_UNSUPPORTED = 1 << 14;
        /// Allow page faults beyond the stream content size.
        const FAULT_BEYOND_STREAM_SIZE = 1 << 15;
        const CAN_MAP_RXW           = Self::CAN_MAP_READ.bits | Self::CAN_MAP_EXECUTE.bits | Self::CAN_MAP_WRITE.bits;
        const PERM_RXW           = Self::PERM_READ.bits | Self::PERM_WRITE.bits | Self::PERM_EXECUTE.bits;
    }
}

impl VmOptions {
    fn to_rights(self) -> Rights {
        let mut rights = Rights::empty();
        if self.contains(VmOptions::CAN_MAP_READ) {
            rights.insert(Rights::READ);
        }
        if self.contains(VmOptions::CAN_MAP_WRITE) {
            rights.insert(Rights::WRITE);
        }
        if self.contains(VmOptions::CAN_MAP_EXECUTE) {
            rights.insert(Rights::EXECUTE);
        }
        rights
    }

    fn to_required_rights(self) -> Rights {
        let mut rights = Rights::empty();
        if self.contains(VmOptions::PERM_READ) {
            rights.insert(Rights::READ);
        }
        if self.contains(VmOptions::PERM_WRITE) {
            rights.insert(Rights::WRITE);
        }
        if self.contains(VmOptions::PERM_EXECUTE) {
            rights.insert(Rights::EXECUTE);
        }
        rights
    }

    fn to_flags(self) -> VmarFlags {
        let mut flags = VmarFlags::empty();
        if self.contains(VmOptions::COMPACT) {
            flags.insert(VmarFlags::COMPACT);
        }
        if self.contains(VmOptions::SPECIFIC) {
            flags.insert(VmarFlags::SPECIFIC);
        }
        if self.contains(VmOptions::SPECIFIC_OVERWRITE) {
            flags.insert(VmarFlags::SPECIFIC_OVERWRITE);
        }
        if self.contains(VmOptions::CAN_MAP_SPECIFIC) {
            flags.insert(VmarFlags::CAN_MAP_SPECIFIC);
        }
        if self.contains(VmOptions::CAN_MAP_READ) {
            flags.insert(VmarFlags::CAN_MAP_READ);
        }
        if self.contains(VmOptions::CAN_MAP_WRITE) {
            flags.insert(VmarFlags::CAN_MAP_WRITE);
        }
        if self.contains(VmOptions::CAN_MAP_EXECUTE) {
            flags.insert(VmarFlags::CAN_MAP_EXECUTE);
        }
        if self.contains(VmOptions::REQUIRE_NON_RESIZABLE) {
            flags.insert(VmarFlags::REQUIRE_NON_RESIZABLE);
        }
        if self.contains(VmOptions::ALLOW_FAULTS) {
            flags.insert(VmarFlags::ALLOW_FAULTS);
        }
        flags
    }
}
