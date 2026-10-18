//! Zircon pager object.
//!
//! A pager creates VMOs whose pages are provided on demand by a
//! userspace pager process. When a thread faults on a pager-backed
//! VMO page, the kernel sends a `ZX_PAGER_VMO_READ` packet to the
//! pager's port. The pager supplies pages via `pager_supply_pages`.

use crate::object::*;
use crate::signal::port::*;
use crate::vm::VmObject;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use lock::Mutex;

/// A pager object that manages demand-paged VMOs.
pub struct Pager {
    base: KObjectBase,
    inner: Mutex<PagerInner>,
}

impl_kobject!(Pager);

/// Per-VMO pager association.
/// Uses `Weak<VmObject>` to avoid reference cycles — the pager must
/// not keep the VMO alive after all userspace handles are closed.
#[allow(dead_code)] // port/key stored for future use (e.g. VMO complete notification)
struct PagerVmo {
    vmo: Weak<VmObject>,
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
        const ZX_VMO_RESIZABLE: u32 = 1 << 1;
        const ZX_VMO_DISCARDABLE: u32 = 1 << 2;
        const ZX_VMO_TRAP_DIRTY: u32 = 1 << 3;
        const ZX_VMO_UNBOUNDED: u32 = 1 << 4;
        // Discardable pager-backed VMOs are not allowed.
        if options & ZX_VMO_DISCARDABLE != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let resizable = options & ZX_VMO_RESIZABLE != 0;
        let unbounded = options & ZX_VMO_UNBOUNDED != 0;
        let trap_dirty = options & ZX_VMO_TRAP_DIRTY != 0;
        // UNBOUNDED and RESIZABLE are mutually exclusive.
        if unbounded && resizable {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate size: must not overflow when rounded up to page boundary.
        // Fuchsia returns OUT_OF_RANGE for sizes that would overflow.
        // Zero size is INVALID_ARGS for pager VMOs (must have content).
        if !unbounded {
            if size == 0 {
                return Err(ZxError::INVALID_ARGS);
            }
            // Check if rounding up to page boundary would overflow u64.
            let page_mask = PAGE_SIZE as u64 - 1;
            if size.checked_add(page_mask).is_none() {
                return Err(ZxError::OUT_OF_RANGE);
            }
        }
        let vmo_pages = if unbounded {
            // UNBOUNDED: VMO capacity is maximum; size arg is just content_size.
            let cap = 256 * 1024usize; // 1GB in pages
                                       // Validate content_size fits within VMO capacity before allocating.
            if size as usize > cap * PAGE_SIZE {
                return Err(ZxError::OUT_OF_RANGE);
            }
            cap
        } else {
            (size as usize).div_ceil(PAGE_SIZE)
        };
        let vmo = VmObject::new_paged_with_resizable(resizable, vmo_pages);
        // Set content_size: for UNBOUNDED, this is the user's requested size;
        // for regular VMOs, it's the VMO's actual (page-rounded) size.
        if unbounded {
            vmo.set_content_size(size as usize)?;
        } else {
            vmo.set_content_size(vmo.len())?;
        }
        vmo.set_name("pager-vmo");
        // Associate the pager's port and key with the VMO for
        // demand-paging notifications. set_pager also marks the VMO
        // as pager-backed so commit_page won't auto-allocate zero frames.
        vmo.set_pager(port.clone(), key);
        if trap_dirty {
            vmo.set_trap_dirty(vmo_pages);
        }

        let mut inner = self.inner.lock();
        inner.vmos.push(PagerVmo {
            vmo: Arc::downgrade(&vmo),
            port: port.clone(),
            key,
        });
        Ok(vmo)
    }

    /// Check if a VMO belongs to this pager.
    pub fn owns_vmo(&self, vmo: &Arc<VmObject>) -> bool {
        let inner = self.inner.lock();
        inner
            .vmos
            .iter()
            .any(|pv| pv.vmo.upgrade().is_some_and(|v| Arc::ptr_eq(&v, vmo)))
    }

    /// Detach a VMO from this pager.
    ///
    /// After detaching, page faults on the VMO will no longer be
    /// forwarded to the pager.
    pub fn detach_vmo(&self, vmo: &Arc<VmObject>) -> ZxResult {
        // Non-pager-backed VMO → INVALID_ARGS.
        if !vmo.is_pager_backed() {
            return Err(ZxError::INVALID_ARGS);
        }
        let mut inner = self.inner.lock();
        if let Some(pos) = inner
            .vmos
            .iter()
            .position(|pv| pv.vmo.upgrade().is_some_and(|v| Arc::ptr_eq(&v, vmo)))
        {
            let pv = inner.vmos.remove(pos);
            // Send COMPLETE packet to notify the pager.
            pv.port.push(PortPacketRepr {
                key: pv.key,
                status: ZxError::OK,
                data: PayloadRepr::PageRequest(PacketPageRequest {
                    command: ZX_PAGER_VMO_COMPLETE,
                    flags: 0,
                    _reserved0: 0,
                    offset: 0,
                    length: 0,
                    _reserved1: 0,
                }),
            });
            // Wake any threads blocked on pager faults before clearing
            // the pager association — they'll get BAD_STATE.
            vmo.fail_pager_requests(ZxError::BAD_STATE);
            vmo.clear_pager();
            Ok(())
        } else {
            // VMO is pager-backed but doesn't belong to this pager.
            Err(ZxError::INVALID_ARGS)
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
        // Validate: VMO must belong to this pager.
        if !self.owns_vmo(vmo) {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate: offset and length must be page-aligned.
        if !(offset as usize).is_multiple_of(PAGE_SIZE)
            || !(length as usize).is_multiple_of(PAGE_SIZE)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate: aux_offset must be page-aligned.
        if !(aux_offset as usize).is_multiple_of(PAGE_SIZE) {
            return Err(ZxError::INVALID_ARGS);
        }
        let mut buf = alloc::vec![0u8; PAGE_SIZE];
        let mut src_off = aux_offset as usize;
        let mut dst_off = offset as usize;
        let end = offset as usize + length as usize;

        // Allow commit_page to allocate frames on this pager-backed VMO.
        vmo.set_supplying(true);

        while dst_off < end {
            let chunk = PAGE_SIZE.min(end - dst_off);
            if let Err(e) = aux_vmo.read(src_off, &mut buf[..chunk]) {
                vmo.set_supplying(false);
                let written = dst_off - offset as usize;
                if written > 0 {
                    vmo.complete_pager_requests(offset as usize, written);
                }
                return Err(e);
            }
            if let Err(e) = vmo.write(dst_off, &buf[..chunk]) {
                vmo.set_supplying(false);
                let written = dst_off - offset as usize;
                if written > 0 {
                    vmo.complete_pager_requests(offset as usize, written);
                }
                return Err(e);
            }
            crate::vm::VMO_PAGER_PAGES.add(1);
            src_off += chunk;
            dst_off += chunk;
        }
        // Done supplying — restore the pager-backed guard.
        vmo.set_supplying(false);
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
        // Validate op code first.
        if !matches!(
            op,
            ZX_PAGER_OP_FAIL
                | ZX_PAGER_OP_DIRTY
                | ZX_PAGER_OP_WRITEBACK_BEGIN
                | ZX_PAGER_OP_WRITEBACK_END
        ) {
            return Err(ZxError::NOT_SUPPORTED);
        }
        // VMO must be pager-backed.
        if !vmo.is_pager_backed() {
            return Err(ZxError::INVALID_ARGS);
        }
        // Verify the VMO belongs to this pager.
        let inner = self.inner.lock();
        if !inner
            .vmos
            .iter()
            .any(|pv| pv.vmo.upgrade().is_some_and(|v| Arc::ptr_eq(&v, vmo)))
        {
            return Err(ZxError::INVALID_ARGS);
        }
        drop(inner);
        // Validate alignment.
        if !(offset as usize).is_multiple_of(PAGE_SIZE)
            || !(length as usize).is_multiple_of(PAGE_SIZE)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate range doesn't overflow and is within VMO bounds.
        let end = (offset as usize)
            .checked_add(length as usize)
            .ok_or(ZxError::OUT_OF_RANGE)?;
        if end > vmo.len() {
            return Err(ZxError::OUT_OF_RANGE);
        }
        match op {
            ZX_PAGER_OP_FAIL => {
                // Error code must be a valid (negative) Fuchsia status code.
                // Map well-known codes to their ZxError variant; for others
                // use a generic mapping.  Fuchsia accepts any zx_status_t.
                let data_i32 = data as i32;
                if data_i32 >= 0 {
                    return Err(ZxError::INVALID_ARGS);
                }
                let err = match data_i32 {
                    -54 => ZxError::NO_SPACE,
                    -5 => ZxError::IO,
                    -29 => ZxError::IO_DATA_INTEGRITY,
                    -45 => ZxError::BAD_STATE,
                    _ => ZxError::IO, // generic fallback for unknown codes
                };
                vmo.fail_pager_requests_range(offset as usize, length as usize, err);
                Ok(())
            }
            ZX_PAGER_OP_DIRTY => {
                // data must be 0.
                if data != 0 {
                    return Err(ZxError::INVALID_ARGS);
                }
                // OP_DIRTY requires TRAP_DIRTY on the VMO.
                if !vmo.is_trap_dirty() {
                    return Err(ZxError::NOT_SUPPORTED);
                }
                // Can only dirty supplied (committed) pages.
                vmo.mark_pages_dirty(offset as usize, length as usize)
            }
            ZX_PAGER_OP_WRITEBACK_BEGIN => {
                // data must be 0 or ZX_VMO_DIRTY_RANGE_IS_ZERO (1).
                const ZX_VMO_DIRTY_RANGE_IS_ZERO: u64 = 1;
                if data != 0 && data != ZX_VMO_DIRTY_RANGE_IS_ZERO {
                    return Err(ZxError::INVALID_ARGS);
                }
                vmo.writeback_begin(offset as usize, length as usize, data);
                Ok(())
            }
            ZX_PAGER_OP_WRITEBACK_END => {
                // data must be 0.
                if data != 0 {
                    return Err(ZxError::INVALID_ARGS);
                }
                vmo.writeback_end(offset as usize, length as usize);
                Ok(())
            }
            _ => Err(ZxError::NOT_SUPPORTED),
        }
    }
}

impl Drop for Pager {
    fn drop(&mut self) {
        let inner = self.inner.lock();
        for pv in &inner.vmos {
            if let Some(vmo) = pv.vmo.upgrade() {
                vmo.fail_pager_requests(ZxError::BAD_STATE);
                vmo.clear_pager();
            }
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
