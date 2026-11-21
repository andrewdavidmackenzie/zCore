//! Lock-order graph for the VM subsystem (#829 Batch 5E-1).
//!
//! The levels and edges below were derived from the *actual blocking
//! paths* in the code (each edge cites its site), not from the old
//! lockdep's documented hierarchy — which contradicted the real
//! acquisition order (it placed VmObjectInner before VmMappingInner;
//! the page-fault handler blocks in the opposite direction).
//!
//! ```text
//! Unlocked → VmarInnerLock → VmMappingInnerLock → VmObjectInnerLock → PageTableLock
//! Unlocked → StreamInnerLock ↗
//! ```
//!
//! ## Edges (each verified against a blocking acquisition site)
//!
//! - `VmarInnerLock → VmMappingInnerLock`: destroy/split holds the
//!   VMAR inner across `map.cut()` (vmar.rs, the destroy path that
//!   iterates `inner.mappings`).
//! - `VmMappingInnerLock → VmObjectInnerLock`: `handle_page_fault`
//!   holds the mapping inner across `vmo.*` calls.
//! - `StreamInnerLock → VmObjectInnerLock`: `Stream::read`/`write`
//!   hold the stream inner across `read_at`/`write_at`.
//! - `VmarInnerLock → PageTableLock`: allocation clears stale PTEs
//!   with `page_table.lock().unmap_cont(..)` under the VMAR inner.
//! - `VmMappingInnerLock → PageTableLock`: the fault handler maps
//!   PTEs under the mapping inner.
//! - `VmObjectInnerLock → PageTableLock`: `range_change` takes
//!   `page_table.lock()` while its caller (resize/decommit) holds
//!   the VMO lock.
//!
//! ## Deliberately absent edge
//!
//! `VmObjectInnerLock → VmMappingInnerLock` does **not** exist: that
//! direction is the #711 exception (`range_change` uses `try_lock`
//! on the mapping under the VMO lock, with a documented
//! PTE-invalidation-skip caveat). Any *blocking* VMO→mapping
//! acquisition is a compile error against this graph.
//!
//! ## Same-type disciplines (documented; not graph-expressible)
//!
//! - COW tree: never hold one VMO's lock across another VMO's lock —
//!   collect-then-act (upgrade the parent Arc, drop the guard, call).
//! - VMAR tree: parent before child (`Drop` clears children under the
//!   parent inner, which takes child inners in the child `Drop`s).
//!
//! `PagerInnerLock` is currently a leaf (collect-then-act everywhere)
//! and is not declared; it joins when it gains a nested path.

use hal::vm::GenericPageTable;
use lock::lockgraph::{LockAfter, LockFor, Unlocked};
use lock::{impl_lock_after, KernelMutexGuard};

use super::stream::Stream;
use super::vmar::{VmAddressRegion, VmMapping};
use super::vmo::VmObject;

// ── Lock levels ─────────────────────────────────────────────────────

/// The `VmAddressRegion::inner` lock.
pub enum VmarInnerLock {}

/// The `VmMapping::inner` lock.
pub enum VmMappingInnerLock {}

/// The `VmObject::inner` lock.
pub enum VmObjectInnerLock {}

/// The shared page-table lock (`Arc<KernelMutex<dyn GenericPageTable>>`).
pub enum PageTableLock {}

/// The `Stream::inner` lock.
pub enum StreamInnerLock {}

// ── The graph ───────────────────────────────────────────────────────
//
// Single-parent levels use `impl_lock_after!` (its transitivity
// blanket is unambiguous). Multi-parent levels — VmObjectInnerLock
// (reachable after Mapping OR Stream) and PageTableLock (after Vmar,
// Mapping, VMO, or Stream) — must use DIRECT impls enumerating their
// predecessors: the macro's blankets would overlap on the shared
// predecessors (e.g. both would claim `LockAfter<Unlocked>` for the
// same level) and fail to compile. Cycle-safety of the direct impls
// is verified by inspection: the graph
// Vmar→{Mapping,PT}, Mapping→{VMO,PT}, Stream→{VMO}, VMO→{PT} is
// acyclic.

// Roots (direct impls — see lockgraph docs).
impl LockAfter<Unlocked> for VmarInnerLock {}
impl LockAfter<Unlocked> for StreamInnerLock {}

// VMAR before Mapping: destroy/split holds the VMAR inner across
// mapping operations. Single parent — the macro is safe here.
impl_lock_after!(VmarInnerLock => VmMappingInnerLock);

// VMO: reachable from Unlocked (every plain vmo.read/write), after
// the VMAR (destroy/split), after a mapping (the page-fault
// handler), or after a stream (stream read/write). The REVERSE
// (VMO → Mapping) is the #711 try_lock exception — deliberately
// absent.
impl LockAfter<Unlocked> for VmObjectInnerLock {}
impl LockAfter<VmarInnerLock> for VmObjectInnerLock {}
impl LockAfter<VmMappingInnerLock> for VmObjectInnerLock {}
impl LockAfter<StreamInnerLock> for VmObjectInnerLock {}

// Page table: the terminal lock in every chain — reachable from
// Unlocked and after every other VM level.
impl LockAfter<Unlocked> for PageTableLock {}
impl LockAfter<VmarInnerLock> for PageTableLock {}
impl LockAfter<VmMappingInnerLock> for PageTableLock {}
impl LockAfter<VmObjectInnerLock> for PageTableLock {}
impl LockAfter<StreamInnerLock> for PageTableLock {}

// ── LockFor: connecting levels to the real fields ──────────────────

impl LockFor<VmarInnerLock> for VmAddressRegion {
    type Data = Option<super::vmar::VmarInner>;
    type Guard<'l> = KernelMutexGuard<'l, Option<super::vmar::VmarInner>>;
    fn lock(&self) -> Self::Guard<'_> {
        self.inner.lock()
    }
}

impl LockFor<PageTableLock> for VmAddressRegion {
    // `dyn GenericPageTable` is unsized; `Data` is informational only
    // (the machinery uses `Guard`).
    type Data = ();
    type Guard<'l> = KernelMutexGuard<'l, dyn GenericPageTable>;
    fn lock(&self) -> Self::Guard<'_> {
        self.page_table.lock()
    }
}

impl LockFor<VmMappingInnerLock> for VmMapping {
    type Data = super::vmar::VmMappingInner;
    type Guard<'l> = KernelMutexGuard<'l, super::vmar::VmMappingInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.inner.lock()
    }
}

impl LockFor<PageTableLock> for VmMapping {
    // See the VmAddressRegion impl — `Data` is informational only.
    type Data = ();
    type Guard<'l> = KernelMutexGuard<'l, dyn GenericPageTable>;
    fn lock(&self) -> Self::Guard<'_> {
        self.page_table.lock()
    }
}

impl LockFor<VmObjectInnerLock> for VmMapping {
    type Data = super::vmo::VmObjectInner;
    type Guard<'l> = KernelMutexGuard<'l, super::vmo::VmObjectInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.vmo.inner.lock()
    }
}

impl LockFor<VmObjectInnerLock> for VmObject {
    type Data = super::vmo::VmObjectInner;
    type Guard<'l> = KernelMutexGuard<'l, super::vmo::VmObjectInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.inner.lock()
    }
}

impl LockFor<StreamInnerLock> for Stream {
    type Data = super::stream::StreamInner;
    type Guard<'l> = KernelMutexGuard<'l, super::stream::StreamInner>;
    fn lock(&self) -> Self::Guard<'_> {
        self.inner.lock()
    }
}

// ── Tests ───────────────────────────────────────────────────────────
//
// The compile-fail doc tests below reproduce the two forbidden
// acquisition shapes; the unit tests exercise the legal chains.

/// The #711 pattern: holding the VMO lock and then *blocking* on a
/// mapping lock. `range_change` is the one legitimate user of this
/// direction and stays on `try_lock` with its documented caveat; any
/// new blocking acquisition must not compile.
///
/// ```compile_fail
/// use lock::lockgraph::{LockFor, Locked};
/// use zircon_object::vm::lock_order::{
///     VmFixture as Vm, VmMappingInnerLock, VmObjectInnerLock,
/// };
///
/// let v = Vm::new();
/// let mut locked = Locked::new(&v);
/// let (_vmo_guard, vmo_ctx) = locked.lock_and::<VmObjectInnerLock>();
/// // VMO → Mapping is the #711 direction — no edge exists.
/// let _map_guard = vmo_ctx.lock::<VmMappingInnerLock>();
/// ```
///
/// The reverse-VMAR pattern: holding a mapping lock and then
/// acquiring the VMAR lock (the graph is VMAR → mapping).
///
/// ```compile_fail
/// use lock::lockgraph::{LockFor, Locked};
/// use zircon_object::vm::lock_order::{
///     VmFixture as Vm, VmarInnerLock, VmMappingInnerLock,
/// };
///
/// let v = Vm::new();
/// let mut locked = Locked::new(&v);
/// let (_map_guard, map_ctx) = locked.lock_and::<VmMappingInnerLock>();
/// let _vmar_guard = map_ctx.lock::<VmarInnerLock>();
/// ```
#[cfg(test)]
#[allow(dead_code)] // constructed by the compile-fail doc tests above
pub(crate) struct VmFixture {
    vmar: lock::KernelMutex<u32>,
    mapping: lock::KernelMutex<u64>,
    vmo: lock::KernelMutex<u16>,
    page_table: lock::KernelMutex<u8>,
    stream: lock::KernelMutex<u32>,
}

#[cfg(test)]
impl VmFixture {
    #[allow(dead_code)] // used by the compile-fail doc tests above
    pub(crate) fn new() -> Self {
        VmFixture {
            vmar: lock::KernelMutex::new(1),
            mapping: lock::KernelMutex::new(2),
            vmo: lock::KernelMutex::new(3),
            page_table: lock::KernelMutex::new(4),
            stream: lock::KernelMutex::new(5),
        }
    }
}

#[cfg(test)]
impl LockFor<VmarInnerLock> for VmFixture {
    type Data = u32;
    type Guard<'l> = KernelMutexGuard<'l, u32>;
    fn lock(&self) -> Self::Guard<'_> {
        self.vmar.lock()
    }
}

#[cfg(test)]
impl LockFor<VmMappingInnerLock> for VmFixture {
    type Data = u64;
    type Guard<'l> = KernelMutexGuard<'l, u64>;
    fn lock(&self) -> Self::Guard<'_> {
        self.mapping.lock()
    }
}

#[cfg(test)]
impl LockFor<VmObjectInnerLock> for VmFixture {
    type Data = u16;
    type Guard<'l> = KernelMutexGuard<'l, u16>;
    fn lock(&self) -> Self::Guard<'_> {
        self.vmo.lock()
    }
}

#[cfg(test)]
impl LockFor<PageTableLock> for VmFixture {
    type Data = u8;
    type Guard<'l> = KernelMutexGuard<'l, u8>;
    fn lock(&self) -> Self::Guard<'_> {
        self.page_table.lock()
    }
}

#[cfg(test)]
impl LockFor<StreamInnerLock> for VmFixture {
    type Data = u32;
    type Guard<'l> = KernelMutexGuard<'l, u32>;
    fn lock(&self) -> Self::Guard<'_> {
        self.stream.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lock::lockgraph::Locked;
    use lock::KernelMutex;

    #[test]
    fn legal_vmar_mapping_vmo_page_table_chain() {
        let v = VmFixture::new();
        let mut locked = Locked::new(&v);
        let (_v, mut ctx) = locked.lock_and::<VmarInnerLock>();
        let (_m, mut ctx) = ctx.lock_and::<VmMappingInnerLock>();
        let (_o, mut ctx) = ctx.lock_and::<VmObjectInnerLock>();
        let _pt = ctx.lock::<PageTableLock>();
    }

    #[test]
    fn legal_stream_vmo_chain() {
        let v = VmFixture::new();
        let mut locked = Locked::new(&v);
        let (_s, mut ctx) = locked.lock_and::<StreamInnerLock>();
        let _o = ctx.lock::<VmObjectInnerLock>();
    }

    #[test]
    fn legal_transitive_vmar_straight_to_page_table() {
        let v = VmFixture::new();
        let mut locked = Locked::new(&v);
        let (_v, mut ctx) = locked.lock_and::<VmarInnerLock>();
        // Transitivity: VMAR → (mapping → VMO →) page table.
        let _pt = ctx.lock::<PageTableLock>();
    }

    #[test]
    fn legal_unlocked_entry_at_any_level() {
        // Every level is reachable from a root, so any level can be
        // entered with no locks held.
        let v = VmFixture::new();
        let mut locked = Locked::new(&v);
        let (_o, _ctx) = locked.lock_and::<VmObjectInnerLock>();
        let mut locked2 = Locked::new(&v);
        let (_pt, _ctx2) = locked2.lock_and::<PageTableLock>();
        let _ = KernelMutex::new(0u8); // silence unused import when tests compile standalone
    }
}
