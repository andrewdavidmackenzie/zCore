use super::*;
use bitflags::bitflags;
use zircon_object::vm::{pages, MMUFlags, VmObject, PAGE_SIZE};

/// Syscalls for virtual memory.
///
/// # Menu
///
/// - [`brk`](Self::sys_brk)
/// - [`mmap`](Self::sys_mmap)
/// - [`mprotect`](Self::sys_mprotect)
/// - [`munmap`](Self::sys_munmap)
impl Syscall<'_> {
    /// Set the program break (end of data segment / heap).
    ///
    /// If `addr` is 0, returns the current break without changing it.
    /// If `addr` is greater than the current break, grows the heap.
    /// If `addr` is less than the current break, shrinks the heap.
    /// Returns the new program break on success.
    pub fn sys_brk(&self, addr: usize) -> SysResult {
        let proc = self.linux_process();
        let current_brk = proc.get_brk();
        info!("brk: addr={:#x}, current={:#x}", addr, current_brk);

        if addr == 0 {
            return Ok(current_brk);
        }

        // Page-align the requested address upward
        let new_brk = (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let old_brk = (current_brk + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        let vmar = self.zircon_process().vmar();

        if new_brk > old_brk {
            // Grow: map anonymous pages for the new region
            let len = new_brk - old_brk;
            let flags = MMUFlags::READ | MMUFlags::WRITE | MMUFlags::USER;
            let vmo = VmObject::new_paged(pages(len));
            let offset = old_brk - vmar.addr();
            if vmar.map(Some(offset), vmo, 0, len, flags).is_err() {
                // Cannot grow — return the current break unchanged
                return Ok(current_brk);
            }
        } else if new_brk < old_brk {
            // Shrink: unmap pages from the end
            let len = old_brk - new_brk;
            let _ = vmar.unmap(new_brk, len);
        }

        proc.set_brk(addr);
        Ok(addr)
    }

    /// Map files or devices into memory
    /// (see [linux man mmap(2)](https://www.man7.org/linux/man-pages/man2/mmap.2.html)).
    ///
    /// `sys_mmap` creates a new mapping in the virtual address space of the calling process.
    ///
    /// The starting address for the new mapping is specified in `addr`.
    ///
    /// The `len` argument specifies the length of the mapping (which must be greater than 0).
    ///
    /// Arguments `fd` and `offset` specifies mapping file descriptor and offset in the file.
    ///
    /// The `prot` argument describes the desired memory protection of the mapping
    /// (and must not conflict with the open mode of the file).
    /// It is either 0 or the bitwise OR of one or more of the following flags:
    ///
    /// - **`MmapProt::READ`**
    ///
    ///   Pages may be read
    ///
    /// - **`MmapProt::WRITE`**
    ///
    ///   Pages may be written
    ///
    /// - **`MmapProt::EXEC`**
    ///
    ///   Pages may be executed
    ///
    /// The `flags` argument determines whether updates to the mapping are visible to other processes mapping the same region,
    /// and whether updates are carried through to the underlying file.
    /// This behavior is determined by including exactly one of the following values:
    ///
    /// - **`MmapFlags::SHARED`**
    ///
    ///   Share this mapping. Updates to the mapping are visible to other processes mapping the same region,
    ///   and (in the case of file-backed mappings) are carried through to the underlying file.
    ///   (To precisely control when updates are carried through to the underlying file requires the use of `msync`,
    ///   which has not been implemented in zcore).
    ///
    /// - **`MmapFlags::PRIVATE`**
    ///
    ///   Create a private copy-on-write mapping.
    ///   Updates to the mapping are not visible to other processes mapping the same file,
    ///   and are not carried through to the underlying file.
    ///   It is unspecified whether changes made to the file after the `sys_mmap` call are visible in the mapped region.
    ///
    /// - **`MmapFlags::FIXED`**
    ///
    ///   Don't interpret `addr` as a hint: place the mapping at exactly that address.
    ///   `addr` must be suitably aligned:
    ///   for most architectures a multiple of the page size is sufficient;
    ///   however, some architectures may impose additional restrictions.
    ///   If the memory region specified by `addr` and `len` overlaps pages of any existing mapping(s),
    ///   then the overlapped part of the existing mapping(s) will be discarded.
    ///   If the specified address cannot be used, `sys_mmap` will fail.
    ///
    /// - **`MmapFlags::ANONYMOUS`**
    ///
    ///   The mapping is not backed by any file; its contents are initialized to zero.
    ///   Both `fd` and `offset` arguments are ignored.
    ///   The use of `MmapFlags::ANONYMOUS` in conjunction with `MmapFlags::SHARED`
    ///   causes an [`EINVAL`](LxError::EINVAL) to be returned.
    pub async fn sys_mmap(
        &self,
        addr: usize,
        len: usize,
        prot: usize,
        flags: usize,
        fd: FileDesc,
        offset: u64,
    ) -> SysResult {
        let prot = MmapProt::from_bits_truncate(prot);
        let flags = MmapFlags::from_bits_truncate(flags);
        info!(
            "mmap: addr={:#x}, size={:#x}, prot={:?}, flags={:?}, fd={:?}, offset={:#x}",
            addr, len, prot, flags, fd, offset
        );

        let proc = self.zircon_process();
        let vmar = proc.vmar();

        if flags.contains(MmapFlags::FIXED) {
            // unmap first
            vmar.unmap(addr, len)?;
        }
        let vmar_offset = flags.contains(MmapFlags::FIXED).then(|| addr - vmar.addr());
        if flags.contains(MmapFlags::ANONYMOUS) {
            if flags.contains(MmapFlags::SHARED) {
                return Err(LxError::EINVAL);
            }
            let vmo = VmObject::new_paged(pages(len));
            let addr = vmar.map(vmar_offset, vmo.clone(), 0, vmo.len(), prot.to_flags())?;
            Ok(addr)
        } else {
            let file_like = self.linux_process().get_file_like(fd)?;
            let vmo = file_like.get_vmo(offset as usize, len)?;
            let addr = vmar.map(vmar_offset, vmo.clone(), 0, vmo.len(), prot.to_flags())?;
            Ok(addr)
        }
    }

    /// Set protection on a region of memory
    /// (see [linux man mprotect(2)](https://www.man7.org/linux/man-pages/man2/mprotect.2.html)).
    ///
    /// `sys_mprotect` changes the access protections for the calling process's memory pages
    /// containing any part of the address range in the interval `[addr, addr+len-1]`.
    /// `addr` must be aligned to a page boundary.
    ///
    /// If the calling process tries to access memory in a manner that violates the protections,
    /// then the kernel generates a SIGSEGV signal for the process.
    ///
    /// `prot` is a combination of the following access flags:
    /// 0 or a bitwise-or of the other values in the following list:
    ///
    /// - **`MmapProt::READ`**
    ///
    ///   The memory can be read.
    ///
    /// - **`MmapProt::WRITE`**
    ///
    ///   The memory can be modified.
    ///
    /// - **`MmapProt::EXEC`**
    ///
    ///   The memory can be executed.
    ///
    /// If `prot` is 0, the memory cannot be accessed at all.
    pub fn sys_mprotect(&self, addr: usize, len: usize, prot: usize) -> SysResult {
        let prot = MmapProt::from_bits_truncate(prot);
        info!(
            "mprotect: addr={:#x}, size={:#x}, prot={:?}",
            addr, len, prot
        );
        // addr must be page-aligned
        if addr % PAGE_SIZE != 0 {
            return Err(LxError::EINVAL);
        }
        if len == 0 {
            return Ok(0);
        }
        // Check addr+len doesn't overflow before rounding
        if addr.checked_add(len).is_none() {
            return Err(LxError::ENOMEM);
        }
        // Round len up to page boundary (Linux behavior)
        let len = (len + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let proc = self.zircon_process();
        let vmar = proc.vmar();
        let flags = prot.to_flags();
        vmar.protect(addr, len, flags)?;
        Ok(0)
    }

    /// Unmap files or devices into memory
    /// (see [linux man munmap(2)](https://www.man7.org/linux/man-pages/man2/munmap.2.html)).
    ///
    /// Deletes the mappings for the specified address range, and causes further references to addresses
    /// within the range to generate invalid memory references.
    ///
    /// The `sys_munmap` system call deletes the mappings for the specified address range,
    /// and causes further references to addresses within the range to generate invalid memory references.
    /// The region is also automatically unmapped when the process is terminated.
    /// On the other hand, closing the file descriptor does not unmap the region.
    ///
    /// Both `addr` and `len` must be aligned to the page size, additionally, `len` must greater than 0.
    /// Otherwise, an [`EINVAL`](LxError::EINVAL) is returned.
    pub fn sys_munmap(&self, addr: usize, len: usize) -> SysResult {
        info!("munmap: addr={:#x}, size={:#x}", addr, len);
        let proc = self.thread.proc();
        let vmar = proc.vmar();
        vmar.unmap(addr, len)?;
        Ok(0)
    }
}

bitflags! {
    /// for the flag argument in mmap()
    pub struct MmapFlags: usize {
        #[allow(clippy::identity_op)]
        /// Changes are shared.
        const SHARED = 1 << 0;
        /// Changes are private.
        const PRIVATE = 1 << 1;
        /// Place the mapping at the exact address
        const FIXED = 1 << 4;
        /// The mapping is not backed by any file. (non-POSIX)
        const ANONYMOUS = MMAP_ANONYMOUS;
    }
}

/// MmapFlags `MMAP_ANONYMOUS` depends on arch
#[cfg(target_arch = "mips")]
const MMAP_ANONYMOUS: usize = 0x800;
#[cfg(not(target_arch = "mips"))]
const MMAP_ANONYMOUS: usize = 1 << 5;

bitflags! {
    /// for the prot argument in mmap()
    pub struct MmapProt: usize {
        #[allow(clippy::identity_op)]
        /// Data can be read
        const READ = 1 << 0;
        /// Data can be written
        const WRITE = 1 << 1;
        /// Data can be executed
        const EXEC = 1 << 2;
    }
}

impl MmapProt {
    /// Convert MmapProt to MMUFlags.
    /// When prot is empty (PROT_NONE), returns USER-only flags with no R/W/X.
    /// On aarch64, a USER-only PTE without the VALID bit is inaccessible.
    fn to_flags(self) -> MMUFlags {
        let mut flags = MMUFlags::USER;
        if self.contains(MmapProt::READ) {
            flags |= MMUFlags::READ;
        }
        if self.contains(MmapProt::WRITE) {
            flags |= MMUFlags::WRITE;
        }
        if self.contains(MmapProt::EXEC) {
            flags |= MMUFlags::EXECUTE;
        }
        flags
    }
}
