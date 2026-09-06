use std::os::unix::io::RawFd;

use nix::fcntl::{self, OFlag};
use nix::sys::mman::{self, MapFlags, ProtFlags};
use nix::{sys::stat::Mode, unistd};

use super::mem::PMEM_MAP_VADDR;
use crate::{MMUFlags, PhysAddr, VirtAddr};

pub struct MockMemory {
    size: usize,
    fd: RawFd,
    /// Track which host-page-aligned vaddrs have been MAP_ANON'd for
    /// executable code on aarch64 macOS (to avoid clobbering previous
    /// pages when multiple 4K pages share one host page).
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    anon_mapped: std::sync::Mutex<std::collections::HashSet<usize>>,
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
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            anon_mapped: std::sync::Mutex::new(std::collections::HashSet::new()),
        };
        mem.mmap(PMEM_MAP_VADDR, size, 0, MMUFlags::READ | MMUFlags::WRITE);
        mem
    }

    /// Mmap `paddr` to `vaddr` in frame file.
    pub fn mmap(&self, vaddr: VirtAddr, len: usize, paddr: PhysAddr, prot: MMUFlags) {
        assert!(paddr < self.size);
        assert!(paddr + len <= self.size);

        // workaround on macOS to write text section.
        #[cfg(target_os = "macos")]
        let prot = if prot.contains(MMUFlags::EXECUTE) {
            prot | MMUFlags::WRITE
        } else {
            prot
        };

        // The host OS page size may be larger than zCore's 4K page size
        // (e.g., 16K on aarch64 macOS). mmap requires vaddr, offset, and
        // length to be aligned to the host page size. We align down the
        // vaddr and offset, and align up the length, mapping a larger
        // region that covers the requested range.
        let host_page_size = unsafe { nix::libc::sysconf(nix::libc::_SC_PAGESIZE) as usize };
        let aligned_vaddr = vaddr & !(host_page_size - 1);
        let vaddr_adjust = vaddr - aligned_vaddr;
        let aligned_offset = paddr & !(host_page_size - 1);
        // Both adjustments should be equal since vaddr and paddr have the
        // same alignment within the mock physical memory.
        let total_adjust = vaddr_adjust.max(paddr - aligned_offset);
        let aligned_len = (len + total_adjust + host_page_size - 1) & !(host_page_size - 1);

        let prot_noexec = ProtFlags::from(prot) - ProtFlags::PROT_EXEC;
        let fd = self.fd;
        trace!(
            "mmap file: fd={}, offset={:#x} (aligned={:#x}), len={:#x} (aligned={:#x}), vaddr={:#x} (aligned={:#x}), prot={:?}",
            fd, paddr, aligned_offset, len, aligned_len, vaddr, aligned_vaddr, prot,
        );

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        if prot.contains(MMUFlags::EXECUTE) {
            // On aarch64 macOS:
            // - MAP_SHARED + PROT_EXEC is blocked by hardened runtime
            // - MAP_PRIVATE from file + mprotect(RX) fails with EACCES
            // - Multiple 4K pages within one 16K host page can't be mapped
            //   from different file offsets (last one clobbers previous)
            //
            // Solution: use MAP_ANON + MAP_PRIVATE for writable anonymous
            // memory, memcpy the code data from the PMEM backing store,
            // then defer mprotect to RX until all pages are filled.
            //
            // We track which host pages have been mapped to avoid
            // re-mapping (which would zero out previously copied data).
            {
                let mut mapped = self.anon_mapped.lock().unwrap();
                if !mapped.contains(&aligned_vaddr) {
                    // First time seeing this host page -- mprotect any
                    // adjacent already-RX page back to RW so we can
                    // continue writing.
                    // Actually, just map fresh anonymous RW memory.
                    unsafe {
                        mman::mmap(
                            aligned_vaddr as _,
                            aligned_len,
                            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                            MapFlags::MAP_PRIVATE | MapFlags::MAP_FIXED | MapFlags::MAP_ANON,
                            -1,
                            0,
                        )
                    }
                    .unwrap_or_else(|err| {
                        panic!(
                            "failed to mmap anon: len={:#x}, vaddr={:#x}: {:?}",
                            aligned_len, aligned_vaddr, err
                        )
                    });
                    mapped.insert(aligned_vaddr);
                } else {
                    // Host page already mapped as RW -- make sure it's
                    // writable (might have been mprotected to RX).
                    self.mprotect(aligned_vaddr, aligned_len, MMUFlags::READ | MMUFlags::WRITE);
                }
            }
            // Copy code from PMEM backing store
            let src = (PMEM_MAP_VADDR + paddr) as *const u8;
            let dst = vaddr as *mut u8;
            unsafe { core::ptr::copy_nonoverlapping(src, dst, len) };
            // Switch to read+execute
            self.mprotect(
                aligned_vaddr,
                aligned_len,
                MMUFlags::READ | MMUFlags::EXECUTE,
            );
            return;
        }

        let flags = MapFlags::MAP_SHARED | MapFlags::MAP_FIXED;
        unsafe {
            mman::mmap(
                aligned_vaddr as _,
                aligned_len,
                prot_noexec,
                flags,
                fd,
                aligned_offset as _,
            )
        }
        .unwrap_or_else(|err| {
            panic!(
                "failed to mmap: fd={}, offset={:#x}, len={:#x}, vaddr={:#x}, prot={:?}: {:?}",
                fd, aligned_offset, aligned_len, aligned_vaddr, prot, err
            )
        });
        if prot.contains(MMUFlags::EXECUTE) {
            self.mprotect(vaddr, len, prot);
        }
    }

    pub fn munmap(&self, vaddr: VirtAddr, len: usize) {
        unsafe { mman::munmap(vaddr as _, len) }
            .unwrap_or_else(|err| panic!("failed to munmap: vaddr={:#x}: {:?}", vaddr, err));
    }

    pub fn mprotect(&self, vaddr: VirtAddr, len: usize, prot: MMUFlags) {
        // Align to host page size (may be larger than zCore's 4K pages)
        let host_page_size = unsafe { nix::libc::sysconf(nix::libc::_SC_PAGESIZE) as usize };
        let aligned_vaddr = vaddr & !(host_page_size - 1);
        let adjust = vaddr - aligned_vaddr;
        let aligned_len = (len + adjust + host_page_size - 1) & !(host_page_size - 1);
        unsafe { mman::mprotect(aligned_vaddr as _, aligned_len, prot.into()) }.unwrap_or_else(
            |err| {
                panic!(
                    "failed to mprotect: vaddr={:#x}, len={:#x}, prot={:?}: {:?}",
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
