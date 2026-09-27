use super::*;
use zircon_object::ipc::{IoBuffer, IoBufferRegion, IOB_MAX_REGIONS};
use zircon_object::vm::VmObject;

impl Syscall<'_> {
    /// Create an IOBuffer (I/O Buffer) object pair.
    ///
    /// IOBuffers are shared-memory IPC primitives with configurable
    /// per-region access control. Returns handles to both endpoints.
    ///
    /// `options` must be 0.
    /// `regions_ptr` points to an array of `zx_iob_region_t` structs.
    /// `region_count` is the number of regions (max 64).
    pub fn sys_iob_create(
        &self,
        options: u64,
        regions_ptr: usize,
        region_count: usize,
        mut ep0_out: UserOutPtr<HandleValue>,
        mut ep1_out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "iob.create: options={}, regions={:#x}, count={}",
            options, regions_ptr, region_count
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        if region_count > IOB_MAX_REGIONS {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if region_count == 0 {
            return Err(ZxError::INVALID_ARGS);
        }

        // Build regions from the descriptor array.
        // For now, we create private VMO-backed regions with the
        // specified sizes. Full zx_iob_region_t parsing would require
        // reading the struct from user memory; we use a simplified
        // approach where each region gets a page-aligned VMO.
        //
        // TODO: parse full zx_iob_region_t from userspace once the
        // ABI struct is fully defined and the region descriptor
        // format is stabilized.
        let mut regions = alloc::vec::Vec::with_capacity(region_count);
        for _ in 0..region_count {
            // Default: 4 KiB private region with full access for both endpoints.
            let size = 4096;
            let vmo = VmObject::new_paged(1); // 1 page
            regions.push(IoBufferRegion::new(vmo, size, 0xF)); // all access bits
        }

        let (ep0, ep1) = IoBuffer::create(regions)?;
        let proc = self.thread.proc();
        let handle0 = proc.add_handle(Handle::new(ep0, Rights::DEFAULT_IOB));
        let handle1 = proc.add_handle(Handle::new(ep1, Rights::DEFAULT_IOB));
        ep0_out.write(handle0)?;
        ep1_out.write(handle1)?;
        Ok(())
    }

    /// Write data to an IOBuffer region.
    ///
    /// Writes an iovec-style scatter/gather list to the specified
    /// region of an IOBuffer endpoint.
    pub fn sys_iob_writev(
        &self,
        handle: HandleValue,
        region_index: u32,
        _options: u32,
        _iovecs: usize,
        _iovec_count: usize,
    ) -> ZxResult {
        info!("iob.writev: handle={:#x}, region={}", handle, region_index);
        let proc = self.thread.proc();
        let iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::WRITE)?;
        if region_index as usize >= iob.region_count() {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // TODO: implement scatter/gather write into the region's VMO.
        warn!("iob.writev: validated but write not yet implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Allocate a unique ID from an IOBuffer region with ID allocator
    /// discipline.
    pub fn sys_iob_allocate_id(
        &self,
        handle: HandleValue,
        region_index: u32,
        _options: u32,
    ) -> ZxResult {
        info!(
            "iob.allocate_id: handle={:#x}, region={}",
            handle, region_index
        );
        let proc = self.thread.proc();
        let iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::WRITE)?;
        if region_index as usize >= iob.region_count() {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // TODO: implement ID allocator discipline.
        warn!("iob.allocate_id: validated but not yet implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Create a shared region for cross-IOB sharing.
    ///
    /// This is an experimental syscall that creates a shared memory
    /// region that can be referenced by multiple IOBuffer pairs.
    pub fn sys_iob_create_shared_region(
        &self,
        _options: u64,
        _size: u64,
        _out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!("iob.create_shared_region");
        // TODO: implement shared region creation.
        warn!("iob.create_shared_region: not yet implemented");
        Err(ZxError::NOT_SUPPORTED)
    }
}
