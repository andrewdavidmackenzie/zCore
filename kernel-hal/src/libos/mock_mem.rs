use std::os::unix::io::RawFd;

use nix::fcntl::{self, OFlag};
use nix::sys::mman::{self, MapFlags, ProtFlags};
use nix::{sys::stat::Mode, unistd};

use super::mem::PMEM_MAP_VADDR;
use crate::{MMUFlags, PhysAddr, VirtAddr};

pub struct MockMemory {
    size: usize,
    fd: RawFd,
    /// On hosts with page size > 4K (e.g., 16K on aarch64 macOS), tracks
    /// how each host page is mapped. Maps host-page-aligned vaddr to
    /// the state of that host page.
    host_pages: std::sync::Mutex<std::collections::HashMap<usize, HostPageState>>,
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
            host_pages: std::sync::Mutex::new(std::collections::HashMap::new()),
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
        // Strategy: try file-backed MAP_SHARED first. This preserves
        // shared-frame aliasing (two vaddrs mapping the same paddr
        // see the same memory). File offsets must be host-page-aligned,
        // so we use the 16K-aligned paddr region as the file offset.
        //
        // If a host page already has a file-backed mapping to a DIFFERENT
        // 16K file region (conflicting guest pages within one host page),
        // fall back to MAP_ANON + memcpy (no aliasing for that page).

        let aligned_vaddr = vaddr & !(hps - 1);
        let aligned_paddr = paddr & !(hps - 1);

        let needs_memcpy = {
            let mut pages = self.host_pages.lock().unwrap();
            let state = pages.get(&aligned_vaddr).copied();

            // File-backed MAP_SHARED requires that the vaddr offset
            // within the host page matches the paddr offset, so that
            // accessing *vaddr reads file[paddr]. This means
            // vaddr % hps == paddr % hps.
            let vaddr_offset = vaddr & (hps - 1);
            let paddr_offset = paddr & (hps - 1);
            let can_file_back = vaddr_offset == paddr_offset;

            // Helper: create anonymous mapping for this host page.
            let map_anon = || {
                unsafe {
                    mman::mmap(
                        aligned_vaddr as _,
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
                        aligned_vaddr, hps, err
                    )
                });
            };

            // needs_memcpy: false for file-backed (data is in the file),
            // true for anonymous (must copy from PMEM).
            let needs_memcpy = match state {
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
                            aligned_paddr as _,
                        )
                    };
                    match result {
                        Ok(_) => {
                            pages.insert(aligned_vaddr, HostPageState::FileBacked(aligned_paddr));
                            false // data is in the file
                        }
                        Err(_) => {
                            map_anon();
                            pages.insert(aligned_vaddr, HostPageState::Anonymous);
                            true
                        }
                    }
                }
                None => {
                    // Offset mismatch: must use MAP_ANON.
                    map_anon();
                    pages.insert(aligned_vaddr, HostPageState::Anonymous);
                    true
                }
                Some(HostPageState::FileBacked(existing_paddr))
                    if existing_paddr == aligned_paddr && can_file_back =>
                {
                    // Same file region, compatible offsets. No-op.
                    false
                }
                Some(HostPageState::FileBacked(_)) => {
                    // Conflict: different file region or offset mismatch.
                    // Save existing data, convert to anonymous.
                    let mut saved = vec![0u8; hps];
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            aligned_vaddr as *const u8,
                            saved.as_mut_ptr(),
                            hps,
                        );
                    }
                    map_anon();
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            saved.as_ptr(),
                            aligned_vaddr as *mut u8,
                            hps,
                        );
                    }
                    pages.insert(aligned_vaddr, HostPageState::Anonymous);
                    true
                }
                Some(HostPageState::Anonymous) => true,
            };

            needs_memcpy
        };

        if needs_memcpy {
            // For anonymous mappings: copy data from PMEM backing store.
            if prot.contains(MMUFlags::EXECUTE) {
                unsafe {
                    let _ = mman::mprotect(
                        aligned_vaddr as _,
                        hps,
                        ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                    );
                }
            }
            let src = (PMEM_MAP_VADDR + paddr) as *const u8;
            let dst = vaddr as *mut u8;
            unsafe { core::ptr::copy_nonoverlapping(src, dst, len) };
        }

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

    pub fn munmap(&self, vaddr: VirtAddr, len: usize) {
        let hps = host_page_size();
        let aligned_vaddr = vaddr & !(hps - 1);
        let aligned_len = ((vaddr + len + hps - 1) & !(hps - 1)) - aligned_vaddr;
        unsafe { mman::munmap(aligned_vaddr as _, aligned_len) }
            .unwrap_or_else(|err| panic!("munmap failed: vaddr={:#x}: {:?}", aligned_vaddr, err));

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
