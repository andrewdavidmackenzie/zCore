//! Zircon pager object.
//!
//! A pager creates VMOs whose pages are provided on demand by a
//! userspace pager process. When a thread faults on a pager-backed
//! VMO page, the kernel sends a `ZX_PAGER_VMO_READ` packet to the
//! pager's port. The pager supplies pages via `pager_supply_pages`.

use crate::object::*;
use crate::signal::port::*;
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

/// Per-VMO pager association.
#[allow(dead_code)] // port/key used when demand-paging notification is implemented
struct PagerVmo {
    vmo: Arc<VmObject>,
    port: Arc<Port>,
    key: u64,
}

#[derive(Default)]
struct PagerInner {
    /// VMOs backed by this pager, with their port/key associations.
    vmos: Vec<PagerVmo>,
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
        port: &Arc<Port>,
        key: u64,
        size: u64,
    ) -> ZxResult<Arc<VmObject>> {
        let pages = (size as usize).div_ceil(PAGE_SIZE);
        let vmo = VmObject::new_paged(pages);
        vmo.set_name("pager-vmo");
        // Associate the pager's port and key with the VMO for
        // demand-paging notifications.
        vmo.set_pager(port.clone(), key);

        let mut inner = self.inner.lock();
        inner.vmos.push(PagerVmo {
            vmo: vmo.clone(),
            port: port.clone(),
            key,
        });
        Ok(vmo)
    }

    /// Detach a VMO from this pager.
    ///
    /// After detaching, page faults on the VMO will no longer be
    /// forwarded to the pager.
    pub fn detach_vmo(&self, vmo: &Arc<VmObject>) -> ZxResult {
        let mut inner = self.inner.lock();
        if let Some(pos) = inner.vmos.iter().position(|pv| Arc::ptr_eq(&pv.vmo, vmo)) {
            inner.vmos.remove(pos);
            vmo.clear_pager();
            Ok(())
        } else {
            Err(ZxError::NOT_FOUND)
        }
    }

    /// Supply pages to a pager-backed VMO from an auxiliary VMO.
    ///
    /// Copies `length` bytes starting at `offset` in the pager VMO
    /// from `aux_offset` in `aux_vmo`.
    pub fn supply_pages(
        &self,
        vmo: &Arc<VmObject>,
        offset: u64,
        length: u64,
        aux_vmo: &Arc<VmObject>,
        aux_offset: u64,
    ) -> ZxResult {
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
        // Notify any threads waiting for these pages.
        vmo.notify_pages_supplied();
        Ok(())
    }

    /// Perform a pager operation on a range of a pager-backed VMO.
    pub fn op_range(
        &self,
        op: u32,
        vmo: &Arc<VmObject>,
        _offset: u64,
        _length: u64,
        _data: u64,
    ) -> ZxResult {
        // Verify the VMO belongs to this pager.
        let inner = self.inner.lock();
        if !inner.vmos.iter().any(|pv| Arc::ptr_eq(&pv.vmo, vmo)) {
            return Err(ZxError::INVALID_ARGS);
        }
        match op {
            ZX_PAGER_OP_FAIL => {
                // Mark pages in the range as failed. Threads waiting
                // for these pages will get an error.
                // TODO: implement page failure notification
                warn!("pager.op_range: FAIL op not fully implemented");
                Ok(())
            }
            ZX_PAGER_OP_DIRTY => {
                // Mark pages as dirty (for writeback pagers).
                // TODO: implement dirty page tracking
                warn!("pager.op_range: DIRTY op not fully implemented");
                Ok(())
            }
            ZX_PAGER_OP_WRITEBACK_BEGIN | ZX_PAGER_OP_WRITEBACK_END => {
                // Writeback operations for modified pages.
                // TODO: implement writeback lifecycle
                warn!("pager.op_range: WRITEBACK op not fully implemented");
                Ok(())
            }
            _ => Err(ZxError::NOT_SUPPORTED),
        }
    }
}

use kernel_hal::PAGE_SIZE;

// Pager operation codes (from Fuchsia's zircon/types.h)
const ZX_PAGER_OP_FAIL: u32 = 1;
const ZX_PAGER_OP_DIRTY: u32 = 2;
const ZX_PAGER_OP_WRITEBACK_BEGIN: u32 = 3;
const ZX_PAGER_OP_WRITEBACK_END: u32 = 4;
