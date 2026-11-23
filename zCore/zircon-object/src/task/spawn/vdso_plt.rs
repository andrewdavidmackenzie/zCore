//! vDSO symbol resolution for dynamically-linked ELF binaries.
//!
//! Eagerly resolves `_zx_*` and `zx_*` relocations in a loaded ELF
//! against the mapped vDSO's dynamic symbol table:
//! - `JUMP_SLOT` (PLT GOT entries) — needed because `ld.so.1` calls
//!   `_zx_*` functions via PLT before its lazy binding resolver is
//!   initialized;
//! - `GLOB_DAT` (non-PLT GOT entries) — needed by static PIEs that
//!   link against the vDSO directly (RODSO-style, e.g. Fuchsia's
//!   userboot, which has `NEEDED libzircon.so` and `BIND_NOW`).
//!
//! Only needed when running dynamically-linked Fuchsia binaries.

use alloc::sync::Arc;

use crate::vm::VmAddressRegion;

/// Resolve vDSO symbol relocations (GLOB_DAT and JUMP_SLOT) in a
/// loaded ELF against the vDSO mapped at `vdso_base`.
///
/// Parses the vDSO ELF to build a symbol lookup table, then patches
/// GOT entries in `elf_data` (loaded at `elf_base`) that reference
/// `_zx_*` or `zx_*` symbols to point to the vDSO trampoline
/// addresses. Handles both the PLT relocation section (DT_JMPREL)
/// and the dynamic relocation section (DT_REL), covering ld.so.1
/// (lazy PLT) and RODSO-style static PIEs like userboot (BIND_NOW
/// GLOB_DAT) alike.
pub fn resolve_vdso_symbols(
    elf_data: &[u8],
    elf_base: usize,
    vdso_base: usize,
    vmar: &Arc<VmAddressRegion>,
) {
    // Parse the vDSO ELF to build symbol name -> offset map.
    // The vDSO ELF is the same data that's in the kernel's VDSO_ELF static.
    // We access it via the VMO content since we don't have a direct reference here.
    // Read the vDSO from the mapped memory.
    let vdso_vmo_data = {
        // Read the vDSO ELF header to find .dynsym and .dynstr
        let mut ehdr = [0u8; 64];
        if vmar.read_memory(vdso_base, &mut ehdr).is_err() {
            warn!("resolve_vdso_plt: cannot read vDSO ELF header");
            return;
        }
        // Check magic
        if ehdr[0..4] != [0x7f, b'E', b'L', b'F'] {
            warn!("resolve_vdso_plt: vDSO is not an ELF");
            return;
        }
        ehdr
    };

    // Parse vDSO's PT_DYNAMIC to find .dynsym, .dynstr, .hash
    let e_phoff = u64::from_le_bytes(vdso_vmo_data[32..40].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(vdso_vmo_data[54..56].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(vdso_vmo_data[56..58].try_into().unwrap()) as usize;

    let mut vdso_dynamic_off = 0usize;
    let mut vdso_dynamic_sz = 0usize;
    for i in 0..e_phnum {
        let ph_off = e_phoff + i * e_phentsize;
        let mut phdr = [0u8; 56];
        if vmar.read_memory(vdso_base + ph_off, &mut phdr).is_err() {
            continue;
        }
        let p_type = u32::from_le_bytes(phdr[0..4].try_into().unwrap());
        if p_type == 2 {
            // PT_DYNAMIC
            vdso_dynamic_off = u64::from_le_bytes(phdr[8..16].try_into().unwrap()) as usize;
            vdso_dynamic_sz = u64::from_le_bytes(phdr[32..40].try_into().unwrap()) as usize;
        }
    }
    if vdso_dynamic_off == 0 {
        warn!("resolve_vdso_plt: no PT_DYNAMIC in vDSO");
        return;
    }

    // Read vDSO .dynamic entries
    let mut vdso_symtab = 0usize;
    let mut vdso_strtab = 0usize;
    let mut vdso_hash = 0usize;
    let mut vdso_gnu_hash = 0usize;
    let mut vdso_syment = 24usize;
    for i in (0..vdso_dynamic_sz).step_by(16) {
        let mut dyn_entry = [0u8; 16];
        if vmar
            .read_memory(vdso_base + vdso_dynamic_off + i, &mut dyn_entry)
            .is_err()
        {
            break;
        }
        let tag = i64::from_le_bytes(dyn_entry[0..8].try_into().unwrap());
        let val = u64::from_le_bytes(dyn_entry[8..16].try_into().unwrap()) as usize;
        match tag {
            0 => break,                        // DT_NULL
            4 => vdso_hash = val,              // DT_HASH
            5 => vdso_strtab = val,            // DT_STRTAB
            6 => vdso_symtab = val,            // DT_SYMTAB
            11 => vdso_syment = val,           // DT_SYMENT
            0x6ffffef5 => vdso_gnu_hash = val, // DT_GNU_HASH
            _ => {}
        }
    }
    if vdso_symtab == 0 || vdso_strtab == 0 || (vdso_hash == 0 && vdso_gnu_hash == 0) {
        warn!("resolve_vdso_plt: incomplete vDSO dynamic section");
        return;
    }

    // Determine the number of dynamic symbols.
    let vdso_nchain = if vdso_hash != 0 {
        // SysV hash: nchain field at offset 4
        let mut hash_hdr = [0u8; 8];
        if vmar
            .read_memory(vdso_base + vdso_hash, &mut hash_hdr)
            .is_err()
        {
            return;
        }
        u32::from_le_bytes(hash_hdr[4..8].try_into().unwrap()) as usize
    } else {
        // GNU hash: parse the header to find the highest symbol index.
        // Layout: nbuckets(u32), symoffset(u32), bloom_size(u32), bloom_shift(u32),
        //         bloom[bloom_size] (u64 each), buckets[nbuckets] (u32 each),
        //         chains[] (u32 each, one per symbol starting from symoffset)
        let mut gh_hdr = [0u8; 16];
        if vmar
            .read_memory(vdso_base + vdso_gnu_hash, &mut gh_hdr)
            .is_err()
        {
            warn!("resolve_vdso_plt: cannot read GNU hash header");
            return;
        }
        let nbuckets = u32::from_le_bytes(gh_hdr[0..4].try_into().unwrap()) as usize;
        let symoffset = u32::from_le_bytes(gh_hdr[4..8].try_into().unwrap()) as usize;
        let bloom_size = u32::from_le_bytes(gh_hdr[8..12].try_into().unwrap()) as usize;
        // Buckets start after: header(16) + bloom(bloom_size * 8)
        let buckets_off = vdso_gnu_hash + 16 + bloom_size * 8;

        // Find the maximum bucket value (= maximum first chain index).
        let mut max_chain_idx = 0usize;
        for i in 0..nbuckets {
            let mut bucket = [0u8; 4];
            if vmar
                .read_memory(vdso_base + buckets_off + i * 4, &mut bucket)
                .is_err()
            {
                break;
            }
            let val = u32::from_le_bytes(bucket) as usize;
            if val > max_chain_idx {
                max_chain_idx = val;
            }
        }
        if max_chain_idx == 0 {
            warn!("resolve_vdso_plt: GNU hash has no symbols");
            return;
        }

        // Follow the chain from max_chain_idx until we find the end
        // (a chain entry with bit 0 set marks the last entry in that bucket).
        let chains_off = buckets_off + nbuckets * 4;
        let mut sym_idx = max_chain_idx;
        loop {
            let chain_entry_off = chains_off + (sym_idx - symoffset) * 4;
            let mut entry = [0u8; 4];
            if vmar
                .read_memory(vdso_base + chain_entry_off, &mut entry)
                .is_err()
            {
                break;
            }
            let val = u32::from_le_bytes(entry);
            sym_idx += 1;
            if val & 1 != 0 {
                break; // Last entry in this chain
            }
        }
        sym_idx // Total number of symbols
    };

    // Now parse the target ELF's dynamic section for its relocation
    // sections and symbol tables. Two relocation sections matter:
    // DT_JMPREL/DT_PLTRELSZ (PLT slots — JUMP_SLOT) and
    // DT_REL/DT_RELSZ (.rel.dyn — GLOB_DAT and relatives).
    let elf = match xmas_elf::ElfFile::new(elf_data) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut ld_jmprel = 0u64;
    let mut ld_pltrelsz = 0u64;
    let mut ld_rel = 0u64;
    let mut ld_relsz = 0u64;
    let mut ld_symtab = 0u64;
    let mut ld_strtab = 0u64;
    let mut ld_syment = 24u64;
    let mut ld_relent = 16u64;

    for ph in elf.program_iter() {
        if ph.get_type().unwrap_or(xmas_elf::program::Type::Null)
            == xmas_elf::program::Type::Dynamic
        {
            let dyn_off = ph.offset() as usize;
            let dyn_sz = ph.file_size() as usize;
            for i in (0..dyn_sz).step_by(16) {
                let off = dyn_off + i;
                if off + 16 > elf_data.len() {
                    break;
                }
                let tag = i64::from_le_bytes(elf_data[off..off + 8].try_into().unwrap());
                let val = u64::from_le_bytes(elf_data[off + 8..off + 16].try_into().unwrap());
                match tag {
                    0 => break,
                    2 => ld_pltrelsz = val, // DT_PLTRELSZ
                    5 => ld_strtab = val,   // DT_STRTAB
                    6 => ld_symtab = val,   // DT_SYMTAB
                    11 => ld_syment = val,  // DT_SYMENT
                    17 => ld_rel = val,     // DT_REL
                    18 => ld_relsz = val,   // DT_RELSZ
                    19 => ld_relent = val,  // DT_RELENT
                    23 => ld_jmprel = val,  // DT_JMPREL
                    _ => {}
                }
            }
        }
    }
    // Use DT_RELENT if available, otherwise default to 16 (REL on x86_64).
    if ld_relent == 0 {
        ld_relent = 16;
    }

    // Dynamic-section values (DT_REL, DT_JMPREL, DT_SYMTAB, DT_STRTAB)
    // are *virtual addresses*, but `elf_data` is the file image. Most
    // Fuchsia binaries link with vaddr == file offset for these
    // tables, but that is not guaranteed by the ELF ABI: translate
    // through the file-backed PT_LOAD segments (falling back to the
    // identity when the vaddr is not found, preserving the previous
    // behavior).
    let vaddr_to_offset = |vaddr: u64| -> Option<usize> {
        for ph in elf.program_iter() {
            if ph.get_type().unwrap_or(xmas_elf::program::Type::Null)
                == xmas_elf::program::Type::Load
                && ph.file_size() > 0
            {
                let va = ph.virtual_addr();
                let sz = ph.file_size();
                if vaddr >= va && vaddr - va < sz {
                    return Some(ph.offset() as usize + (vaddr - va) as usize);
                }
            }
        }
        None
    };
    let translate = |vaddr: u64| -> usize { vaddr_to_offset(vaddr).unwrap_or(vaddr as usize) };
    let ld_jmprel_off = translate(ld_jmprel);
    let ld_rel_off = translate(ld_rel);
    let ld_symtab_off = translate(ld_symtab);
    let ld_strtab_off = translate(ld_strtab);

    // Resolve one relocation section: entries at the translated file
    // offset of `rel_sz` bytes. Both JUMP_SLOT and GLOB_DAT patch a
    // GOT entry at r_offset with the resolved vDSO symbol address.
    //
    // Relocation types: x86_64 GLOB_DAT=6 / JUMP_SLOT=7;
    // aarch64 GLOB_DAT=1025 / JUMP_SLOT=1026.
    let mut resolved = 0usize;
    let mut resolve_section = |rel_off: usize, rel_sz: u64| {
        if rel_off == 0 || rel_sz == 0 {
            return;
        }
        let count = rel_sz as usize / ld_relent as usize;
        for i in 0..count {
            let off = rel_off + i * ld_relent as usize;
            if off + 16 > elf_data.len() {
                break;
            }
            let r_offset = u64::from_le_bytes(elf_data[off..off + 8].try_into().unwrap());
            let r_info = u64::from_le_bytes(elf_data[off + 8..off + 16].try_into().unwrap());
            let r_type = (r_info & 0xFFFF_FFFF) as u32;
            let r_sym = (r_info >> 32) as usize;

            match r_type {
                7 | 1026 => {} // JUMP_SLOT (x86_64 | aarch64)
                6 | 1025 => {} // GLOB_DAT (x86_64 | aarch64)
                _ => continue,
            }

            // Look up the symbol name in the target ELF's .dynstr
            // (using the file-offset-translated tables).
            let sym_off = ld_symtab_off + r_sym * ld_syment as usize;
            if sym_off + 24 > elf_data.len() {
                continue;
            }
            let st_name = u32::from_le_bytes(elf_data[sym_off..sym_off + 4].try_into().unwrap());
            let st_info = elf_data[sym_off + 4];
            let binding = st_info >> 4; // STB_LOCAL=0, GLOBAL=1, WEAK=2
            let st_shndx =
                u16::from_le_bytes(elf_data[sym_off + 6..sym_off + 8].try_into().unwrap());
            let st_value =
                u64::from_le_bytes(elf_data[sym_off + 8..sym_off + 16].try_into().unwrap());
            let name_off = ld_strtab_off + st_name as usize;
            if name_off >= elf_data.len() {
                continue;
            }
            let name_end = elf_data[name_off..]
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(0);
            let sym_name =
                core::str::from_utf8(&elf_data[name_off..name_off + name_end]).unwrap_or("");

            let got_addr = elf_base + r_offset as usize;

            // Weak UNDEFINED symbols (scudo allocator hooks, ZSTD
            // trace hooks, ...) are absent from the vDSO by design:
            // the ELF ABI resolves them to NULL and callers test
            // before calling. Only st_shndx == SHN_UNDEF (0) counts —
            // a weak symbol with a definition keeps it (load bias +
            // st_value). Leaving link-time placeholders in the GOT
            // makes null-checks see garbage (observed: zstd's bootfs
            // decompression calling ZSTD_trace_decompress_begin
            // through a stale PLT slot).
            let is_vdso_symbol = sym_name.starts_with("_zx_") || sym_name.starts_with("zx_");
            if !is_vdso_symbol {
                if binding == 2 && st_shndx == 0 {
                    // Weak undefined: resolve to NULL.
                    if vmar.write_memory(got_addr, &0u64.to_ne_bytes()).is_ok() {
                        resolved += 1;
                    }
                } else if binding == 2 {
                    // Weak but defined in this binary: load bias + st_value.
                    let addr = elf_base + st_value as usize;
                    if vmar.write_memory(got_addr, &addr.to_ne_bytes()).is_ok() {
                        resolved += 1;
                    }
                }
                continue;
            }

            // Look up this symbol in the vDSO
            if let Some(vdso_addr) = lookup_vdso_symbol(
                vmar,
                vdso_base,
                vdso_symtab,
                vdso_strtab,
                vdso_syment,
                vdso_nchain,
                sym_name,
            ) {
                if vmar
                    .write_memory(got_addr, &vdso_addr.to_ne_bytes())
                    .is_ok()
                {
                    resolved += 1;
                }
            } else if binding == 2 {
                // Weak symbol not provided by the vDSO: NULL.
                if vmar.write_memory(got_addr, &0u64.to_ne_bytes()).is_ok() {
                    resolved += 1;
                }
            } else {
                warn!(
                    "resolve_vdso_symbols: unresolved non-weak symbol {}",
                    sym_name
                );
            }
        }
    };
    resolve_section(ld_rel_off, ld_relsz);
    resolve_section(ld_jmprel_off, ld_pltrelsz);
    info!(
        "resolve_vdso_symbols: resolved {} vDSO GOT entries (GLOB_DAT + JUMP_SLOT)",
        resolved
    );
}

/// Look up a symbol by name in the vDSO's .dynsym.
fn lookup_vdso_symbol(
    vmar: &Arc<VmAddressRegion>,
    vdso_base: usize,
    symtab: usize,
    strtab: usize,
    syment: usize,
    nsyms: usize,
    name: &str,
) -> Option<usize> {
    for i in 1..nsyms {
        let sym_off = vdso_base + symtab + i * syment;
        let mut sym = [0u8; 24];
        if vmar.read_memory(sym_off, &mut sym).is_err() {
            continue;
        }
        let st_name = u32::from_le_bytes(sym[0..4].try_into().unwrap()) as usize;
        let st_value = u64::from_le_bytes(sym[8..16].try_into().unwrap()) as usize;

        // Read the name from .dynstr
        let name_addr = vdso_base + strtab + st_name;
        let mut name_buf = [0u8; 64];
        if vmar.read_memory(name_addr, &mut name_buf).is_err() {
            continue;
        }
        let name_end = name_buf.iter().position(|&b| b == 0).unwrap_or(64);
        let sym_name = core::str::from_utf8(&name_buf[..name_end]).unwrap_or("");

        if sym_name == name {
            return Some(vdso_base + st_value);
        }
    }
    None
}
