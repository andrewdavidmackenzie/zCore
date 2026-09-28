//! IOBuffer (I/O Buffer) — shared-memory IPC primitive.
//!
//! An IOBuffer is a peered kernel object that encapsulates one or more
//! memory regions for high-throughput, low-latency point-to-point IPC.
//! Each IOBuffer has two endpoints (like Channel/Socket) with
//! per-region, per-endpoint access control.

use {
    crate::object::*,
    crate::vm::VmObject,
    alloc::sync::{Arc, Weak},
    alloc::vec::Vec,
    lock::Mutex,
};

/// Maximum number of regions per IOBuffer.
pub const IOB_MAX_REGIONS: usize = 64;

/// IOB discipline types.
pub const IOB_DISCIPLINE_NONE: u32 = 0;
/// ID allocator discipline: thread-safe atomic ID assignment.
pub const IOB_DISCIPLINE_ID_ALLOCATOR: u32 = 1;

/// Per-region ID allocator state.
struct IdAllocator {
    /// Next ID to allocate.
    next_id: u32,
    /// Blob data keyed by allocated IDs (id → blob bytes).
    blobs: alloc::collections::BTreeMap<u32, Vec<u8>>,
}

impl IdAllocator {
    fn new() -> Self {
        Self {
            next_id: 1,
            blobs: alloc::collections::BTreeMap::new(),
        }
    }

    fn allocate(&mut self, blob: Vec<u8>) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.blobs.insert(id, blob);
        id
    }
}

/// A single memory region within an IOBuffer.
pub struct IoBufferRegion {
    /// Backing VMO for this region.
    vmo: Arc<VmObject>,
    /// Size in bytes.
    size: usize,
    /// Access flags (which endpoint can map read/write).
    access: u64,
    /// Discipline type for this region.
    discipline: u32,
    /// ID allocator state (only for ID_ALLOCATOR discipline).
    /// Shared between endpoints via Arc.
    id_allocator: Option<Arc<Mutex<IdAllocator>>>,
}

impl IoBufferRegion {
    /// Create a new IOBuffer region with no discipline.
    pub fn new(vmo: Arc<VmObject>, size: usize, access: u64) -> Self {
        Self {
            vmo,
            size,
            access,
            discipline: IOB_DISCIPLINE_NONE,
            id_allocator: None,
        }
    }

    /// Create a new IOBuffer region with ID allocator discipline.
    pub fn new_with_id_allocator(vmo: Arc<VmObject>, size: usize, access: u64) -> Self {
        Self {
            vmo,
            size,
            access,
            discipline: IOB_DISCIPLINE_ID_ALLOCATOR,
            id_allocator: Some(Arc::new(Mutex::new(IdAllocator::new()))),
        }
    }

    /// Get the discipline type.
    pub fn discipline(&self) -> u32 {
        self.discipline
    }

    /// Allocate an ID in this region (must be ID_ALLOCATOR discipline).
    pub fn allocate_id(&self, blob: Vec<u8>) -> ZxResult<u32> {
        let allocator = self.id_allocator.as_ref().ok_or(ZxError::WRONG_TYPE)?;
        Ok(allocator.lock().allocate(blob))
    }
}

/// Inner mutable state of an IoBuffer endpoint.
struct IoBufferInner {
    /// Memory regions shared between the two endpoints.
    regions: Vec<IoBufferRegion>,
}

/// An IOBuffer endpoint. Always created in pairs (ep0, ep1).
///
/// IOBuffers provide shared-memory IPC with configurable per-region
/// access control. Regions are backed by VMOs that can be mapped
/// into the process address space via `zx_vmar_map_iob`.
pub struct IoBuffer {
    base: KObjectBase,
    _counter: CountHelper,
    /// Weak reference to the peer endpoint (interior mutable for init).
    peer: Mutex<Weak<IoBuffer>>,
    /// Which endpoint this is (0 or 1).
    endpoint_index: u32,
    /// Shared state (regions are shared between endpoints).
    inner: Mutex<IoBufferInner>,
}

impl_kobject!(IoBuffer
    fn peer(&self) -> ZxResult<Arc<dyn KernelObject>> {
        let peer = self.peer.lock().upgrade().ok_or(ZxError::PEER_CLOSED)?;
        Ok(peer)
    }
    fn related_koid(&self) -> KoID {
        self.peer.lock().upgrade().map(|p| p.id()).unwrap_or(0)
    }
);
define_count_helper!(IoBuffer);

impl IoBuffer {
    /// Create an IOBuffer pair with the given regions.
    ///
    /// Returns `(ep0, ep1)`. Both endpoints share the same set of
    /// memory regions, with access controlled by per-region flags.
    pub fn create(regions: Vec<IoBufferRegion>) -> ZxResult<(Arc<Self>, Arc<Self>)> {
        if regions.len() > IOB_MAX_REGIONS {
            return Err(ZxError::OUT_OF_RANGE);
        }

        // Build region list for ep1 (shares VMOs and ID allocators).
        let shared_regions: Vec<IoBufferRegion> = regions
            .iter()
            .map(|r| IoBufferRegion {
                vmo: r.vmo.clone(),
                size: r.size,
                access: r.access,
                discipline: r.discipline,
                id_allocator: r.id_allocator.clone(),
            })
            .collect();

        let ep0 = Arc::new(IoBuffer {
            base: KObjectBase::with_signal(Signal::WRITABLE),
            _counter: CountHelper::new(),
            peer: Mutex::new(Weak::default()),
            endpoint_index: 0,
            inner: Mutex::new(IoBufferInner { regions }),
        });

        let ep1 = Arc::new(IoBuffer {
            base: KObjectBase::with_signal(Signal::WRITABLE),
            _counter: CountHelper::new(),
            peer: Mutex::new(Arc::downgrade(&ep0)),
            endpoint_index: 1,
            inner: Mutex::new(IoBufferInner {
                regions: shared_regions,
            }),
        });

        // Set ep0's peer to ep1 via interior mutability.
        *ep0.peer.lock() = Arc::downgrade(&ep1);

        Ok((ep0, ep1))
    }

    /// Get the endpoint index (0 or 1).
    pub fn endpoint_index(&self) -> u32 {
        self.endpoint_index
    }

    /// Get the number of regions.
    pub fn region_count(&self) -> usize {
        self.inner.lock().regions.len()
    }

    /// Get a region's VMO and size by index.
    pub fn get_region(&self, index: usize) -> ZxResult<(Arc<VmObject>, usize, u64)> {
        let inner = self.inner.lock();
        let region = inner.regions.get(index).ok_or(ZxError::OUT_OF_RANGE)?;
        Ok((region.vmo.clone(), region.size, region.access))
    }

    /// Allocate an ID from a region with ID allocator discipline.
    ///
    /// The blob data is stored alongside the allocated ID.
    pub fn allocate_id(&self, region_index: usize, blob: Vec<u8>) -> ZxResult<u32> {
        let inner = self.inner.lock();
        let region = inner
            .regions
            .get(region_index)
            .ok_or(ZxError::OUT_OF_RANGE)?;
        region.allocate_id(blob)
    }

    /// Check if the peer endpoint is closed.
    pub fn peer_closed(&self) -> bool {
        self.peer.lock().upgrade().is_none()
    }
}

/// A shared memory region that can be referenced by multiple IOBuffer pairs.
///
/// Created via `zx_iob_create_shared_region`. The backing VMO can be
/// referenced when creating IOBuffer pairs with `ZX_IOB_REGION_TYPE_SHARED`.
pub struct IoBufferSharedRegion {
    base: KObjectBase,
    /// Backing VMO for this shared region.
    vmo: Arc<VmObject>,
    /// Size in bytes (page-aligned).
    size: usize,
}

impl_kobject!(IoBufferSharedRegion);

impl IoBufferSharedRegion {
    /// Create a new shared region backed by a VMO.
    pub fn create(size: usize) -> ZxResult<Arc<Self>> {
        if size == 0 || !size.is_multiple_of(crate::vm::PAGE_SIZE) {
            return Err(ZxError::INVALID_ARGS);
        }
        let pages = size / crate::vm::PAGE_SIZE;
        let vmo = VmObject::new_paged(pages);
        vmo.set_name("iob-shared-region");
        Ok(Arc::new(Self {
            base: KObjectBase::new(),
            vmo,
            size,
        }))
    }

    /// Get the backing VMO.
    pub fn vmo(&self) -> &Arc<VmObject> {
        &self.vmo
    }

    /// Get the size in bytes.
    pub fn size(&self) -> usize {
        self.size
    }
}

impl Drop for IoBuffer {
    fn drop(&mut self) {
        if let Some(peer) = self.peer.lock().upgrade() {
            peer.base
                .signal_change(Signal::WRITABLE, Signal::PEER_CLOSED);
        }
    }
}
