use std::os::unix::io::RawFd;

use nix::fcntl::{self, OFlag};
use nix::sys::mman::{self, MapFlags, ProtFlags};
use nix::{sys::stat::Mode, unistd};

use super::mem::PMEM_MAP_VADDR;
use crate::{MMUFlags, PhysAddr, VirtAddr};

pub struct MockMemory {
    size: usize,
    fd: RawFd,
    /// On hosts with page size > 4K (e.g., 16K on aarch64 macOS), we use
    /// anonymous mappings + memcpy instead of file-backed MAP_FIXED to
    /// prevent host-page clobbering when multiple 4K guest pages share
    /// one host page. This set tracks which host-page-aligned vaddrs
    /// already have an anonymous mapping, so we don't re-map (zero out)
    /// pages that were already populated.
    anon_mapped: std::sync::Mutex<std::collections::HashSet<usize>>,
}

/// Return the host OS page size (cached after first call).
fn host_page_size() -> usize {
    static CACHED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| unsafe { nix::libc::sysconf(nix::libc::_SC_PAGESIZE) as usize })
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
        .expect("faild to open");
        unistd::ftruncate(fd, size as _).expect("failed to set size of shared memory!");

        let mem = Self {
            size,
            fd,
            anon_mapped: std::sync::Mutex::new(std::collections::HashSet::new()),
        };
        // Map the entire physical memory backing store at PMEM_MAP_VADDR.
        // This is always MAP_SHARED so VMO read/write via phys_to_virt works.
        mem.mmap_file(PMEM_MAP_VADDR, size, 0, MMUFlags::READ | MMUFlags::WRITE);
        mem
    }

    /// Low-level file-backed mmap. Used for the PMEM backing store
    /// (where vaddr, len, and offset are always host-page-aligned).
    fn mmap_file(&self, vaddr: VirtAddr, len: usize, offset: usize, prot: MMUFlags) {
        let prot_flags = ProtFlags::from(prot) - ProtFlags::PROT_EXEC;
        let flags = MapFlags::MAP_SHARED | MapFlags::MAP_FIXED;
        unsafe {
            mman::mmap(vaddr as _, len, prot_flags, flags, self.fd, offset as _).unwrap_or_else(
                |err| {
                    panic!(
                        "mmap_file failed: fd={}, offset={:#x}, len={:#x}, vaddr={:#x}: {:?}",
                        self.fd, offset, len, vaddr, err
                    )
                },
            );
        }
    }

    /// Map `paddr` to `vaddr` in the guest address space.
    ///
    /// On hosts with 4K pages, this is a direct file-backed MAP_SHARED.
    /// On hosts with larger pages (16K on aarch64 macOS), this uses
    /// anonymous mappings + memcpy to avoid clobbering adjacent 4K
    /// guest pages that share the same host page.
    pub fn mmap(&self, vaddr: VirtAddr, len: usize, paddr: PhysAddr, prot: MMUFlags) {
        assert!(paddr < self.size);
        assert!(paddr + len <= self.size);

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

            let prot_noexec = ProtFlags::from(prot) - ProtFlags::PROT_EXEC;
            let flags = MapFlags::MAP_SHARED | MapFlags::MAP_FIXED;
            unsafe { mman::mmap(vaddr as _, len, prot_noexec, flags, self.fd, paddr as _) }
                .unwrap_or_else(|err| {
                    panic!(
                        "mmap failed: fd={}, offset={:#x}, len={:#x}, vaddr={:#x}: {:?}",
                        self.fd, paddr, len, vaddr, err
                    )
                });
            if prot.contains(MMUFlags::EXECUTE) {
                self.mprotect(vaddr, len, prot);
            }
            return;
        }

        // Host page size > 4K (e.g., 16K on aarch64 macOS).
        //
        // We cannot use file-backed MAP_FIXED because multiple 4K guest
        // pages within one host page would clobber each other (each
        // MAP_FIXED replaces the entire host page with a different file
        // region).
        //
        // Solution: use MAP_ANON for the host page, then memcpy data
        // from the PMEM backing store. Track which host pages are
        // already mapped to avoid re-mapping (which zeros them out).

        let aligned_vaddr = vaddr & !(hps - 1);
        let aligned_len = ((vaddr + len + hps - 1) & !(hps - 1)) - aligned_vaddr;

        {
            let mut mapped = self.anon_mapped.lock().unwrap();
            // Map any host pages in [aligned_vaddr, aligned_vaddr+aligned_len)
            // that we haven't seen yet.
            let mut page = aligned_vaddr;
            while page < aligned_vaddr + aligned_len {
                if !mapped.contains(&page) {
                    unsafe {
                        mman::mmap(
                            page as _,
                            hps,
                            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                            MapFlags::MAP_PRIVATE | MapFlags::MAP_FIXED | MapFlags::MAP_ANON,
                            -1,
                            0,
                        )
                    }
                    .unwrap_or_else(|err| {
                        panic!(
                            "mmap anon failed: vaddr={:#x}, len={:#x}: {:?}",
                            page, hps, err
                        )
                    });
                    mapped.insert(page);
                }
                page += hps;
            }
        }

        // Ensure pages are writable for the memcpy (may have been
        // mprotected to RX previously for executable pages).
        if prot.contains(MMUFlags::EXECUTE) {
            unsafe {
                let _ = mman::mprotect(
                    aligned_vaddr as _,
                    aligned_len,
                    ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                );
            }
        }

        // Copy data from PMEM backing store into the anonymous mapping.
        let src = (PMEM_MAP_VADDR + paddr) as *const u8;
        let dst = vaddr as *mut u8;
        unsafe { core::ptr::copy_nonoverlapping(src, dst, len) };

        // Set the requested protection on the host page(s).
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
                let p = ProtFlags::from(prot);
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
        // Non-executable pages stay RW (the anonymous mapping default).
        // The data is synced to PMEM via write() below, not via mmap
        // sharing, since these are MAP_PRIVATE pages.
    }

    /// Synchronize a guest page back to the PMEM backing store.
    /// On 16K hosts with MAP_ANON pages, writes to user pages are
    /// private and must be copied back to PMEM for VMO reads to work.
    pub fn sync_to_pmem(&self, vaddr: VirtAddr, len: usize, paddr: PhysAddr) {
        if host_page_size() <= 0x1000 {
            return; // MAP_SHARED -- already synced.
        }
        let src = vaddr as *const u8;
        let dst = (PMEM_MAP_VADDR + paddr) as *mut u8;
        unsafe { core::ptr::copy_nonoverlapping(src, dst, len) };
    }

    pub fn munmap(&self, vaddr: VirtAddr, len: usize) {
        let hps = host_page_size();
        let aligned_vaddr = vaddr & !(hps - 1);
        let aligned_len = ((vaddr + len + hps - 1) & !(hps - 1)) - aligned_vaddr;
        unsafe { mman::munmap(aligned_vaddr as _, aligned_len) }
            .unwrap_or_else(|err| panic!("munmap failed: vaddr={:#x}: {:?}", aligned_vaddr, err));
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

        unsafe { mman::mprotect(aligned_vaddr as _, aligned_len, prot.into()) }.unwrap_or_else(
            |err| {
                panic!(
                    "mprotect failed: vaddr={:#x}, len={:#x}, prot={:?}: {:?}",
                    aligned_vaddr, aligned_len, prot, err
                )
            },
        );
    }

    pub fn phys_to_virt(&self, paddr: PhysAddr) -> VirtAddr {
        assert!(paddr < self.size);
        PMEM_MAP_VADDR + paddr
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

impl From<MMUFlags> for ProtFlags {
    fn from(f: MMUFlags) -> Self {
        let mut flags = Self::empty();
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
}
