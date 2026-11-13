use {
    super::*,
    crate::object::*,
    crate::vm::*,
    alloc::{sync::Arc, vec::Vec},
    dev::Iommu,
    lock::Mutex,
};

/// Bus Transaction Initiator.
///
/// Bus Transaction Initiators (BTIs) represent the bus mastering/DMA capability
/// of a device, and can be used for granting a device access to memory.
pub struct BusTransactionInitiator {
    base: KObjectBase,
    iommu: Arc<Iommu>,
    #[allow(dead_code)]
    bti_id: u64,
    inner: Mutex<BtiInner>,
}

#[derive(Default)]
struct BtiInner {
    /// A BTI manages a list of quarantined PMTs.
    pmts: Vec<Arc<PinnedMemoryToken>>,
}

impl_kobject!(BusTransactionInitiator);

impl BusTransactionInitiator {
    /// Create a new bus transaction initiator.
    pub fn create(iommu: Arc<Iommu>, bti_id: u64) -> Arc<Self> {
        Arc::new(BusTransactionInitiator {
            base: KObjectBase::new(),
            iommu,
            bti_id,
            inner: Mutex::new(BtiInner::default()),
        })
    }

    /// Get information of BTI.
    pub fn get_info(&self) -> BtiInfo {
        BtiInfo {
            minimum_contiguity: self.iommu.minimum_contiguity() as u64,
            aspace_size: self.iommu.aspace_size() as u64,
            pmo_count: self.pmo_count() as u64,
            quarantine_count: self.quarantine_count() as u64,
        }
    }

    /// Pin memory and grant access to it to the BTI.
    pub fn pin(
        self: &Arc<Self>,
        vmo: Arc<VmObject>,
        offset: usize,
        size: usize,
        perms: IommuPerms,
    ) -> ZxResult<Arc<PinnedMemoryToken>> {
        if size == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        // Reject new pins if there are quarantined PMTs (PMTs whose
        // handles were closed without unpinning). Matches Fuchsia's
        // behavior where quarantined entries block new pins until
        // release_quarantine is called.
        if self.quarantine_count() > 0 {
            return Err(ZxError::BAD_STATE);
        }
        let pmt = PinnedMemoryToken::create(self, vmo, perms, offset, size)?;
        self.inner.lock().pmts.push(pmt.clone());
        Ok(pmt)
    }

    /// Releases all quarantined PMTs.
    pub fn release_quarantine(&self) {
        let mut inner = self.inner.lock();
        // remove no handle, the only Arc is from self.pmts
        inner.pmts.retain(|pmt| Arc::strong_count(pmt) > 1);
    }

    /// Release a PMT by KoID.
    pub fn release_pmt(&self, id: KoID) {
        let mut inner = self.inner.lock();
        inner.pmts.retain(|pmt| pmt.id() != id);
    }

    pub(super) fn iommu(&self) -> Arc<Iommu> {
        self.iommu.clone()
    }

    fn pmo_count(&self) -> usize {
        self.inner.lock().pmts.len()
    }

    fn quarantine_count(&self) -> usize {
        self.inner
            .lock()
            .pmts
            .iter()
            .filter(|pmt| Arc::strong_count(pmt) == 1)
            .count()
    }
}

/// Information of BTI.
#[repr(C)]
#[derive(Default)]
pub struct BtiInfo {
    /// Minimum contiguity guaranteed by the IOMMU.
    pub minimum_contiguity: u64,
    /// Size of the address space managed by the IOMMU.
    pub aspace_size: u64,
    /// Number of pinned memory objects (PMTs).
    pub pmo_count: u64,
    /// Number of quarantined PMTs (dropped without unpin).
    pub quarantine_count: u64,
}
