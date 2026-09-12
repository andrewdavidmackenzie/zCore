//! Zircon pager object.
//!
//! A pager creates VMOs whose pages are provided on demand by a
//! userspace pager process. When a thread faults on a pager-backed
//! VMO page, the kernel sends a `ZX_PAGER_VMO_READ` packet to the
//! pager's port. The pager supplies pages via `pager_supply_pages`.

use crate::object::*;
use crate::vm::VmObject;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lock::Mutex;

/// A pager object that manages demand-paged VMOs.
pub struct Pager {
    base: KObjectBase,
    inner: Mutex<PagerInner>,
}

impl_kobject!(Pager);

#[derive(Default)]
struct PagerInner {
    /// VMOs backed by this pager.
    vmos: Vec<Arc<VmObject>>,
}

impl Pager {
    /// Create a new pager.
    pub fn new() -> Arc<Self> {
        Arc::new(Pager {
            base: KObjectBase::new(),
            inner: Mutex::new(PagerInner::default()),
        })
    }

    /// Create a VMO backed by this pager, associated with a port and key.
    ///
    /// When a page fault occurs on the VMO, a packet with the given
    /// `key` is sent to the `port`.
    pub fn create_vmo(
        self: &Arc<Self>,
        _options: u32,
        _port: &Arc<super::Port>,
        _key: u64,
        size: u64,
    ) -> ZxResult<Arc<VmObject>> {
        // For now, create a standard paged VMO.
        // TODO: implement true demand-paging with port notification
        // on page fault and lazy page commitment.
        let pages = (size as usize).div_ceil(PAGE_SIZE);
        let vmo = VmObject::new_paged(pages);
        vmo.set_name("pager-vmo");

        let mut inner = self.inner.lock();
        inner.vmos.push(vmo.clone());
        Ok(vmo)
    }

    /// Detach a VMO from this pager.
    pub fn detach_vmo(&self, vmo: &Arc<VmObject>) -> ZxResult {
        let mut inner = self.inner.lock();
        if let Some(pos) = inner.vmos.iter().position(|v| Arc::ptr_eq(v, vmo)) {
            inner.vmos.remove(pos);
            Ok(())
        } else {
            Err(ZxError::NOT_FOUND)
        }
    }

    /// Supply pages to a pager-backed VMO from an auxiliary VMO.
    pub fn supply_pages(
        &self,
        vmo: &Arc<VmObject>,
        offset: u64,
        length: u64,
        aux_vmo: &Arc<VmObject>,
        aux_offset: u64,
    ) -> ZxResult {
        // Copy pages from aux_vmo into the pager-backed vmo.
        let mut buf = alloc::vec![0u8; PAGE_SIZE];
        let mut src_off = aux_offset as usize;
        let mut dst_off = offset as usize;
        let end = offset as usize + length as usize;

        while dst_off < end {
            let chunk = PAGE_SIZE.min(end - dst_off);
            aux_vmo.read(src_off, &mut buf[..chunk])?;
            vmo.write(dst_off, &buf[..chunk])?;
            src_off += chunk;
            dst_off += chunk;
        }
        Ok(())
    }
}

use kernel_hal::PAGE_SIZE;
