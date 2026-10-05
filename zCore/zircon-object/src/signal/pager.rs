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
        options: u32,
        port: &Arc<Port>,
        key: u64,
        size: u64,
    ) -> ZxResult<Arc<VmObject>> {
        const ZX_VMO_TRAP_DIRTY: u32 = 1 << 3;
        let trap_dirty = options & ZX_VMO_TRAP_DIRTY != 0;
        let pages = (size as usize).div_ceil(PAGE_SIZE);
        // Pager-backed VMOs are always resizable in Fuchsia.
        let vmo = VmObject::new_paged_with_resizable(true, pages);
        vmo.set_content_size(vmo.len())?;
        vmo.set_name("pager-vmo");
        // Associate the pager's port and key with the VMO for
        // demand-paging notifications.
        vmo.set_pager(port.clone(), key);
        if trap_dirty {
            vmo.set_trap_dirty(pages);
        }

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
            // Wake any threads blocked on pager faults before clearing
            // the pager association — they'll get NOT_FOUND.
            vmo.fail_pager_requests(ZxError::NOT_FOUND);
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
            if let Err(e) = aux_vmo.read(src_off, &mut buf[..chunk]) {
                // Wake waiters for the pages written so far.
                let written = dst_off - offset as usize;
                if written > 0 {
                    vmo.complete_pager_requests(offset as usize, written);
                }
                return Err(e);
            }
            if let Err(e) = vmo.write(dst_off, &buf[..chunk]) {
                let written = dst_off - offset as usize;
                if written > 0 {
                    vmo.complete_pager_requests(offset as usize, written);
                }
                return Err(e);
            }
            src_off += chunk;
            dst_off += chunk;
        }
        // Wake threads waiting for pages in this range.
        vmo.complete_pager_requests(offset as usize, length as usize);
        Ok(())
    }

    /// Perform a pager operation on a range of a pager-backed VMO.
    pub fn op_range(
        &self,
        op: u32,
        vmo: &Arc<VmObject>,
        offset: u64,
        length: u64,
        data: u64,
    ) -> ZxResult {
        // Verify the VMO belongs to this pager.
        let inner = self.inner.lock();
        if !inner.vmos.iter().any(|pv| Arc::ptr_eq(&pv.vmo, vmo)) {
            return Err(ZxError::INVALID_ARGS);
        }
        drop(inner); // Release pager lock before operating on VMO.
        match op {
            ZX_PAGER_OP_FAIL => {
                // Fail waiting threads for the specified range with
                // the error code in `data`. Map common Fuchsia errors;
                // default to IO for unknown codes.
                let err = match data as i32 {
                    -54 => ZxError::NO_SPACE,
                    -5 => ZxError::IO,
                    -29 => ZxError::IO_DATA_INTEGRITY,
                    -45 => ZxError::BAD_STATE,
                    _ => ZxError::IO,
                };
                vmo.fail_pager_requests_range(offset as usize, length as usize, err);
                Ok(())
            }
            ZX_PAGER_OP_DIRTY => {
                // Grant dirty permission for the range. Wakes threads
                // blocked on dirty traps.
                vmo.mark_pages_dirty(offset as usize, length as usize);
                Ok(())
            }
            ZX_PAGER_OP_WRITEBACK_BEGIN | ZX_PAGER_OP_WRITEBACK_END => {
                // Writeback lifecycle — currently a no-op since we
                // don't distinguish clean/writeback/dirty states.
                Ok(())
            }
            _ => Err(ZxError::NOT_SUPPORTED),
        }
    }
}

use hal::PAGE_SIZE;

// Pager operation codes (from Fuchsia's zircon/types.h)
const ZX_PAGER_OP_FAIL: u32 = 1;
const ZX_PAGER_OP_DIRTY: u32 = 2;
const ZX_PAGER_OP_WRITEBACK_BEGIN: u32 = 3;
const ZX_PAGER_OP_WRITEBACK_END: u32 = 4;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::Port;
    use crate::vm::VmObject;

    fn test_port() -> Arc<Port> {
        Port::new(0).unwrap()
    }

    #[test]
    fn create_pager() {
        let pager = Pager::new();
        assert_eq!(pager.inner.lock().vmos.len(), 0);
    }

    #[test]
    fn create_vmo() {
        let pager = Pager::new();
        let port = test_port();
        let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
        assert_eq!(vmo.len(), PAGE_SIZE);
        assert_eq!(pager.inner.lock().vmos.len(), 1);
    }

    #[test]
    fn detach_vmo() {
        let pager = Pager::new();
        let port = test_port();
        let vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
        assert_eq!(pager.inner.lock().vmos.len(), 1);
        pager.detach_vmo(&vmo).unwrap();
        assert_eq!(pager.inner.lock().vmos.len(), 0);

        // Detaching again should fail
        assert!(pager.detach_vmo(&vmo).is_err());
    }

    #[test]
    fn supply_pages() {
        let pager = Pager::new();
        let port = test_port();
        let pager_vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();

        // Create a source VMO with data
        let src_vmo = VmObject::new_paged(1);
        src_vmo.write(0, b"pager data!").unwrap();

        // Supply the page
        let result = pager.supply_pages(&pager_vmo, 0, PAGE_SIZE as u64, &src_vmo, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn op_range_on_wrong_vmo() {
        let pager = Pager::new();
        let port = test_port();
        let _pager_vmo = pager.create_vmo(0, &port, 0, PAGE_SIZE as u64).unwrap();
        let other_vmo = VmObject::new_paged(1);
        // Operating on a VMO that doesn't belong to this pager should fail
        assert!(pager
            .op_range(ZX_PAGER_OP_FAIL, &other_vmo, 0, PAGE_SIZE as u64, 0)
            .is_err());
    }
}
