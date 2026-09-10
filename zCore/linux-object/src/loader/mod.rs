//! Linux ELF Program Loader
#![deny(missing_docs)]

use {
    crate::error::LxResult,
    crate::fs::INodeExt,
    alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec},
    rcore_fs::vfs::INode,
    xmas_elf::ElfFile,
    zircon_object::{util::elf_loader::*, vm::*, ZxError},
};

mod abi;

/// Linux ELF Program Loader.
pub struct LinuxElfLoader {
    /// syscall entry
    pub syscall_entry: usize,
    /// stack page number
    pub stack_pages: usize,
    /// root inode of LinuxElfLoader
    pub root_inode: Arc<dyn INode>,
}

impl LinuxElfLoader {
    /// Load a Linux ELF and return (entry, sp, initial_brk).
    pub fn load(
        &self,
        vmar: &Arc<VmAddressRegion>,
        data: &[u8],
        args: Vec<String>,
        envs: Vec<String>,
        path: String,
    ) -> LxResult<(VirtAddr, VirtAddr, VirtAddr)> {
        debug!(
            "load: vmar.addr & size: {:#x?}, data {:#x?}, args: {:?}, envs: {:?}",
            vmar.get_info(),
            data.as_ptr(),
            args,
            envs
        );

        // On macOS aarch64 (libos mode), Linux binaries use `svc #0` for
        // syscalls. Replace with `brk #1` so macOS delivers SIGTRAP
        // instead of processing it as a Mach trap. Unlike svc, brk
        // does NOT corrupt any registers (XNU's syscall return path
        // overwrites x0/x1 for svc, but brk bypasses it entirely).
        #[cfg(all(feature = "libos", target_arch = "aarch64", target_os = "macos"))]
        let data = {
            use xmas_elf::program::Type as PhType;
            const SVC_0: [u8; 4] = 0xd4000001u32.to_le_bytes();
            const BRK_1: [u8; 4] = 0xd4200020u32.to_le_bytes(); // brk #1
            let pre_elf = ElfFile::new(data).map_err(|_| ZxError::INVALID_ARGS)?;
            let mut patched_data = data.to_vec();
            let mut total = 0usize;
            for ph in pre_elf.program_iter() {
                if ph.get_type().unwrap() != PhType::Load || !ph.flags().is_execute() {
                    continue;
                }
                let file_start = ph.offset() as usize;
                let file_end = file_start + ph.file_size() as usize;
                let vaddr_mod4 = ph.virtual_addr() as usize % 4;
                let offset_mod4 = file_start % 4;
                let align_adj = (4 + vaddr_mod4 - offset_mod4) % 4;
                let scan_start = file_start + align_adj;
                for i in (scan_start..file_end.saturating_sub(3)).step_by(4) {
                    if patched_data[i..i + 4] == SVC_0 {
                        patched_data[i..i + 4].copy_from_slice(&BRK_1);
                        total += 1;
                    }
                }
            }
            if total > 0 {
                info!("Patched {} svc #0 -> brk #1 in executable segments", total);
            }
            patched_data
        };
        #[cfg(all(feature = "libos", target_arch = "aarch64", target_os = "macos"))]
        let data: &[u8] = &data;

        let elf = ElfFile::new(data).map_err(|_| ZxError::INVALID_ARGS)?;

        debug!("elf info:  {:#x?}", elf.header.pt2);

        if let Ok(interp) = elf.get_interpreter() {
            info!("interp: {:?}, path: {:?}", interp, path);
            let inode = self.root_inode.lookup(interp)?;
            let data = inode.read_as_vec()?;
            let mut new_args = vec![interp.into(), path.clone()];
            new_args.extend_from_slice(&args[1..]);
            return self.load(vmar, &data, new_args, envs, path);
        }

        let size = elf.load_segment_size();
        let image_vmar = vmar.allocate(None, size, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)?;
        let vmo = image_vmar.load_from_elf(&elf)?;

        // The VMAR maps ELF segments at image_vmar.addr() + ph.virtual_addr().
        // The "base" is image_vmar.addr(), used to compute AT_BASE, AT_PHDR,
        // AT_ENTRY, and initial_brk as offsets from image_vmar.addr().
        let base = image_vmar.addr();
        let entry = base + elf.header.pt2.entry_point() as usize;
        debug!(
            "load: vmar.addr & size: {:#x?}, base: {:#x?}, entry: {:#x?}",
            vmar.get_info(),
            base,
            entry
        );

        // fill syscall entry
        if let Some(offset) = elf.get_symbol_address("rcore_syscall_entry") {
            vmo.write(offset as usize, &self.syscall_entry.to_ne_bytes())?;
        }

        // For PIE (DYN type) binaries, we normally skip our relocator
        // because the binary's rcrt1 startup code does self-relocation.
        //
        // However, on aarch64 macOS (libos mode), static-PIE binaries
        // with TEXTREL can't self-relocate because macOS W^X enforcement
        // prevents text pages from being both writable and executable.
        // In this case we apply relocations in the loader (which can use
        // write_memory() to temporarily make RX pages writable) so rcrt1
        // sees already-relocated values and the writes become idempotent.
        use xmas_elf::header::Type;
        let is_pie = elf.header.pt2.type_().as_type() == Type::SharedObject;
        if !is_pie {
            match elf.relocate(image_vmar) {
                Ok(()) => info!("elf relocate passed !"),
                Err(error) => {
                    warn!("elf relocate Err:{:?}, base {:x?}", error, base);
                }
            }
        } else {
            // On libos with TEXTREL, apply relocations ourselves since
            // rcrt1 can't write to RX pages on W^X-enforcing hosts.
            // On other platforms, skip and let rcrt1 handle it.
            #[cfg(feature = "libos")]
            {
                if elf.has_textrel() {
                    info!(
                        "PIE binary with TEXTREL: applying relocations in loader (W^X workaround)"
                    );
                    elf.relocate(image_vmar.clone()).map_err(|e| {
                        warn!("PIE TEXTREL relocate failed: {:?}, base {:x?}", e, base);
                        ZxError::INVALID_ARGS
                    })?;
                    // Zero out RELASZ in the DYNAMIC section so rcrt1
                    // sees no relocations and skips its relocation loop.
                    // Without this, rcrt1 tries to re-apply the same
                    // TEXTREL relocations and crashes on macOS W^X.
                    for ph in elf.program_iter() {
                        if ph.get_type().unwrap() != xmas_elf::program::Type::Dynamic {
                            continue;
                        }
                        if let Ok(xmas_elf::program::SegmentData::Dynamic64(entries)) =
                            ph.get_data(&elf)
                        {
                            for (i, entry) in entries.iter().enumerate() {
                                // DT_RELASZ = 8: zero out the size so rcrt1
                                // thinks there are no RELA entries to process.
                                if entry.get_tag() == Ok(xmas_elf::dynamic::Tag::RelaSize) {
                                    let dyn_vaddr = ph.virtual_addr() as usize + i * 16 + 8;
                                    let addr = base + dyn_vaddr;
                                    let zero = 0usize.to_ne_bytes();
                                    image_vmar
                                        .write_memory(addr, &zero)
                                        .map_err(|_| ZxError::INVALID_ARGS)?;
                                    trace!("Zeroed DT_RELASZ at {:#x}", addr);
                                    break;
                                }
                            }
                        }
                    }

                    info!("PIE TEXTREL relocations applied");
                } else {
                    info!("PIE binary: skipping relocator (rcrt1 will self-relocate)");
                }
            }
            #[cfg(not(feature = "libos"))]
            {
                info!("PIE binary: skipping relocator (rcrt1 will self-relocate)");
            }
        }

        let stack_vmo = VmObject::new_paged(self.stack_pages);
        let flags = MMUFlags::READ | MMUFlags::WRITE | MMUFlags::USER;
        let stack_bottom = vmar.map(None, stack_vmo.clone(), 0, stack_vmo.len(), flags)?;
        let mut sp = stack_bottom + stack_vmo.len();
        debug!("load stack bottom: {:#x}", stack_bottom);

        let info = abi::ProcInitInfo {
            args,
            envs,
            auxv: {
                let mut map = BTreeMap::new();
                #[cfg(target_arch = "x86_64")]
                {
                    use xmas_elf::header::Type;
                    let is_pie = elf.header.pt2.type_().as_type() == Type::SharedObject;
                    if is_pie {
                        map.insert(abi::AT_BASE, 0);
                    } else {
                        map.insert(abi::AT_BASE, base);
                    }
                    map.insert(abi::AT_PHDR, base + elf.header.pt2.ph_offset() as usize);
                    map.insert(abi::AT_ENTRY, entry);
                }
                #[cfg(target_arch = "riscv64")]
                if let Some(phdr_vaddr) = elf.get_phdr_vaddr() {
                    map.insert(abi::AT_PHDR, phdr_vaddr as usize);
                }
                #[cfg(target_arch = "aarch64")]
                {
                    // For static-pie (DYN type), AT_BASE = 0 (no interpreter).
                    // For non-PIE (EXEC type), AT_BASE = load base.
                    if is_pie {
                        map.insert(abi::AT_BASE, 0);
                    } else {
                        map.insert(abi::AT_BASE, base);
                    }
                    map.insert(abi::AT_ENTRY, entry);
                    if let Some(phdr_vaddr) = elf.get_phdr_vaddr() {
                        // Relocate PHDR address by the load base
                        map.insert(abi::AT_PHDR, base + phdr_vaddr as usize);
                    }
                }
                map.insert(abi::AT_PHENT, elf.header.pt2.ph_entry_size() as usize);
                map.insert(abi::AT_PHNUM, elf.header.pt2.ph_count() as usize);
                map.insert(abi::AT_PAGESZ, PAGE_SIZE);
                // AT_RANDOM: value is a placeholder; push_at() replaces it
                // with the actual stack address of the 16 random bytes.
                map.insert(abi::AT_RANDOM, 0);
                // User/group identity — always root (uid=0, gid=0)
                map.insert(abi::AT_UID, 0);
                map.insert(abi::AT_EUID, 0);
                map.insert(abi::AT_GID, 0);
                map.insert(abi::AT_EGID, 0);
                // Clock ticks per second (matches Linux default of 100 Hz)
                map.insert(abi::AT_CLKTCK, 100);
                // Not running in secure mode (no setuid/setgid)
                map.insert(abi::AT_SECURE, 0);
                // Hardware capabilities (0 = no special capabilities)
                map.insert(abi::AT_HWCAP, 0);
                map
            },
        };
        let init_stack = info.push_at(sp, self.stack_pages * PAGE_SIZE)?;
        stack_vmo.write(self.stack_pages * PAGE_SIZE - init_stack.len(), &init_stack)?;
        sp -= init_stack.len();

        debug!(
            "ProcInitInfo auxv: {:#x?}\nentry:{:#x}, sp:{:#x}",
            info.auxv, entry, sp
        );

        // Initial brk = end of loaded image (page-aligned)
        let initial_brk = base + size;

        Ok((entry, sp, initial_brk))
    }
}
