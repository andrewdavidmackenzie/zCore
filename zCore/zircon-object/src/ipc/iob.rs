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

/// A single memory region within an IOBuffer.
pub struct IoBufferRegion {
    /// Backing VMO for this region.
    vmo: Arc<VmObject>,
    /// Size in bytes.
    size: usize,
    /// Access flags (which endpoint can map read/write).
    access: u64,
}

impl IoBufferRegion {
    /// Create a new IOBuffer region.
    pub fn new(vmo: Arc<VmObject>, size: usize, access: u64) -> Self {
        Self { vmo, size, access }
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

        // Build region list for ep1 (shares the same VMOs).
        let shared_regions: Vec<IoBufferRegion> = regions
            .iter()
            .map(|r| IoBufferRegion {
                vmo: r.vmo.clone(),
                size: r.size,
                access: r.access,
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

    /// Check if the peer endpoint is closed.
    pub fn peer_closed(&self) -> bool {
        self.peer.lock().upgrade().is_none()
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
