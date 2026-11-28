use super::*;
use zircon_object::ipc::{IoBuffer, IoBufferRegion, IoBufferSharedRegion, IOB_MAX_REGIONS};
use zircon_object::vm::{pages, VmObject};

impl Syscall<'_> {
    /// Create an IOBuffer (I/O Buffer) object pair.
    ///
    /// IOBuffers are shared-memory IPC primitives with configurable
    /// per-region access control. Returns handles to both endpoints.
    ///
    /// `options` must be 0. `regions_ptr` points to an array of region
    /// descriptors. `region_count` is the number of regions (max 64).
    ///
    /// Region descriptor: `zx_iob_region_t`
    /// (zircon/syscalls/iob.h + syscalls-next.h), repr(C), 120 bytes:
    /// ```text
    ///   0:  type       (u32) — 0 = PRIVATE, 1 = SHARED
    ///   4:  access     (u32) — ZX_IOB_ACCESS_* bitfield (8 bits)
    ///   8:  size       (u64) — PRIVATE: region size; SHARED: ignored
    ///  16:  discipline (72 bytes: type u64 + 64 reserved)
    ///  88:  extension  (32 bytes; SHARED: options u32, handle u32,
    ///                  padding u64[3])
    /// ```
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
        self.enforce_basic_policy(PolicyCondition::NewIOB)?;
        if region_count > IOB_MAX_REGIONS {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if region_count == 0 {
            return Err(ZxError::INVALID_ARGS);
        }

        // Read region descriptors from userspace as raw bytes.
        // We parse fields manually to avoid unsafe reinterpret casts
        // (crate denies unsafe).
        const DESC_SIZE: usize = 120; // sizeof(zx_iob_region_t)
        const ZX_IOB_REGION_TYPE_PRIVATE: u32 = 0;
        const ZX_IOB_REGION_TYPE_SHARED: u32 = 1;
        const ZX_IOB_DISCIPLINE_TYPE_NONE: u64 = 0;
        const ZX_IOB_DISCIPLINE_TYPE_ID_ALLOCATOR: u64 = 1;
        const ZX_IOB_DISCIPLINE_TYPE_MEDIATED_WRITE_RING_BUFFER: u64 = 2;

        let raw = regions_ptr.read_array(region_count * DESC_SIZE)?;

        let proc = self.thread.proc();
        let mut regions = alloc::vec::Vec::with_capacity(region_count);
        for i in 0..region_count {
            let b = i * DESC_SIZE;
            let region_type = u32::from_ne_bytes([raw[b], raw[b + 1], raw[b + 2], raw[b + 3]]);
            let access =
                u32::from_ne_bytes([raw[b + 4], raw[b + 5], raw[b + 6], raw[b + 7]]) as u64;
            let size = u64::from_le_bytes(raw[b + 8..b + 16].try_into().unwrap());
            let discipline_type = u64::from_le_bytes(raw[b + 16..b + 24].try_into().unwrap());

            if access & !0xff != 0 {
                return Err(ZxError::INVALID_ARGS);
            }
            match discipline_type {
                ZX_IOB_DISCIPLINE_TYPE_NONE
                | ZX_IOB_DISCIPLINE_TYPE_ID_ALLOCATOR
                | ZX_IOB_DISCIPLINE_TYPE_MEDIATED_WRITE_RING_BUFFER => {}
                _ => return Err(ZxError::NOT_SUPPORTED),
            }

            let (vmo, region_size) = match region_type {
                ZX_IOB_REGION_TYPE_PRIVATE => {
                    if size == 0 {
                        return Err(ZxError::INVALID_ARGS);
                    }
                    let num_pages = pages(size as usize);
                    (VmObject::new_paged(num_pages), size as usize)
                }
                ZX_IOB_REGION_TYPE_SHARED => {
                    // The region references an existing shared region
                    // (created via zx_iob_create_shared_region) by
                    // handle, carried in the extension:
                    // zx_iob_region_shared_t at offset 88:
                    // options u32, shared_region handle u32, padding.
                    let handle_value =
                        u32::from_le_bytes(raw[b + 92..b + 96].try_into().unwrap()) as HandleValue;
                    let shared = proc
                        .get_object_with_rights::<IoBufferSharedRegion>(handle_value, Rights::READ)
                        .map_err(|_| ZxError::INVALID_ARGS)?;
                    (shared.vmo().clone(), shared.size())
                }
                _ => return Err(ZxError::INVALID_ARGS),
            };

            let region = if discipline_type == ZX_IOB_DISCIPLINE_TYPE_ID_ALLOCATOR {
                IoBufferRegion::new_with_id_allocator(vmo, region_size, access)
            } else {
                IoBufferRegion::new_with_discipline(vmo, region_size, access, discipline_type)
            };
            regions.push(region);
        }

        let (ep0, ep1) = IoBuffer::create(regions)?;
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
    /// Reads an iovec-style scatter/gather list from userspace and
    /// writes the gathered data sequentially into the region's VMO.
    pub fn sys_iob_writev(
        &self,
        handle: HandleValue,
        options: u32,
        region_index: u32,
        iovecs_ptr: UserInPtr<u8>,
        iovec_count: usize,
    ) -> ZxResult {
        info!(
            "iob.writev: handle={:#x}, options={}, region={}, count={}",
            handle, options, region_index, iovec_count
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        if iovec_count == 0 {
            return Ok(());
        }
        let proc = self.thread.proc();
        let iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::WRITE)?;
        let (vmo, region_size, access) = iob.get_region(region_index as usize)?;

        // Check write permission for this endpoint.
        let ep_idx = iob.endpoint_index();
        let can_write = if ep_idx == 0 {
            access & 0x02 != 0 // EP0_CAN_MAP_WRITE
        } else {
            access & 0x20 != 0 // EP1_CAN_MAP_WRITE
        };
        if !can_write {
            return Err(ZxError::ACCESS_DENIED);
        }

        // Read iovec descriptors from userspace.
        // Each iovec is 16 bytes: [ptr: u64, len: u64] on 64-bit.
        let iovec_size = 16usize;
        let total_bytes = iovec_count * iovec_size;
        let raw = iovecs_ptr.read_array(total_bytes)?;

        // Gather data from iovecs and write sequentially into the VMO.
        let mut offset = 0usize;
        for i in 0..iovec_count {
            let b = i * iovec_size;
            let ptr = usize::from_ne_bytes(
                raw[b..b + core::mem::size_of::<usize>()]
                    .try_into()
                    .map_err(|_| ZxError::INVALID_ARGS)?,
            );
            let len = usize::from_ne_bytes(
                raw[b + core::mem::size_of::<usize>()..b + iovec_size]
                    .try_into()
                    .map_err(|_| ZxError::INVALID_ARGS)?,
            );
            if len == 0 {
                continue;
            }
            if offset + len > region_size {
                return Err(ZxError::OUT_OF_RANGE);
            }
            // Read data from the user buffer.
            let user_buf: UserInPtr<u8> = ptr.into();
            let data = user_buf.read_array(len)?;
            // Write into the region's VMO.
            vmo.write(offset, &data)?;
            offset += len;
        }
        Ok(())
    }

    /// Allocate a unique ID from an IOBuffer region with ID allocator
    /// discipline.
    ///
    /// The region must have `ZX_IOB_DISCIPLINE_TYPE_ID_ALLOCATOR` and
    /// the endpoint must have mediated write access. The blob data is
    /// stored alongside the allocated ID.
    #[allow(clippy::too_many_arguments)]
    pub fn sys_iob_allocate_id(
        &self,
        handle: HandleValue,
        options: u32,
        region_index: u32,
        blob_ptr: usize,
        blob_size: usize,
        mut out_id: UserOutPtr<u32>,
    ) -> ZxResult {
        info!(
            "iob.allocate_id: handle={:#x}, options={}, region={}, blob_size={}",
            handle, options, region_index, blob_size
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let iob = proc.get_object_with_rights::<IoBuffer>(handle, Rights::WRITE)?;

        // Check mediated write access for this endpoint.
        let (_, _, access) = iob.get_region(region_index as usize)?;
        let ep_idx = iob.endpoint_index();
        let can_mediated_write = if ep_idx == 0 {
            access & 0x08 != 0 // EP0_CAN_MEDIATED_WRITE
        } else {
            access & 0x80 != 0 // EP1_CAN_MEDIATED_WRITE
        };
        if !can_mediated_write {
            return Err(ZxError::ACCESS_DENIED);
        }

        // Read blob data from userspace.
        let blob = if blob_size > 0 {
            let buf: UserInPtr<u8> = blob_ptr.into();
            buf.read_array(blob_size)?
        } else {
            alloc::vec::Vec::new()
        };

        // Allocate the ID.
        let id = iob.allocate_id(region_index as usize, blob)?;
        out_id.write(id)?;
        Ok(())
    }

    /// Create a shared memory region for cross-IOB sharing.
    ///
    /// Creates a standalone shared region backed by a VMO that can be
    /// referenced by multiple IOBuffer pairs via `ZX_IOB_REGION_TYPE_SHARED`.
    pub fn sys_iob_create_shared_region(
        &self,
        options: u64,
        size: u64,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "iob.create_shared_region: options={}, size={:#x}",
            options, size
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        self.enforce_basic_policy(PolicyCondition::NewIOB)?;
        let region = zircon_object::ipc::IoBufferSharedRegion::create(size as usize)?;
        let proc = self.thread.proc();
        let handle = proc.add_handle(Handle::new(region, Rights::DEFAULT_VMO));
        out.write(handle)?;
        Ok(())
    }
}
