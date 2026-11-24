use std::os::unix::io::RawFd;

use nix::fcntl::{self, OFlag};
use nix::sys::mman::{self, MapFlags, ProtFlags};
use nix::{sys::stat::Mode, unistd};

use crate::{MMUFlags, PhysAddr, VirtAddr, PAGE_SIZE};

pub struct MockMemory {
    size: usize,
    fd: RawFd,
    /// Host address of the PMEM backing-store mapping (kernel-chosen).
    map_vaddr: VirtAddr,
    /// On hosts with page size > 4K (e.g., 16K on aarch64 macOS), tracks
    /// how each host page is mapped. Maps host-page-aligned vaddr to
    /// the state of that host page.
    host_pages: std::sync::Mutex<std::collections::HashMap<usize, HostPageState>>,
    /// Reverse mapping: paddr (4K-aligned) → user vaddrs, for anonymous
    /// pages only. Used to sync data between PMEM and user VA.
    /// Multiple vaddrs can map the same paddr (shared-frame aliasing).
    anon_paddr_to_vaddr: std::sync::Mutex<std::collections::HashMap<PhysAddr, Vec<usize>>>,
}

/// State of a host page in the large-page mapping path.
#[derive(Clone, Copy, PartialEq)]
enum HostPageState {
    /// Mapped via MAP_SHARED with the PMEM file at this 16K-aligned offset.
    /// All 4K guest pages within this host page share the same file region,
    /// so writes via one vaddr are visible through another (aliasing works).
    FileBacked(usize),
    /// Mapped via MAP_ANON because the guest pages within this host page
    /// map to different 16K file regions. Aliasing does NOT work.
    Anonymous,
}

/// Return the host OS page size (cached after first call).
fn host_page_size() -> usize {
    static CACHED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| unsafe { nix::libc::sysconf(nix::libc::_SC_PAGESIZE) as usize })
}

/// Check if a host page is currently mapped (accessible) using mincore.
/// Returns false if the page is unmapped (stale entries from recycled
/// address spaces). Used defensively in sync_pmem_to_user/sync_user_to_pmem
/// and mmap conflict detection (#709).
fn is_page_mapped(vaddr: usize, len: usize) -> bool {
    let mut vec = alloc::vec![0u8; (len + host_page_size() - 1) / host_page_size()];
    unsafe {
        nix::libc::mincore(
            vaddr as *mut nix::libc::c_void,
            len,
            vec.as_mut_ptr() as *mut _,
        ) == 0
    }
}

impl MockMemory {
    pub fn new(size: usize) -> Self {
        let dir = tempfile::tempdir().expect("failed to create pmem directory");
        let path = dir.path().join("zcore_libos_pmem");

        let fd = fcntl::open(
            &path,
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR,
            Mode::S_IRWXU,
        )
        .expect("failed to open");
        unistd::ftruncate(fd, size as _).expect("failed to set size of shared memory!");

        // Map the PMEM backing store at a KERNEL-CHOSEN address.
        //
        // A fixed address with MAP_FIXED would silently destroy any
        // host mapping already occupying the window: under memory
        // pressure macOS malloc grows vm_allocate'd regions into the
        // mid address space, and clobbering one leaves malloc's free
        // lists pointing into the (zero-filled) PMEM file — every
        // later malloc that walks them aborts with "memory corruption
        // of free block" (the intermittent macOS SIGTRAPs, #862).
        // mmap without MAP_FIXED picks a region disjoint from all
        // existing mappings by construction.
        let base: VirtAddr = unsafe {
            mman::mmap(
                core::ptr::null_mut(),
                size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                fd,
                0,
            )
            .expect("failed to map PMEM backing store") as usize
        };

        Self {
            size,
            fd,
            map_vaddr: base,
            host_pages: std::sync::Mutex::new(std::collections::HashMap::new()),
            anon_paddr_to_vaddr: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// The host address at which the PMEM backing store is mapped
    /// (chosen by the kernel at initialization; see `new`).
    pub fn map_vaddr(&self) -> VirtAddr {
        self.map_vaddr
    }

    /// Remove reverse-mapping entries whose vaddr falls inside
    /// [start, end) at `granularity` alignment.
    ///
    /// Two granularities are used (#862):
    /// - `PAGE_SIZE`: same-address replacement — the guest pages being
    ///   (re)mapped lose their previous entries, while sibling guest
    ///   pages sharing a larger host page keep theirs (the host
    ///   allocation may survive with only the data source changed).
    /// - host page size: the whole host page is being destroyed
    ///   (`map_anon`'s raw munmap) — every entry pointing into it must
    ///   go, or later syncs write through vaddrs the host may reuse.
    fn remove_entries_in(
        map: &mut std::collections::HashMap<PhysAddr, Vec<usize>>,
        start: usize,
        end: usize,
        granularity: usize,
    ) {
        for vaddrs in map.values_mut() {
            vaddrs.retain(|&va| {
                let base = va & !(granularity - 1);
                let vend = base + granularity;
                vend <= start || base >= end // keep if disjoint
            });
        }
        map.retain(|_, vaddrs| !vaddrs.is_empty());
    }

    /// Map `paddr` to `vaddr` in the guest address space.
    ///
    /// On hosts with 4K pages, this is a direct file-backed MAP_SHARED.
    /// On hosts with larger pages (16K on aarch64 macOS), this uses
    /// anonymous mappings + memcpy to avoid clobbering adjacent 4K
    /// guest pages that share the same host page.
    /// Map `paddr` to `vaddr` in the guest address space.
    ///
    /// Returns `true` on success, `false` if the mapping could not be
    /// created (e.g., macOS rejects `MAP_FIXED` at low virtual addresses).
    pub fn mmap(&self, vaddr: VirtAddr, len: usize, paddr: PhysAddr, prot: MMUFlags) -> bool {
        assert!(paddr.0 < self.size);
        assert!(paddr.0 + len <= self.size);

        // On aarch64 macOS, MAP_FIXED fails below ~0x200000000 (8 GiB).
        // Return false instead of panicking so the caller can propagate
        // a PagingError::NoMemory.
        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        {
            const MACOS_AARCH64_MIN_MMAP: usize = 0x2_0000_0000;
            if vaddr < MACOS_AARCH64_MIN_MMAP {
                warn!(
                    "mmap: vaddr={:#x} is below macOS aarch64 minimum ({:#x}), skipping",
                    vaddr, MACOS_AARCH64_MIN_MMAP
                );
                return false;
            }
        }

        // Refuse to map guest pages over the PMEM backing store. The
        // PMEM base is kernel-chosen (see `new`), so it could in
        // principle land where a guest VMAR wants a fixed mapping — a
        // guest MAP_FIXED here would destroy the kernel's own physical
        // memory. Return false so the caller propagates a mapping
        // failure instead of corrupting PMEM.
        if vaddr < self.map_vaddr + self.size && vaddr + len > self.map_vaddr {
            warn!(
                "mmap: guest range {:#x}..{:#x} overlaps PMEM window {:#x}..{:#x}, refusing",
                vaddr,
                vaddr + len,
                self.map_vaddr,
                self.map_vaddr + self.size
            );
            return false;
        }

        let hps = host_page_size();

        trace!(
            "mmap: vaddr={:#x}, len={:#x}, paddr={:#x}, prot={:?}, hps={:#x}",
            vaddr,
            len,
            paddr,
            prot,
            hps,
        );

        if hps <= 0x1000 {
            // Host page size is 4K (matches guest) -- direct file-backed mmap.
            // workaround on macOS to write text section.
            #[cfg(target_os = "macos")]
            let prot = if prot.contains(MMUFlags::EXECUTE) {
                prot | MMUFlags::WRITE
            } else {
                prot
            };

            let prot_noexec = mmu_flags_to_prot(prot) - ProtFlags::PROT_EXEC;
            let flags = MapFlags::MAP_SHARED | MapFlags::MAP_FIXED;
            match unsafe { mman::mmap(vaddr as _, len, prot_noexec, flags, self.fd, paddr.0 as _) }
            {
                Ok(_) => {}
                Err(err) => {
                    warn!(
                        "mmap failed: fd={}, offset={:#x}, len={:#x}, vaddr={:#x}: {:?}",
                        self.fd, paddr, len, vaddr, err
                    );
                    return false;
                }
            }
            if prot.contains(MMUFlags::EXECUTE) {
                self.mprotect(vaddr, len, prot);
            }
            return true;
        }

        // Host page size > 4K (e.g., 16K on aarch64 macOS).
        //
        // Strategy: try file-backed MAP_SHARED first. This preserves
        // shared-frame aliasing (two vaddrs mapping the same paddr
        // see the same memory). File offsets must be host-page-aligned,
        // so we use the 16K-aligned paddr region as the file offset.
        //
        // If a host page already has a file-backed mapping to a DIFFERENT
        // 16K file region (conflicting guest pages within one host page),
        // fall back to MAP_ANON + memcpy (no aliasing for that page).

        let aligned_vaddr = vaddr & !(hps - 1);
        let aligned_paddr = PhysAddr::new(paddr.0 & !(hps - 1));

        // Hold the reverse-mapping lock across the strategy. The lock
        // order (anon map -> host pages) matches `munmap`, and holding
        // it lets `map_anon` drop entries for the host page it is
        // about to destroy before the raw munmap.
        let mut anon_map = self.anon_paddr_to_vaddr.lock().unwrap();
        // Same-address replacement: the guest pages being (re)mapped
        // lose their previous entries. Sibling guest pages sharing the
        // host page keep theirs — the host allocation may survive with
        // only the mapped page's data source changing.
        Self::remove_entries_in(
            &mut anon_map,
            vaddr & !(PAGE_SIZE - 1),
            vaddr + len,
            PAGE_SIZE,
        );

        let needs_memcpy = {
            let mut pages = self.host_pages.lock().unwrap();
            let state = pages.get(&aligned_vaddr).copied();

            // File-backed MAP_SHARED requires that the vaddr offset
            // within the host page matches the paddr offset, so that
            // accessing *vaddr reads file[paddr]. This means
            // vaddr % hps == paddr % hps.
            let vaddr_offset = vaddr & (hps - 1);
            let paddr_offset = paddr.0 & (hps - 1);
            let can_file_back = vaddr_offset == paddr_offset;

            // Helper: create anonymous mapping for this host page.
            // Returns false if the mapping failed (e.g., low address on macOS).
            // Takes the reverse map: the raw munmap below destroys every
            // guest mapping in this host page, so all entries pointing
            // into it must be dropped first (#862).
            let map_anon =
                |anon_map: &mut std::collections::HashMap<PhysAddr, Vec<usize>>| -> bool {
                    Self::remove_entries_in(anon_map, aligned_vaddr, aligned_vaddr + hps, hps);
                    // On macOS, MAP_FIXED | MAP_ANON can return EACCES if the
                    // address previously had a MAP_SHARED file mapping. Clear
                    // any stale association first.
                    let _ = unsafe { mman::munmap(aligned_vaddr as _, hps) };
                    let result = unsafe {
                        mman::mmap(
                            aligned_vaddr as _,
                            hps,
                            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                            MapFlags::MAP_PRIVATE | MapFlags::MAP_FIXED | MapFlags::MAP_ANON,
                            -1,
                            0,
                        )
                    };
                    match result {
                        Ok(_) => true,
                        Err(err) => {
                            eprintln!(
                                "mmap anon failed: vaddr={:#x}, len={:#x}: {:?}",
                                aligned_vaddr, hps, err
                            );
                            false
                        }
                    }
                };

            // false for file-backed (data is in the file),
            // true for anonymous (must copy from PMEM).
            match state {
                None if can_file_back => {
                    // First mapping: try file-backed MAP_SHARED.
                    let prot_flags = ProtFlags::PROT_READ | ProtFlags::PROT_WRITE;
                    let flags = MapFlags::MAP_SHARED | MapFlags::MAP_FIXED;
                    let result = unsafe {
                        mman::mmap(
                            aligned_vaddr as _,
                            hps,
                            prot_flags,
                            flags,
                            self.fd,
                            aligned_paddr.0 as _,
                        )
                    };
                    match result {
                        Ok(_) => {
                            pages.insert(aligned_vaddr, HostPageState::FileBacked(aligned_paddr.0));
                            false // data is in the file
                        }
                        Err(_) => {
                            if !map_anon(&mut anon_map) {
                                return false;
                            }
                            pages.insert(aligned_vaddr, HostPageState::Anonymous);
                            true
                        }
                    }
                }
                None => {
                    // Offset mismatch: must use MAP_ANON.
                    if !map_anon(&mut anon_map) {
                        return false;
                    }
                    pages.insert(aligned_vaddr, HostPageState::Anonymous);
                    true
                }
                Some(HostPageState::FileBacked(existing_paddr))
                    if existing_paddr == aligned_paddr.0 && can_file_back =>
                {
                    // Same file region, compatible offsets. No-op.
                    false
                }
                Some(HostPageState::FileBacked(_)) => {
                    // Conflict: different file region or offset mismatch.
                    // Save existing data, convert to anonymous.
                    // First verify the page is still mapped — it may have
                    // been munmap'd by a previous test's cleanup (#709).
                    let page_mapped = is_page_mapped(aligned_vaddr, hps);
                    let mut saved = vec![0u8; hps];
                    if page_mapped {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                aligned_vaddr as *const u8,
                                saved.as_mut_ptr(),
                                hps,
                            );
                        }
                    }
                    if !map_anon(&mut anon_map) {
                        return false;
                    }
                    if page_mapped {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                saved.as_ptr(),
                                aligned_vaddr as *mut u8,
                                hps,
                            );
                        }
                    }
                    pages.insert(aligned_vaddr, HostPageState::Anonymous);
                    true
                }
                Some(HostPageState::Anonymous) => true,
            }
        };

        if needs_memcpy {
            // For anonymous mappings: copy data from PMEM backing store
            // and record the reverse mapping for future syncing.
            if prot.contains(MMUFlags::EXECUTE) {
                unsafe {
                    let _ = mman::mprotect(
                        aligned_vaddr as _,
                        hps,
                        ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                    );
                }
            }
            let src = (self.map_vaddr + paddr.0) as *const u8;
            let dst = vaddr as *mut u8;
            unsafe { core::ptr::copy_nonoverlapping(src, dst, len) };

            // Track paddr→vaddr for anonymous pages so we can sync later.
            // Multiple vaddrs can map the same paddr (shared-frame aliasing).
            let aligned_paddr = PhysAddr::new(paddr.0 & !(PAGE_SIZE - 1));
            let aligned_va = vaddr & !(PAGE_SIZE - 1);
            anon_map.entry(aligned_paddr).or_default().push(aligned_va);
        }
        drop(anon_map);

        // Set the requested protection on the host page(s).
        let aligned_len = ((vaddr + len + hps - 1) & !(hps - 1)) - aligned_vaddr;
        if prot.contains(MMUFlags::EXECUTE) {
            // On aarch64 macOS, W^X is enforced -- use RX only.
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                unsafe {
                    mman::mprotect(
                        aligned_vaddr as _,
                        aligned_len,
                        ProtFlags::PROT_READ | ProtFlags::PROT_EXEC,
                    )
                }
                .unwrap_or_else(|err| {
                    panic!(
                        "mprotect RX failed: vaddr={:#x}, len={:#x}: {:?}",
                        aligned_vaddr, aligned_len, err
                    )
                });
            }
            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            {
                let p = mmu_flags_to_prot(prot);
                unsafe { mman::mprotect(aligned_vaddr as _, aligned_len, p) }.unwrap_or_else(
                    |err| {
                        panic!(
                            "mprotect failed: vaddr={:#x}, len={:#x}: {:?}",
                            aligned_vaddr, aligned_len, err
                        )
                    },
                );
            }
        }
        // For non-executable pages: apply the requested protection.
        // COW pages are mapped READ-only by VmMapping::map() — the
        // mprotect ensures a write fault (SIGBUS on macOS) is generated
        // so the COW handler can copy the page and remap with WRITE.
        if !prot.contains(MMUFlags::EXECUTE) {
            let p = mmu_flags_to_prot(prot) - ProtFlags::PROT_EXEC;
            if let Err(err) = unsafe { mman::mprotect(aligned_vaddr as _, aligned_len, p) } {
                warn!(
                    "mprotect failed: vaddr={:#x}, len={:#x}, prot={:?}: {:?}",
                    aligned_vaddr, aligned_len, prot, err
                );
            }
        }
        true
    }

    /// Sync data from PMEM to user VA for anonymous-mapped pages.
    ///
    /// Called after `pmem_write(paddr, buf)` to ensure anonymous user
    /// VA pages see the updated data. No-op for file-backed pages
    /// (which are coherent via MAP_SHARED).
    pub fn sync_pmem_to_user(&self, paddr: PhysAddr, len: usize) {
        let map = self.anon_paddr_to_vaddr.lock().unwrap();
        let page_size = PAGE_SIZE;
        let hps = host_page_size();
        let start_page = PhysAddr::new(paddr.0 & !(page_size - 1));
        let end = paddr.0 + len;
        let mut page = start_page.0;
        while page < end {
            if let Some(vaddrs) = map.get(&PhysAddr::new(page)) {
                let offset_in_page = paddr.0.saturating_sub(page);
                let copy_start = page + offset_in_page;
                let copy_end = (page + page_size).min(end);
                let copy_len = copy_end - copy_start;
                if copy_len > 0 {
                    let src = (self.map_vaddr + copy_start) as *const u8;
                    for &vaddr in vaddrs {
                        // Probe the target address for writeability before
                        // copying. Stale entries can persist when frames are
                        // recycled across tests (#709). Use mincore to check
                        // if the page is mapped.
                        let host_page = vaddr & !(hps - 1);
                        if !is_page_mapped(host_page, hps) {
                            continue;
                        }
                        let dst = (vaddr + offset_in_page) as *mut u8;
                        unsafe { core::ptr::copy_nonoverlapping(src, dst, copy_len) };
                    }
                }
            }
            page += page_size;
        }
    }

    /// Sync data from user VA to PMEM for anonymous-mapped pages.
    ///
    /// Called before `pmem_read(paddr, buf)` to ensure PMEM has the
    /// latest data written through anonymous user VAs. Uses the first
    /// mapped vaddr as the authoritative source.
    pub fn sync_user_to_pmem(&self, paddr: PhysAddr, len: usize) {
        let map = self.anon_paddr_to_vaddr.lock().unwrap();
        let hps = host_page_size();
        let page_size = PAGE_SIZE;
        let start_page = PhysAddr::new(paddr.0 & !(page_size - 1));
        let end = paddr.0 + len;
        let mut page = start_page.0;
        while page < end {
            if let Some(vaddrs) = map.get(&PhysAddr::new(page)) {
                if let Some(&vaddr) = vaddrs.first() {
                    // Verify the user VA is still mapped (#709).
                    let host_page = vaddr & !(hps - 1);
                    if !is_page_mapped(host_page, hps) {
                        page += page_size;
                        continue;
                    }
                    let offset_in_page = paddr.0.saturating_sub(page);
                    let copy_start = page + offset_in_page;
                    let copy_end = (page + page_size).min(end);
                    let copy_len = copy_end - copy_start;
                    if copy_len > 0 {
                        let src = (vaddr + offset_in_page) as *const u8;
                        let dst = (self.map_vaddr + copy_start) as *mut u8;
                        unsafe { core::ptr::copy_nonoverlapping(src, dst, copy_len) };
                    }
                }
            }
            page += page_size;
        }
    }

    pub fn munmap(&self, vaddr: VirtAddr, len: usize) {
        let hps = host_page_size();
        let aligned_vaddr = vaddr & !(hps - 1);
        let aligned_len = ((vaddr + len + hps - 1) & !(hps - 1)) - aligned_vaddr;

        // Acquire the reverse mapping lock BEFORE unmapping, so that
        // concurrent sync operations cannot read through an address
        // that we're about to unmap (Comment 3 fix).
        let mut anon_map = self.anon_paddr_to_vaddr.lock().unwrap();

        // Remove reverse mapping entries for anonymous pages in the
        // unmapped range. Each paddr maps to a Vec of vaddrs.
        for vaddrs in anon_map.values_mut() {
            vaddrs.retain(|&va| {
                let va_aligned = va & !(hps - 1);
                va_aligned < aligned_vaddr || va_aligned >= aligned_vaddr + aligned_len
            });
        }
        anon_map.retain(|_, vaddrs| !vaddrs.is_empty());

        // Now safe to unmap — no sync can read through these addresses.
        // POSIX munmap on an already-unmapped range is a no-op (not an error).
        if let Err(err) = unsafe { mman::munmap(aligned_vaddr as _, aligned_len) } {
            warn!(
                "munmap non-fatal: vaddr={:#x}, len={:#x}: {:?}",
                aligned_vaddr, aligned_len, err
            );
            return;
        }

        // Remove host page tracking entries for the unmapped pages.
        let mut pages = self.host_pages.lock().unwrap();
        let mut page = aligned_vaddr;
        while page < aligned_vaddr + aligned_len {
            pages.remove(&page);
            page += hps;
        }
    }

    pub fn mprotect(&self, vaddr: VirtAddr, len: usize, prot: MMUFlags) {
        let hps = host_page_size();
        let aligned_vaddr = vaddr & !(hps - 1);
        let aligned_len = ((vaddr + len + hps - 1) & !(hps - 1)) - aligned_vaddr;

        // On aarch64 macOS, W^X: reject simultaneous W+X.
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let prot = if prot.contains(MMUFlags::WRITE) && prot.contains(MMUFlags::EXECUTE) {
            warn!("mprotect: dropping WRITE from RWX on aarch64 macOS");
            (prot | MMUFlags::READ) - MMUFlags::WRITE
        } else {
            prot
        };

        unsafe { mman::mprotect(aligned_vaddr as _, aligned_len, mmu_flags_to_prot(prot)) }
            .unwrap_or_else(|err| {
                panic!(
                    "mprotect failed: vaddr={:#x}, len={:#x}, prot={:?}: {:?}",
                    aligned_vaddr, aligned_len, prot, err
                )
            });
    }

    /// Clear all user-space mapping state (host_pages + anon tracking).
    /// Called between tests to prevent stale state from leaking across
    /// test boundaries.  The PMEM backing file and its mapping at
    /// PMEM_MAP_VADDR are preserved — only user-space mappings are cleared.
    /// Remove all user VA tracking entries for a physical address.
    /// Called when a physical frame is deallocated to prevent
    /// `sync_pmem_to_user` from writing to stale user VAs after
    /// the frame is recycled (#709).
    pub fn clear_paddr_mappings(&self, paddr: PhysAddr) {
        let aligned = PhysAddr::new(paddr.0 & !(PAGE_SIZE - 1));
        self.anon_paddr_to_vaddr.lock().unwrap().remove(&aligned);
    }

    pub fn phys_to_virt(&self, paddr: PhysAddr) -> VirtAddr {
        assert!(paddr.0 < self.size);
        self.map_vaddr + paddr.0
    }

    pub fn as_ptr<T>(&self, paddr: PhysAddr) -> *const T {
        self.phys_to_virt(paddr) as _
    }

    pub fn as_mut_ptr<T>(&self, paddr: PhysAddr) -> *mut T {
        self.phys_to_virt(paddr) as _
    }
}

impl Drop for MockMemory {
    fn drop(&mut self) {
        trace!("Drop MockMemory: fd={:?}", self.fd);
        unistd::close(self.fd).expect("failed to close shared memory file!");
    }
}

fn mmu_flags_to_prot(f: MMUFlags) -> ProtFlags {
    let mut flags = ProtFlags::empty();
    if f.contains(MMUFlags::READ) {
        flags |= ProtFlags::PROT_READ;
    }
    if f.contains(MMUFlags::WRITE) {
        flags |= ProtFlags::PROT_WRITE;
    }
    if f.contains(MMUFlags::EXECUTE) {
        flags |= ProtFlags::PROT_EXEC;
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imp::mem::MOCK_PHYS_MEM;

    /// #862 regression: replacing a mapping at a vaddr (`MAP_FIXED`
    /// clobbers silently) must invalidate the previous paddr's
    /// reverse-mapping entry. A stale entry later directs
    /// `sync_pmem_to_user` to write through a vaddr the host may have
    /// reused for its own allocations — macOS heap corruption
    /// ("memory corruption of free block").
    #[test]
    fn remap_invalidates_stale_entries() {
        // Reverse-mapping entries only exist on hosts whose page size
        // exceeds the guest 4K (the anonymous-mapping strategy);
        // on 4K hosts mappings are direct file-backed.
        if host_page_size() <= PAGE_SIZE {
            return;
        }
        let mem = &*MOCK_PHYS_MEM;
        // Above the macOS aarch64 mmap minimum (8 GiB), the PMEM
        // window (32–33 GiB) and the aspace region (48 GiB), and
        // unique to this test.
        let vaddr: VirtAddr = 0x10_0000_0000;
        let paddr_a: PhysAddr = PhysAddr::new(0x1000);
        let paddr_b: PhysAddr = PhysAddr::new(0x3000);
        let aligned = |p: usize| p & !(PAGE_SIZE - 1);

        // vaddr is 16K-aligned, paddr is not → offset mismatch forces
        // the anonymous path, which records the reverse mapping.
        assert!(mem.mmap(vaddr, PAGE_SIZE, paddr_a, MMUFlags::READ | MMUFlags::WRITE));
        {
            let map = mem.anon_paddr_to_vaddr.lock().unwrap();
            let entry = map
                .get(&PhysAddr::new(aligned(paddr_a.0)))
                .expect("entry for paddr_a");
            assert!(
                entry.contains(&aligned(vaddr)),
                "paddr_a should track vaddr after anon map"
            );
        }

        // Replace the mapping at the same vaddr with a different paddr.
        assert!(mem.mmap(vaddr, PAGE_SIZE, paddr_b, MMUFlags::READ | MMUFlags::WRITE));
        {
            let map = mem.anon_paddr_to_vaddr.lock().unwrap();
            let stale = map
                .get(&PhysAddr::new(aligned(paddr_a.0)))
                .is_some_and(|v| v.contains(&aligned(vaddr)));
            assert!(
                !stale,
                "stale reverse-mapping entry survived remap — later syncs would write through the reused vaddr (#862)"
            );
        }

        mem.munmap(vaddr, PAGE_SIZE);
    }

    /// #862 regression (CodeRabbit review): sibling guest pages sharing
    /// a host page must keep independent, live reverse mappings — the
    /// same-address invalidation is guest-page-granular, so mapping a
    /// second guest page in the same host page must not drop the first
    /// page's entry. Verifies synchronization in both directions.
    #[test]
    fn sibling_pages_sync_independently() {
        if host_page_size() <= PAGE_SIZE {
            return;
        }
        let mem = &*MOCK_PHYS_MEM;
        // 16K-aligned base, distinct from other tests' ranges.
        let base: VirtAddr = 0x11_0000_0000;
        let pa: PhysAddr = PhysAddr::new(0x11000); // offset 0x1000 in its host page → anon path
        let pb: PhysAddr = PhysAddr::new(0x13000); // different paddr for the sibling page
        let flags = MMUFlags::READ | MMUFlags::WRITE;

        assert!(mem.mmap(base, PAGE_SIZE, pa, flags));
        assert!(mem.mmap(base + PAGE_SIZE, PAGE_SIZE, pb, flags));

        // PMEM → user: writes through pmem_write must reach BOTH user
        // pages (each entry must have survived the sibling mapping).
        crate::hal_fn::mem::pmem_write(pa, &[0xAA]);
        crate::hal_fn::mem::pmem_write(pb, &[0xBB]);
        assert_eq!(
            unsafe { core::ptr::read_volatile(base as *const u8) },
            0xAA,
            "first sibling page must sync from its paddr"
        );
        assert_eq!(
            unsafe { core::ptr::read_volatile((base + PAGE_SIZE) as *const u8) },
            0xBB,
            "second sibling page must sync from its paddr"
        );

        // User → PMEM: a write through each user page must be visible
        // via pmem_read of its paddr.
        unsafe { core::ptr::write_volatile((base + PAGE_SIZE) as *mut u8, 0xCC) };
        let mut buf = [0u8; 1];
        crate::hal_fn::mem::pmem_read(pb, &mut buf);
        assert_eq!(buf[0], 0xCC, "user write must sync back to its paddr");

        unsafe { core::ptr::write_volatile(base as *mut u8, 0xDD) };
        crate::hal_fn::mem::pmem_read(pa, &mut buf);
        assert_eq!(buf[0], 0xDD, "first sibling's user write must sync back");

        mem.munmap(base, 2 * PAGE_SIZE);
    }
}
