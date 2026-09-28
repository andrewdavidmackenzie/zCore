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
            // Stripped binary: find DT_RELA/DT_RELASZ via PT_DYNAMIC.
            let (rela_off, rela_sz, relent) = find_rela_from_dynamic(self)?;
            if rela_sz > 0 && relent > 0 {
                let entry_size = relent as usize;
                let count = rela_sz as usize / entry_size;
                // Read raw relocation data from the VMAR (already mapped).
                for i in 0..count {
                    let addr = base + rela_off as usize + i * entry_size;
                    let mut buf = [0u8; 24]; // sizeof(Elf64_Rela)
                    vmar.read_memory(addr, &mut buf)
                        .map_err(|_| "failed to read rela entry")?;
                    let r_offset = u64::from_le_bytes(buf[0..8].try_into().unwrap());
                    let r_info = u64::from_le_bytes(buf[8..16].try_into().unwrap());
                    let r_addend = i64::from_le_bytes(buf[16..24].try_into().unwrap());
                    let r_type = (r_info & 0xFFFF_FFFF) as u32;

                    // Only handle RELATIVE relocations for stripped binaries
                    // (the dynamic linker self-relocates, no symbol lookup needed).
                    const REL_RELATIVE: u32 = 8; // R_X86_64_RELATIVE
                    const R_AARCH64_RELATIVE: u32 = 0x403;
                    const R_RISCV_RELATIVE: u32 = 3;
                    match r_type {
                        REL_RELATIVE | R_AARCH64_RELATIVE | R_RISCV_RELATIVE => {
                            let value = base + r_addend as usize;
                            let target = base + r_offset as usize;
                            vmar.write_memory(target, &value.to_ne_bytes())
                                .map_err(|_| "failed to write relocation")?;
                        }
                        0 => {} // R_X86_64_NONE — skip
                        _ => {
                            // Skip non-RELATIVE relocations for now.
                            // The dynamic linker handles these itself.
                        }
                    }
                }
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

/// Find DT_RELA, DT_RELASZ, DT_RELAENT from PT_DYNAMIC.
fn find_rela_from_dynamic(elf: &ElfFile) -> Result<(u64, u64, u64), &'static str> {
    use xmas_elf::program::Type;
    let dyn_ph = elf
        .program_iter()
        .find(|ph| ph.get_type() == Ok(Type::Dynamic))
        .ok_or("no PT_DYNAMIC")?;
    let dyn_offset = dyn_ph.offset() as usize;
    let dyn_size = dyn_ph.file_size() as usize;
    let raw = &elf.input[dyn_offset..dyn_offset + dyn_size];

    let mut rela_off = 0u64;
    let mut rela_sz = 0u64;
    let mut relent = 0u64;

    let mut i = 0;
    while i + 16 <= raw.len() {
        let tag = i64::from_le_bytes(raw[i..i + 8].try_into().unwrap());
        let val = u64::from_le_bytes(raw[i + 8..i + 16].try_into().unwrap());
        match tag {
            0 => break,          // DT_NULL
            7 => rela_off = val, // DT_RELA
            8 => rela_sz = val,  // DT_RELASZ
            9 => relent = val,   // DT_RELAENT
            _ => {}
        }
        i += 16;
    }
    Ok((rela_off, rela_sz, relent))
}
