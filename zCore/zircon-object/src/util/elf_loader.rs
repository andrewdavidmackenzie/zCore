//! ELF loading of Zircon and Linux.
use crate::{error::*, vm::*};
use alloc::sync::Arc;
use xmas_elf::{
    program::{Flags, ProgramHeader, SegmentData, Type},
    sections::SectionData,
    symbol_table::{DynEntry64, Entry},
    ElfFile,
};

/// Extensional ELF loading methods for `VmAddressRegion`.
pub trait VmarExt {
    /// Create `VMObject` from all LOAD segments of `elf` and map them to this VMAR.
    /// Return the first `VMObject`.
    fn load_from_elf(&self, elf: &ElfFile) -> ZxResult<Arc<VmObject>>;
    /// Same as `load_from_elf`, but the `vmo` is an existing one instead of a lot of new ones.
    fn map_from_elf(&self, elf: &ElfFile, vmo: Arc<VmObject>) -> ZxResult;
}

impl VmarExt for VmAddressRegion {
    fn load_from_elf(&self, elf: &ElfFile) -> ZxResult<Arc<VmObject>> {
        let mut first_vmo = None;
        let mut seg_idx = 0u32;
        for ph in elf.program_iter() {
            if ph.get_type().unwrap() != Type::Load {
                continue;
            }
            trace!(
                "load_from_elf: seg {} vaddr={:#x} memsz={:#x} filesz={:#x}",
                seg_idx,
                ph.virtual_addr(),
                ph.mem_size(),
                ph.file_size()
            );
            let vmo = make_vmo(elf, ph)?;
            trace!(
                "load_from_elf: seg {} vmo created, len={:#x}",
                seg_idx,
                vmo.len()
            );
            let offset = ph.virtual_addr() as usize / PAGE_SIZE * PAGE_SIZE;
            let flags = ph.flags().to_mmu_flags();
            self.map_at(offset, vmo.clone(), 0, vmo.len(), flags)?;
            trace!(
                "load_from_elf: seg {} mapped at offset {:#x}",
                seg_idx,
                offset
            );
            first_vmo.get_or_insert(vmo);
            seg_idx += 1;
        }
        Ok(first_vmo.unwrap())
    }
    fn map_from_elf(&self, elf: &ElfFile, vmo: Arc<VmObject>) -> ZxResult {
        for ph in elf.program_iter() {
            if ph.get_type().unwrap() != Type::Load {
                continue;
            }
            let offset = ph.virtual_addr() as usize;
            let flags = ph.flags().to_mmu_flags();
            let vmo_offset = pages(ph.physical_addr() as usize) * PAGE_SIZE;
            let len = pages(ph.mem_size() as usize) * PAGE_SIZE;
            self.map_at(offset, vmo.clone(), vmo_offset, len, flags)?;
        }
        Ok(())
    }
}

trait FlagsExt {
    fn to_mmu_flags(&self) -> MMUFlags;
}

impl FlagsExt for Flags {
    fn to_mmu_flags(&self) -> MMUFlags {
        let mut flags = MMUFlags::USER;
        if self.is_read() {
            flags.insert(MMUFlags::READ);
        }
        if self.is_write() {
            flags.insert(MMUFlags::WRITE);
        }
        if self.is_execute() {
            flags.insert(MMUFlags::EXECUTE);
        }
        flags
    }
}

fn make_vmo(elf: &ElfFile, ph: ProgramHeader) -> ZxResult<Arc<VmObject>> {
    assert_eq!(ph.get_type().unwrap(), Type::Load);
    let page_offset = ph.virtual_addr() as usize % PAGE_SIZE;
    let pages = pages(ph.mem_size() as usize + page_offset);
    trace!("make_vmo: pages={}, page_offset={:#x}", pages, page_offset);
    let vmo = VmObject::new_paged(pages);
    trace!("make_vmo: vmo created");
    let data = match ph.get_data(elf).unwrap() {
        SegmentData::Undefined(data) => data,
        _ => return Err(ZxError::INVALID_ARGS),
    };
    trace!(
        "make_vmo: writing {} bytes at offset {:#x}",
        data.len(),
        page_offset
    );
    vmo.write(page_offset, data)?;
    trace!("make_vmo: done");
    Ok(vmo)
}

/// Extensional ELF loading methods for `ElfFile`.
pub trait ElfExt {
    /// Get total size of all LOAD segments.
    fn load_segment_size(&self) -> usize;
    /// Get address of the given `symbol`.
    fn get_symbol_address(&self, symbol: &str) -> Option<u64>;
    /// Get the program interpreter path name.
    fn get_interpreter(&self) -> Result<&str, &str>;
    /// Get address of elf phdr
    fn get_phdr_vaddr(&self) -> Option<u64>;
    /// Get the symbol table for dynamic linking (.dynsym section).
    fn dynsym(&self) -> Result<&[DynEntry64], &'static str>;
    /// Relocate according to the dynamic relocation section (.rel.dyn section).
    fn relocate(&self, vmar: Arc<VmAddressRegion>) -> Result<(), &'static str>;
    /// Check if the ELF has DT_TEXTREL in its DYNAMIC section.
    ///
    /// DT_TEXTREL indicates that relocations may modify read-only text
    /// segments, requiring the loader to make them writable before the
    /// binary's self-relocation code (rcrt1) runs.
    fn has_textrel(&self) -> bool;
}

impl ElfExt for ElfFile<'_> {
    fn load_segment_size(&self) -> usize {
        self.program_iter()
            .filter(|ph| ph.get_type().unwrap() == Type::Load)
            .map(|ph| pages((ph.virtual_addr() + ph.mem_size()) as usize))
            .max()
            .unwrap_or(0)
            * PAGE_SIZE
    }

    fn get_symbol_address(&self, symbol: &str) -> Option<u64> {
        for section in self.section_iter() {
            if let SectionData::SymbolTable64(entries) = section.get_data(self).unwrap() {
                for e in entries {
                    if e.get_name(self).unwrap() == symbol {
                        return Some(e.value());
                    }
                }
            }
        }
        None
    }

    fn get_interpreter(&self) -> Result<&str, &str> {
        let header = self
            .program_iter()
            .find(|ph| ph.get_type() == Ok(Type::Interp))
            .ok_or("no interp header")?;
        let data = match header.get_data(self)? {
            SegmentData::Undefined(data) => data,
            _ => return Err("bad interp"),
        };
        let len = (0..).find(|&i| data[i] == 0).unwrap();
        let path = core::str::from_utf8(&data[..len]).map_err(|_| "failed to convert to utf8")?;
        Ok(path)
    }

    fn get_phdr_vaddr(&self) -> Option<u64> {
        if let Some(phdr) = self
            .program_iter()
            .find(|ph| ph.get_type() == Ok(Type::Phdr))
        {
            // if phdr exists in program header, use it
            Some(phdr.virtual_addr())
        } else if let Some(elf_addr) = self
            .program_iter()
            .find(|ph| ph.get_type() == Ok(Type::Load) && ph.offset() == 0)
        {
            // otherwise, check if elf is loaded from the beginning, then phdr can be inferred.
            Some(elf_addr.virtual_addr() + self.header.pt2.ph_offset())
        } else {
            warn!("elf: no phdr found, tls might not work");
            None
        }
    }

    fn dynsym(&self) -> Result<&[DynEntry64], &'static str> {
        match self
            .find_section_by_name(".dynsym")
            .ok_or(".dynsym not found")?
            .get_data(self)
            .map_err(|_| "corrupted .dynsym")?
        {
            SectionData::DynSymbolTable64(dsym) => Ok(dsym),
            _ => Err("bad .dynsym"),
        }
    }

    /// Scan PT_DYNAMIC for a DT_TEXTREL entry.
    fn has_textrel(&self) -> bool {
        for ph in self.program_iter() {
            if ph.get_type() != Ok(Type::Dynamic) {
                continue;
            }
            if let Ok(SegmentData::Dynamic64(entries)) = ph.get_data(self) {
                for entry in entries {
                    if entry.get_tag() == Ok(xmas_elf::dynamic::Tag::TextRel) {
                        return true;
                    }
                }
            }
        }
        false
    }

    #[allow(unsafe_code)]
    fn relocate(&self, vmar: Arc<VmAddressRegion>) -> Result<(), &'static str> {
        // Try section-based lookup first (works for unstripped ELFs).
        // Fall back to PT_DYNAMIC-based lookup for stripped binaries.
        let rela_data = if let Some(section) = self.find_section_by_name(".rela.dyn") {
            match section.get_data(self) {
                Ok(SectionData::Rela64(entries)) => Some(entries),
                _ => None,
            }
        } else {
            None
        };

        let base = vmar.addr();

        if let Some(entries) = rela_data {
            // Section-based relocation (unstripped ELF).
            let dynsym = self.dynsym()?;
            apply_rela_entries(entries, base, &dynsym, self, &vmar)?;
        } else {
            // Stripped binary: find relocations via PT_DYNAMIC.
            let info = find_reloc_from_dynamic(self)?;
            warn!(
                "relocate: PT_DYNAMIC: rela={:#x}/{:#x}, rel={:#x}/{:#x}, relr={:#x}/{:#x}, base={:#x}",
                info.rela_off, info.rela_sz, info.rel_off, info.rel_sz,
                info.relr_off, info.relr_sz, base
            );

            // Apply RELA entries (24-byte, explicit addend).
            if info.rela_sz > 0 && info.rela_ent > 0 {
                let count = info.rela_sz as usize / info.rela_ent as usize;
                for i in 0..count {
                    let addr = base + info.rela_off as usize + i * info.rela_ent as usize;
                    let mut buf = [0u8; 24];
                    vmar.read_memory(addr, &mut buf)
                        .map_err(|_| "failed to read rela entry")?;
                    let r_offset = u64::from_le_bytes(buf[0..8].try_into().unwrap());
                    let r_addend = i64::from_le_bytes(buf[16..24].try_into().unwrap());
                    let r_info = u64::from_le_bytes(buf[8..16].try_into().unwrap());
                    let r_type = (r_info & 0xFFFF_FFFF) as u32;
                    if r_type == 8 || r_type == 0x403 || r_type == 3 {
                        // RELATIVE
                        let value = base + r_addend as usize;
                        let target = base + r_offset as usize;
                        vmar.write_memory(target, &value.to_ne_bytes())
                            .map_err(|_| "RELA write failed")?;
                    }
                }
                warn!("relocate: applied {} RELA entries", count);
            }

            // Apply RELR entries (compact relative-only format).
            if info.relr_sz > 0 {
                warn!(
                    "relocate: RELR at file offset {:#x}, sz={:#x}, ent={:#x}",
                    info.relr_off, info.relr_sz, info.relr_ent
                );
                let applied = apply_relr(
                    self.input,
                    info.relr_off,
                    info.relr_sz,
                    info.relr_ent,
                    base,
                    &vmar,
                )?;
                warn!("relocate: applied {} RELR entries", applied);
            }

            // Also apply REL entries (needed for GOT/PLT on ld.so.1).
            if info.rel_sz > 0 && info.rel_ent > 0 {
                let count = info.rel_sz as usize / info.rel_ent as usize;
                let mut rel_applied = 0usize;
                for i in 0..count {
                    let off = info.rel_off as usize + i * info.rel_ent as usize;
                    if off + 16 > self.input.len() {
                        break;
                    }
                    let r_offset = u64::from_le_bytes(self.input[off..off + 8].try_into().unwrap());
                    let r_info =
                        u64::from_le_bytes(self.input[off + 8..off + 16].try_into().unwrap());
                    let r_type = (r_info & 0xFFFF_FFFF) as u32;
                    // R_X86_64_RELATIVE = 8 (but REL has implicit addend)
                    if r_type == 8 {
                        // Read addend from the current value at offset
                        let vaddr = r_offset as usize;
                        if vaddr + 8 <= self.input.len() {
                            let addend = usize::from_le_bytes(
                                self.input[vaddr..vaddr + 8].try_into().unwrap(),
                            );
                            let value = base + addend;
                            let target = base + vaddr;
                            vmar.write_memory(target, &value.to_ne_bytes())
                                .map_err(|_| "REL write failed")?;
                            rel_applied += 1;
                        }
                    }
                    // Skip non-RELATIVE REL entries — ld.so.1 resolves
                    // those itself after self-relocation.
                }
                warn!(
                    "relocate: applied {} REL RELATIVE entries out of {}",
                    rel_applied, count
                );
            }
        }
        Ok(())
    }
}

/// Apply relocation entries from a parsed Rela64 slice.
fn apply_rela_entries(
    entries: &[xmas_elf::sections::Rela<u64>],
    base: usize,
    dynsym: &[xmas_elf::symbol_table::DynEntry64],
    elf: &ElfFile,
    vmar: &Arc<VmAddressRegion>,
) -> Result<(), &'static str> {
    for entry in entries.iter() {
        const REL_GOT: u32 = 6;
        const REL_PLT: u32 = 7;
        const REL_RELATIVE: u32 = 8;
        const R_RISCV_64: u32 = 2;
        const R_RISCV_RELATIVE: u32 = 3;
        const R_AARCH64_RELATIVE: u32 = 0x403;
        const R_AARCH64_GLOBAL_DATA: u32 = 0x401;

        match entry.get_type() {
            REL_GOT | REL_PLT | R_RISCV_64 | R_AARCH64_GLOBAL_DATA => {
                let sym = &dynsym[entry.get_symbol_table_index() as usize];
                let symval = if sym.shndx() == 0 {
                    let name = sym.get_name(elf)?;
                    panic!("need to find symbol: {:?}", name);
                } else {
                    base + sym.value() as usize
                };
                let value = symval + entry.get_addend() as usize;
                let addr = base + entry.get_offset() as usize;
                vmar.write_memory(addr, &value.to_ne_bytes())
                    .map_err(|_| "Invalid Vmar")?;
            }
            REL_RELATIVE | R_RISCV_RELATIVE | R_AARCH64_RELATIVE => {
                let value = base + entry.get_addend() as usize;
                let addr = base + entry.get_offset() as usize;
                vmar.write_memory(addr, &value.to_ne_bytes())
                    .map_err(|_| "Invalid Vmar")?;
            }
            0 => {} // R_*_NONE
            t => {
                warn!("unknown relocation type: {}", t);
            }
        }
    }
    Ok(())
}

/// Dynamic relocation info parsed from PT_DYNAMIC.
struct DynRelocInfo {
    /// DT_RELA offset and size (24-byte entries with explicit addend).
    rela_off: u64,
    rela_sz: u64,
    rela_ent: u64,
    /// DT_REL offset and size (16-byte entries, implicit addend).
    rel_off: u64,
    rel_sz: u64,
    rel_ent: u64,
    /// DT_RELR offset and size (compact relative-only format).
    relr_off: u64,
    relr_sz: u64,
    relr_ent: u64,
    /// DT_JMPREL (PLT relocations).
    jmprel_off: u64,
    jmprel_sz: u64,
}

/// Parse dynamic relocation info from PT_DYNAMIC.
fn find_reloc_from_dynamic(elf: &ElfFile) -> Result<DynRelocInfo, &'static str> {
    use xmas_elf::program::Type;
    let dyn_ph = elf
        .program_iter()
        .find(|ph| ph.get_type() == Ok(Type::Dynamic))
        .ok_or("no PT_DYNAMIC")?;
    let dyn_offset = dyn_ph.offset() as usize;
    let dyn_size = dyn_ph.file_size() as usize;
    let raw = &elf.input[dyn_offset..dyn_offset + dyn_size];

    let mut info = DynRelocInfo {
        rela_off: 0,
        rela_sz: 0,
        rela_ent: 0,
        rel_off: 0,
        rel_sz: 0,
        rel_ent: 0,
        relr_off: 0,
        relr_sz: 0,
        relr_ent: 0,
        jmprel_off: 0,
        jmprel_sz: 0,
    };

    let mut i = 0;
    while i + 16 <= raw.len() {
        let tag = i64::from_le_bytes(raw[i..i + 8].try_into().unwrap());
        let val = u64::from_le_bytes(raw[i + 8..i + 16].try_into().unwrap());
        match tag {
            0 => break,                  // DT_NULL
            7 => info.rela_off = val,    // DT_RELA
            8 => info.rela_sz = val,     // DT_RELASZ
            9 => info.rela_ent = val,    // DT_RELAENT
            17 => info.rel_off = val,    // DT_REL
            18 => info.rel_sz = val,     // DT_RELSZ
            19 => info.rel_ent = val,    // DT_RELENT
            23 => info.jmprel_off = val, // DT_JMPREL
            2 => info.jmprel_sz = val,   // DT_PLTRELSZ
            36 => info.relr_off = val,   // DT_RELR
            35 => info.relr_sz = val,    // DT_RELRSZ
            37 => info.relr_ent = val,   // DT_RELRENT
            _ => {}
        }
        i += 16;
    }
    Ok(info)
}

/// Apply RELR (compact relative relocations).
///
/// RELR is a packed format where each entry is either:
/// - A base address (even value): apply a RELATIVE relocation at that address
/// - A bitmap (odd value): apply relocations at offsets from the last base,
///   one per set bit
fn apply_relr(
    elf_data: &[u8],
    relr_off: u64,
    relr_sz: u64,
    relr_ent: u64,
    base: usize,
    vmar: &Arc<VmAddressRegion>,
) -> Result<usize, &'static str> {
    if relr_sz == 0 || relr_ent == 0 {
        return Ok(0);
    }
    let entry_size = relr_ent as usize;
    let count = relr_sz as usize / entry_size;
    let mut applied = 0usize;
    let mut where_addr = 0usize;

    for i in 0..count {
        let off = relr_off as usize + i * entry_size;
        if off + 8 > elf_data.len() {
            break;
        }
        let entry = u64::from_le_bytes(elf_data[off..off + 8].try_into().unwrap());

        if entry & 1 == 0 {
            // Even: absolute address — apply one relocation here.
            where_addr = entry as usize;
            let value = base + read_usize_at(elf_data, where_addr);
            let target = base + where_addr;
            vmar.write_memory(target, &value.to_ne_bytes())
                .map_err(|_| "RELR write failed")?;
            where_addr += core::mem::size_of::<usize>();
            applied += 1;
        } else {
            // Odd: bitmap — each set bit is a relocation at
            // where_addr + bit_index * sizeof(usize).
            let mut bitmap = entry >> 1;
            let mut addr = where_addr;
            while bitmap != 0 {
                if bitmap & 1 != 0 {
                    let value = base + read_usize_at(elf_data, addr);
                    let target = base + addr;
                    vmar.write_memory(target, &value.to_ne_bytes())
                        .map_err(|_| "RELR bitmap write failed")?;
                    applied += 1;
                }
                bitmap >>= 1;
                addr += core::mem::size_of::<usize>();
            }
            where_addr = addr;
        }
    }
    Ok(applied)
}

/// Read a usize from ELF file data at a virtual address offset.
fn read_usize_at(data: &[u8], vaddr: usize) -> usize {
    if vaddr + core::mem::size_of::<usize>() <= data.len() {
        usize::from_le_bytes(
            data[vaddr..vaddr + core::mem::size_of::<usize>()]
                .try_into()
                .unwrap_or([0; core::mem::size_of::<usize>()]),
        )
    } else {
        0
    }
}
