use {
    super::*,
    bitflags::bitflags,
    hal_impl::DevVAddr,
    zircon_object::{dev::*, signal::*, task::*, vm::*},
};

impl Syscall<'_> {
    /// Create a new object in the kernel representing an IOMMU device.
    pub fn sys_iommu_create(
        &self,
        resource: HandleValue,
        type_: u32,
        desc: UserInPtr<u8>,
        desc_size: usize,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "iommu.create: resource={:#x}, type={:#x}, desc={:#x?} desc_size={:#x} out={:#x?}",
            resource, type_, desc, desc_size, out
        );
        let proc = self.thread.proc();
        let res = proc.get_resource(resource)?;
        // Accept ROOT resource or SYSTEM resource with IOMMU base.
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_IOMMU_BASE, 1)?;
        }
        if desc_size > IOMMU_MAX_DESC_LEN {
            return Err(ZxError::INVALID_ARGS);
        }
        if desc_size != IOMMU_DESC_SIZE {
            return Err(ZxError::INVALID_ARGS);
        }
        if type_ != IOMMU_TYPE_DUMMY {
            // TODO: implement non-dummy IOMMU types
            warn!("iommu.create: type {} not supported (only dummy)", type_);
            return Err(ZxError::NOT_SUPPORTED);
        }
        let _copied_desc = desc.read_array(desc_size)?;
        let iommu = Iommu::create();
        let handle = proc.add_handle(Handle::new(iommu, Rights::DEFAULT_CHANNEL));
        info!("iommu handle value {:#x}", handle);
        out.write(handle)?;
        Ok(())
    }
    /// Creates a new bus transaction initiator.
    ///
    /// `iommu: HandleValue`, a handle to an IOMMU.
    /// `options: u32`, must be 0 (reserved for future definition of creation flags).
    /// `bti_id: u64`, a hardware transaction identifier for a device downstream of that IOMMU.
    pub fn sys_bti_create(
        &self,
        iommu: HandleValue,
        options: u32,
        bti_id: u64,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "bti.create: iommu={:#x}, options={:?}, bti_id={:#x?}",
            iommu, options, bti_id
        );
        let proc = self.thread.proc();
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let iommu = proc.get_object::<Iommu>(iommu)?;
        if !iommu.is_valid_bus_txn_id() {
            return Err(ZxError::INVALID_ARGS);
        }
        let bti = BusTransactionInitiator::create(iommu, bti_id);
        let handle = proc.add_handle(Handle::new(bti, Rights::DEFAULT_BTI));
        out.write(handle)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    /// Pin pages and grant devices access to them.
    pub async fn sys_bti_pin(
        &self,
        bti: HandleValue,
        options: u32,
        vmo: HandleValue,
        offset: usize,
        size: usize,
        mut addrs: UserOutPtr<DevVAddr>,
        addrs_count: usize,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        let options = BtiOptions::from_bits(options).ok_or(ZxError::INVALID_ARGS)?;
        // At least one permission flag must be set.
        if !options
            .intersects(BtiOptions::PERM_READ | BtiOptions::PERM_WRITE | BtiOptions::PERM_EXECUTE)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        info!(
            "bti.pin: bti={:#x}, options={:?}, vmo={:#x}, offset={:#x}, size={:#x}, addrs={:#x?}, addrs_count={:#x}",
            bti, options, vmo, offset, size, addrs, addrs_count
        );
        let proc = self.thread.proc();
        let bti = proc.get_object_with_rights::<BusTransactionInitiator>(bti, Rights::MAP)?;
        if !page_aligned(offset) || !page_aligned(size) {
            return Err(ZxError::INVALID_ARGS);
        }
        // Reject if the output buffer would exceed the maximum
        // user-copy size (PAGE_SIZE * 64 = 256KB), matching Fuchsia.
        if addrs_count * core::mem::size_of::<DevVAddr>() > PAGE_SIZE * 64 {
            return Err(ZxError::INVALID_ARGS);
        }
        let vmo = proc.get_object_with_rights::<VmObject>(vmo, options.to_vmo_rights())?;
        let compress_results = options.contains(BtiOptions::COMPRESS);
        let contiguous = options.contains(BtiOptions::CONTIGUOUS);
        if contiguous && (compress_results || !vmo.is_contiguous()) {
            return Err(ZxError::INVALID_ARGS);
        }
        // For pager-backed VMOs, demand-page the pages before pinning.
        // The pin needs committed pages, so we wait for the pager to
        // supply them. Use blocking_run so Thread::kill() can interrupt.
        if vmo.is_pager_backed() || vmo.is_in_pager_tree() {
            use zircon_object::task::ThreadState;
            let pager_vmo = if vmo.is_pager_backed() {
                vmo.clone()
            } else {
                vmo.find_root_pager_vmo().unwrap_or_else(|| vmo.clone())
            };
            let start_page = offset / PAGE_SIZE;
            let end_page = (offset + size).div_ceil(PAGE_SIZE);
            for page_idx in start_page..end_page {
                if pager_vmo.committed_pages_in_range(page_idx, page_idx + 1) > 0 {
                    continue;
                }
                // Check if page is beyond VMO size (resize may have
                // shrunk it while we were waiting).
                if page_idx * PAGE_SIZE >= vmo.len() {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                let _ = pager_vmo.request_pages(page_idx * PAGE_SIZE, PAGE_SIZE);
                let future = pager_vmo.wait_for_page(page_idx * PAGE_SIZE);
                futures::pin_mut!(future);
                self.thread.set_blocking_state(ThreadState::BlockedPager);
                let result = self
                    .thread
                    .blocking_run::<_, (), _>(
                        future,
                        ThreadState::BlockedPager,
                        core::time::Duration::from_nanos(u64::MAX),
                        None,
                    )
                    .await;
                match result {
                    Ok(()) => {}
                    Err(ZxError::OUT_OF_RANGE) => return Err(ZxError::OUT_OF_RANGE),
                    Err(e) => return Err(e),
                }
            }
        }
        let pmt = bti.pin(vmo, offset, size, options.to_iommu_perms())?;
        let encoded_addrs = pmt.encode_addrs(compress_results, contiguous)?;
        if encoded_addrs.len() != addrs_count {
            warn!(
                "bti.pin addrs_count = {}, but encoded_addrs.len = {}",
                addrs_count,
                encoded_addrs.len()
            );
            return Err(ZxError::INVALID_ARGS);
        }
        addrs.write_array(&encoded_addrs)?;
        let handle = proc.add_handle(Handle::new(pmt, Rights::INSPECT));
        out.write(handle)?;
        Ok(())
    }

    /// Unpins pages that were previously pinned by `zx_bti_pin()`.
    pub fn sys_pmt_unpin(&self, pmt: HandleValue) -> ZxResult {
        info!("pmt.unpin: pmt={:#x}", pmt);
        let proc = self.thread.proc();
        let pmt = proc.remove_object::<PinnedMemoryToken>(pmt)?;
        pmt.unpin();
        Ok(())
    }

    /// Releases all quarantined PMTs for the given BTI.
    pub fn sys_bti_release_quarantine(&self, bti: HandleValue) -> ZxResult {
        info!("bti.release_quarantine: bti = {:#x}", bti);
        let proc = self.thread.proc();
        let bti = proc.get_object_with_rights::<BusTransactionInitiator>(bti, Rights::WRITE)?;
        bti.release_quarantine();
        Ok(())
    }

    /// Get PC firmware tables (ACPI, SMBIOS).
    #[allow(unused_variables, unused_mut)]
    pub fn sys_pc_firmware_tables(
        &self,
        resource: HandleValue,
        mut acpi_rsdp_ptr: UserOutPtr<u64>,
        mut smbios_ptr: UserOutPtr<u64>,
    ) -> ZxResult {
        info!("pc_firmware_tables: handle={:?}", resource);
        let proc = self.thread.proc();
        proc.get_resource(resource)?.validate(ResourceKind::ROOT)?;
        cfg_if::cfg_if! {
            if #[cfg(all(target_arch = "x86_64", target_os = "none"))] {
                let (acpi_rsdp, smbios) = hal_impl::x86_64::pc_firmware_tables();
                acpi_rsdp_ptr.write(acpi_rsdp)?;
                smbios_ptr.write(smbios)?;
                Ok(())
            } else {
                Err(ZxError::NOT_SUPPORTED)
            }
        }
    }

    /// Creates an interrupt object which represents a physical or virtual interrupt.
    pub fn sys_interrupt_create(
        &self,
        resource: HandleValue,
        src_num: usize,
        options: u32,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "interrupt.create: handle={:?} src_num={:?} options={:?}",
            resource, src_num, options
        );
        let proc = self.thread.proc();
        let options = InterruptOptions::from_bits_truncate(options);
        let interrupt = if options.contains(InterruptOptions::VIRTUAL) {
            // VIRTUAL may be combined with WAKE_VECTOR; reject other flags.
            let allowed = InterruptOptions::VIRTUAL | InterruptOptions::WAKE_VECTOR;
            if options & !allowed != InterruptOptions::empty() {
                return Err(ZxError::INVALID_ARGS);
            }
            // WAKE_VECTOR requires a valid resource handle.
            let is_wake = options.contains(InterruptOptions::WAKE_VECTOR);
            if is_wake {
                proc.get_resource(resource)?;
            }
            let intr = if is_wake {
                Interrupt::new_virtual_wake()
            } else {
                Interrupt::new_virtual()
            };
            if is_wake {
                zircon_object::dev::register_wake_source(intr.id());
            }
            intr
        } else {
            let resource = proc.get_resource(resource)?;
            resource.validate_ranged_resource(ResourceKind::IRQ, src_num, 1)?;
            Interrupt::new_physical(src_num, options)?
        };
        let handle = proc.add_handle(Handle::new(interrupt, Rights::DEFAULT_INTERRUPT));
        out.write(handle)?;
        Ok(())
    }

    /// Binds or unbinds an interrupt object to a port.
    ///
    /// The key used when binding the interrupt will be present in the key field of the `zx_port_packet_t`.
    pub fn sys_interrupt_bind(
        &self,
        interrupt: HandleValue,
        port: HandleValue,
        key: u64,
        options: u32,
    ) -> ZxResult {
        info!(
            "interrupt.bind: interrupt={:?} port={:?} key={:?} options={:?}",
            interrupt, port, key, options
        );
        let proc = self.thread.proc();
        let interrupt = proc.get_object_with_rights::<Interrupt>(interrupt, Rights::READ)?;
        let port = proc.get_object_with_rights::<Port>(port, Rights::WRITE)?;
        if !port.can_bind_to_interrupt() {
            return Err(ZxError::WRONG_TYPE);
        }
        if options == InterruptOp::Bind as _ {
            interrupt.bind(&port, key)
        } else if options == InterruptOp::Unbind as _ {
            interrupt.unbind(&port)
        } else {
            Err(ZxError::INVALID_ARGS)
        }
    }

    /// Triggers a virtual interrupt object.
    pub fn sys_interrupt_trigger(
        &self,
        interrupt: HandleValue,
        options: u32,
        timestamp: i64,
    ) -> ZxResult {
        info!(
            "interrupt.trigger: interrupt={:?} options={:?} timestamp={:?}",
            interrupt, options, timestamp
        );
        let interrupt = self
            .thread
            .proc()
            .get_object_with_rights::<Interrupt>(interrupt, Rights::SIGNAL)?;
        // Record wake source trigger for suspend reports.
        // Use the caller-provided timestamp (boot time at trigger).
        zircon_object::dev::wake_source_triggered(interrupt.id(), timestamp as u64);
        interrupt.trigger(timestamp)
    }

    /// Acknowledge an interrupt and re-arm it.
    ///
    /// This system call acknowledges an interrupt object, causing it to be eligible to trigger again (and delivering a packet to the port it is bound to).
    pub fn sys_interrupt_ack(&self, interrupt: HandleValue) -> ZxResult {
        info!("interupt.ack: interrupt={:?}", interrupt);
        let interrupt = self
            .thread
            .proc()
            .get_object_with_rights::<Interrupt>(interrupt, Rights::WRITE)?;
        let koid = interrupt.id();
        let now = hal_impl::timer::timer_now().as_nanos() as u64;
        let re_triggered = interrupt.ack()?;
        // Record wake source ack for suspend reports.
        // If the interrupt immediately re-triggered (pending coalesced trigger),
        // keep the wake source signaled. Otherwise, clear signaled state.
        if re_triggered {
            zircon_object::dev::wake_source_ack_remain_signaled(koid, now);
        } else {
            zircon_object::dev::wake_source_acked(koid, now);
        }
        Ok(())
    }

    /// Destroys an interrupt object.
    pub fn sys_interrupt_destroy(&self, interrupt: HandleValue) -> ZxResult {
        info!("interupt.destory: interrupt={:?}", interrupt);
        let interrupt = self.thread.proc().get_object::<Interrupt>(interrupt)?;
        interrupt.destroy()
    }

    /// A blocking syscall which causes the caller to wait until an interrupt is triggered.
    pub async fn sys_interrupt_wait(
        &self,
        interrupt: HandleValue,
        mut out: UserOutPtr<i64>,
    ) -> ZxResult {
        info!("interrupt.wait: handle={:?}", interrupt);
        assert_eq!(core::mem::size_of::<PortPacket>(), 48);
        let proc = self.thread.proc();
        let interrupt = proc.get_object_with_rights::<Interrupt>(interrupt, Rights::WAIT)?;
        let future = interrupt.wait();
        pin_mut!(future);
        self.thread
            .set_blocking_state(ThreadState::BlockedInterrupt);
        let timestamp = self
            .thread
            .blocking_run(
                future,
                ThreadState::BlockedInterrupt,
                Deadline::forever().into(),
                None,
            )
            .await?;
        out.write_if_not_null(timestamp)?;
        Ok(())
    }

    /// Request access to I/O ports (x86-only).
    pub fn sys_ioports_request(&self, resource: HandleValue, io_addr: u16, len: u32) -> ZxResult {
        info!(
            "ioports.request: resource={:#x}, addr={:#x}, len={}",
            resource, io_addr, len
        );
        let proc = self.thread.proc();
        proc.get_resource(resource)?.validate_ranged_resource(
            ResourceKind::IOPORT,
            io_addr as usize,
            len as usize,
        )?;
        if len == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Err(ZxError::NOT_SUPPORTED)
        }
        #[cfg(target_arch = "x86_64")]
        {
            // IOPL=3 is already set in RFLAGS, so ring-3 code has
            // full I/O port access. A proper implementation would
            // use the TSS IOPB for per-port granularity.
            Ok(())
        }
    }

    /// Release access to I/O ports (x86-only).
    pub fn sys_ioports_release(&self, resource: HandleValue, io_addr: u16, len: u32) -> ZxResult {
        info!(
            "ioports.release: resource={:#x}, addr={:#x}, len={}",
            resource, io_addr, len
        );
        let proc = self.thread.proc();
        proc.get_resource(resource)?.validate_ranged_resource(
            ResourceKind::IOPORT,
            io_addr as usize,
            len as usize,
        )?;
        if len == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Err(ZxError::NOT_SUPPORTED)
        }
        #[cfg(target_arch = "x86_64")]
        {
            // IOPL=3 grants access; release is a no-op until we have
            // proper IOPB management.
            Ok(())
        }
    }

    /// Make an ARM Secure Monitor Call (aarch64-only).
    pub fn sys_smc_call(
        &self,
        handle: HandleValue,
        _parameters: usize,
        _out_result: usize,
    ) -> ZxResult {
        info!("smc.call: handle={:#x}", handle);
        let proc = self.thread.proc();
        proc.get_resource(handle)?.validate(ResourceKind::SMC)?;
        // Would issue SMC instruction on bare metal aarch64
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Allocate a block of MSI interrupt vectors.
    ///
    /// The resource handle must be of kind `ZX_RSRC_KIND_SYSTEM` with
    /// base `ZX_RSRC_SYSTEM_MSI_BASE`, or the root resource.
    /// Count must be a power of two (1, 2, 4, 8, 16, or 32).
    pub fn sys_msi_allocate(
        &self,
        resource: HandleValue,
        count: u32,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!("msi.allocate: resource={:#x}, count={}", resource, count);
        let proc = self.thread.proc();
        let res = proc.get_resource(resource)?;
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_MSI_BASE, 1)?;
        }
        if count == 0 || count > 32 || !count.is_power_of_two() {
            return Err(ZxError::INVALID_ARGS);
        }
        // Try to allocate hardware MSI vectors via the HAL.
        // If no MSI controller is available, allocate with base_irq=0
        // (software-only allocation for testing).
        let base_irq = hal_impl::interrupt::msi_alloc_block(count as usize)
            .map(|range| range.start)
            .unwrap_or(0);
        let alloc = MsiAllocation::create(count, base_irq);
        // MSI allocation objects get BASIC rights (TRANSFER | DUPLICATE | WAIT | INSPECT).
        let handle = proc.add_handle(Handle::new(alloc, Rights::BASIC));
        out.write(handle)?;
        Ok(())
    }

    /// Create an interrupt object from an MSI allocation.
    ///
    /// `handle` must be an MSI allocation object. `options` must be 0.
    /// `msi_id` selects the vector within the allocation.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_msi_create(
        &self,
        handle: HandleValue,
        options: u32,
        msi_id: u32,
        vmo_handle: HandleValue,
        vmo_offset: usize,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "msi.create: handle={:#x}, options={}, msi_id={}, vmo={:#x}, vmo_off={:#x}",
            handle, options, msi_id, vmo_handle, vmo_offset
        );
        const ZX_MSI_MODE_MSI_X: u32 = 1;
        if options & !ZX_MSI_MODE_MSI_X != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let alloc = proc.get_object::<MsiAllocation>(handle)?;
        // Validate the VMO handle.
        let vmo = proc.get_object_with_rights::<VmObject>(vmo_handle, Rights::MAP)?;
        // Validate msi_id is within the allocation.
        if msi_id >= alloc.count() {
            return Err(ZxError::INVALID_ARGS);
        }
        let is_msix = options & ZX_MSI_MODE_MSI_X != 0;
        // Validate vmo_offset: must have enough space for the capability
        // structure (MSI: 24 bytes, MSI-X: 16 bytes per entry).
        let cap_size = if is_msix {
            16 * (msi_id as usize + 1) // MSI-X table entry per vector
        } else {
            24 // MSI capability structure
        };
        if vmo_offset >= vmo.len() || vmo.len() - vmo_offset < cap_size {
            return Err(ZxError::INVALID_ARGS);
        }
        // For MSI (non-MSI-X), validate the capability ID in the VMO.
        if !is_msix {
            let mut id_buf = [0u8; 1];
            vmo.read(vmo_offset, &mut id_buf)?;
            if id_buf[0] != 0x05 {
                // MSI capability ID
                return Err(ZxError::INVALID_ARGS);
            }
        }
        alloc.use_vector(msi_id)?;
        let interrupt = Interrupt::new_virtual();
        // Set up the capability structure and on-close callback.
        // If any VMO operation fails, release the vector before returning.
        let setup_result = if is_msix {
            // MSI-X table entry: msg_addr(4) + msg_upper_addr(4)
            //   + msg_data(4) + vector_control(4) = 16 bytes per entry.
            let entry_base = vmo_offset + (msi_id as usize) * 16;
            let target_addr: u64 = 0xFEE0_0000;
            let target_data: u32 = alloc.base_irq() as u32 + msi_id;
            (|| -> ZxResult {
                vmo.write(entry_base, &(target_addr as u32).to_le_bytes())?;
                vmo.write(entry_base + 4, &((target_addr >> 32) as u32).to_le_bytes())?;
                vmo.write(entry_base + 8, &target_data.to_le_bytes())?;
                let mut ctrl = [0u8; 4];
                vmo.read(entry_base + 12, &mut ctrl)?;
                let val = u32::from_le_bytes(ctrl) & !1; // unmask
                vmo.write(entry_base + 12, &val.to_le_bytes())?;
                Ok(())
            })()
            .map(|()| {
                // On close: mask first, then clear, then release.
                let vmo_clone = vmo.clone();
                let alloc_clone = alloc.clone();
                interrupt.set_on_close(move || {
                    // Re-mask before releasing.
                    let masked = 1u32;
                    let _ = vmo_clone.write(entry_base + 12, &masked.to_le_bytes());
                    let zeros = [0u8; 12]; // clear addr + data
                    let _ = vmo_clone.write(entry_base, &zeros);
                    alloc_clone.release_vector(msi_id);
                });
            })
        } else {
            // MSI capability: mask_bits_32.
            // Offset depends on whether 64-bit addressing is enabled.
            // Check control register bit 7 (64-bit capable).
            let mut control_buf = [0u8; 2];
            vmo.read(vmo_offset + 2, &mut control_buf).and_then(|()| {
                let control = u16::from_le_bytes(control_buf);
                let is_64bit = control & (1 << 7) != 0;
                let mask_offset = if is_64bit {
                    vmo_offset + 0x10 // 64-bit: mask at offset 16
                } else {
                    vmo_offset + 0x0C // 32-bit: mask at offset 12
                };
                let mut mask = [0u8; 4];
                vmo.read(mask_offset, &mut mask)?;
                let val = u32::from_le_bytes(mask) & !(1u32 << msi_id);
                vmo.write(mask_offset, &val.to_le_bytes())?;
                // On close: re-mask and release vector.
                let vmo_clone = vmo.clone();
                let alloc_clone = alloc.clone();
                interrupt.set_on_close(move || {
                    let mut mask = [0u8; 4];
                    if vmo_clone.read(mask_offset, &mut mask).is_ok() {
                        let val = u32::from_le_bytes(mask) | (1u32 << msi_id);
                        let _ = vmo_clone.write(mask_offset, &val.to_le_bytes());
                    }
                    alloc_clone.release_vector(msi_id);
                });
                Ok(())
            })
        };
        if let Err(e) = setup_result {
            alloc.release_vector(msi_id);
            return Err(e);
        }
        let handle = proc.add_handle(Handle::new(interrupt, Rights::DEFAULT_INTERRUPT));
        out.write(handle)?;
        Ok(())
    }

    /// Bind an interrupt object to a virtual CPU.
    ///
    /// When the interrupt fires, it will be delivered to the guest
    /// vCPU as a virtual interrupt. Options must be 0.
    pub fn sys_interrupt_bind_vcpu(
        &self,
        interrupt: HandleValue,
        vcpu: HandleValue,
        options: u32,
    ) -> ZxResult {
        info!(
            "interrupt.bind_vcpu: interrupt={:#x}, vcpu={:#x}, options={}",
            interrupt, vcpu, options
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let _interrupt_obj = proc.get_object_with_rights::<Interrupt>(interrupt, Rights::READ)?;

        #[cfg(feature = "hypervisor")]
        {
            use zircon_object::hypervisor::Vcpu;
            let _vcpu_obj = proc.get_object_with_rights::<Vcpu>(vcpu, Rights::WRITE)?;
            // Both handles validated. The actual interrupt-to-vCPU
            // delivery requires adding a vCPU binding to the Interrupt
            // object's trigger path (Interrupt::bind_vcpu method),
            // which will call vcpu.virtual_interrupt(vector) when the
            // interrupt fires. This wiring is a follow-up.
            warn!("interrupt.bind_vcpu: handles validated, delivery wiring TBD");
            Ok(())
        }

        #[cfg(not(feature = "hypervisor"))]
        {
            let _ = vcpu;
            Err(ZxError::NOT_SUPPORTED)
        }
    }
}

const IOMMU_TYPE_DUMMY: u32 = 0;
const IOMMU_MAX_DESC_LEN: usize = 4096;
const IOMMU_DESC_SIZE: usize = 1;

bitflags! {
    struct BtiOptions: u32 {
        #[allow(clippy::identity_op)]
        const PERM_READ             = 1 << 0;
        const PERM_WRITE            = 1 << 1;
        const PERM_EXECUTE          = 1 << 2;
        const COMPRESS              = 1 << 3;
        const CONTIGUOUS            = 1 << 4;
    }
}

enum InterruptOp {
    Bind = 0,
    Unbind = 1,
}

impl BtiOptions {
    /// Get desired rights of VMO handle.
    fn to_vmo_rights(self) -> Rights {
        let mut rights = Rights::MAP;
        if self.contains(BtiOptions::PERM_READ) {
            rights.insert(Rights::READ);
        }
        if self.contains(BtiOptions::PERM_WRITE) {
            rights.insert(Rights::WRITE);
        }
        if self.contains(BtiOptions::PERM_EXECUTE) {
            // NOTE: Check Rights::READ instead of Rights::EXECUTE,
            // because Rights::EXECUTE applies to the execution permission of the host CPU,
            // but ZX_BTI_PERM_EXECUTE applies to transactions initiated by the bus device.
            rights.insert(Rights::READ);
        }
        rights
    }

    fn to_iommu_perms(self) -> IommuPerms {
        let mut perms = IommuPerms::empty();
        if self.contains(BtiOptions::PERM_READ) {
            perms.insert(IommuPerms::PERM_READ)
        }
        if self.contains(BtiOptions::PERM_WRITE) {
            perms.insert(IommuPerms::PERM_WRITE);
        }
        if self.contains(BtiOptions::PERM_EXECUTE) {
            perms.insert(IommuPerms::PERM_EXECUTE);
        }
        perms
    }
}
