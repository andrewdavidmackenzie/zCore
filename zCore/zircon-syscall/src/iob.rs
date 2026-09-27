use super::*;
use zircon_object::ipc::{IoBuffer, IoBufferRegion, IOB_MAX_REGIONS};
use zircon_object::vm::{pages, VmObject};

impl Syscall<'_> {
    /// Create an IOBuffer (I/O Buffer) object pair.
    ///
    /// IOBuffers are shared-memory IPC primitives with configurable
    /// per-region access control. Returns handles to both endpoints.
    ///
    /// `options` must be 0.
    /// `regions_ptr` points to an array of region descriptors.
    /// `region_count` is the number of regions (max 64).
    pub fn sys_iob_create(
        &self,
        options: u64,
        regions_ptr: UserInPtr<u8>,
        region_count: usize,
        mut ep0_out: UserOutPtr<HandleValue>,
        mut ep1_out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "iob.create: options={}, regions={:?}, count={}",
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

        // Read region descriptors from userspace as raw bytes.
        // Each descriptor is 64 bytes. We parse fields manually to
        // avoid unsafe reinterpret casts (crate denies unsafe).
        let desc_size = 64usize;
        let total_bytes = region_count * desc_size;
        let raw = regions_ptr.read_array(total_bytes)?;

        let mut regions = alloc::vec::Vec::with_capacity(region_count);
        for i in 0..region_count {
            let b = i * desc_size;
            // IobRegionDesc field offsets (repr(C)):
            // 0: region_type (u32), 8: access (u64), 16: size (u64)
            let region_type = u32::from_ne_bytes([raw[b], raw[b + 1], raw[b + 2], raw[b + 3]]);
            let access = u64::from_ne_bytes([
                raw[b + 8],
                raw[b + 9],
                raw[b + 10],
                raw[b + 11],
                raw[b + 12],
                raw[b + 13],
                raw[b + 14],
                raw[b + 15],
            ]);
            let size = u64::from_ne_bytes([
                raw[b + 16],
                raw[b + 17],
                raw[b + 18],
                raw[b + 19],
                raw[b + 20],
                raw[b + 21],
                raw[b + 22],
                raw[b + 23],
            ]);

            if region_type != 0 {
                return Err(ZxError::INVALID_ARGS);
            }
            if size == 0 {
                return Err(ZxError::INVALID_ARGS);
            }
            let num_pages = pages(size as usize);
            let vmo = VmObject::new_paged(num_pages);
            regions.push(IoBufferRegion::new(vmo, size as usize, access));
        }

        let (ep0, ep1) = IoBuffer::create(regions)?;
        let proc = self.thread.proc();
        let handle0 = proc.add_handle(Handle::new(ep0, Rights::DEFAULT_IOB));
        let handle1 = proc.add_handle(Handle::new(ep1, Rights::DEFAULT_IOB));

        // Write handles to userspace; roll back on failure.
        if ep0_out.write(handle0).is_err() {
            proc.remove_handle(handle0).ok();
            proc.remove_handle(handle1).ok();
            return Err(ZxError::INVALID_ARGS);
        }
        if ep1_out.write(handle1).is_err() {
            proc.remove_handle(handle0).ok();
            proc.remove_handle(handle1).ok();
            return Err(ZxError::INVALID_ARGS);
        }
        Ok(())
    }

    /// Write data to an IOBuffer region.
    ///
    /// Writes an iovec-style scatter/gather list to the specified
    /// region of an IOBuffer endpoint.
    pub fn sys_iob_writev(
        &self,
        handle: HandleValue,
        options: u32,
        region_index: u32,
        _iovecs: usize,
        _iovec_count: usize,
    ) -> ZxResult {
        info!(
            "iob.writev: handle={:#x}, options={}, region={}",
            handle, options, region_index
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
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
    #[allow(clippy::too_many_arguments)]
    pub fn sys_iob_allocate_id(
        &self,
        handle: HandleValue,
        options: u32,
        region_index: u32,
        _blob: usize,
        _blob_size: usize,
        _out_id: UserOutPtr<u64>,
    ) -> ZxResult {
        info!(
            "iob.allocate_id: handle={:#x}, options={}, region={}",
            handle, options, region_index
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
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
    /// This is an experimental upstream syscall that creates a shared
    /// memory region referenced by multiple IOBuffer pairs. Not yet
    /// implemented — returns NOT_SUPPORTED.
    pub fn sys_iob_create_shared_region(
        &self,
        _options: u64,
        _size: u64,
        _out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!("iob.create_shared_region");
        warn!("iob.create_shared_region: experimental, not yet implemented");
        Err(ZxError::NOT_SUPPORTED)
    }
}
