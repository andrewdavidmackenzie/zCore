//! Build script for zircon-loader.
//!
//! When `VDSO_BIN` is not set, generates a minimal ELF shared library
//! containing syscall trampolines from `zx-syscall-numbers.h`. This
//! avoids requiring a C cross-compiler — the ELF is constructed as
//! raw bytes in pure Rust.

use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-env-changed=USERSTART_ELF");
    if std::env::var("USERSTART_ELF").is_err() {
        let stub = out.join("empty_userstart.elf");
        std::fs::write(stub.as_path(), b"").unwrap();
        println!("cargo:rustc-env=USERSTART_ELF={}", stub.display());
    }

    println!("cargo:rerun-if-env-changed=VDSO_BIN");
    if let Ok(vdso_path) = std::env::var("VDSO_BIN") {
        println!("cargo:rerun-if-changed={}", vdso_path);
    } else {
        // Generate vDSO ELF from syscall numbers header
        let header_path = PathBuf::from("../zircon-syscall/src/zx-syscall-numbers.h");
        println!("cargo:rerun-if-changed={}", header_path.display());

        let target = std::env::var("TARGET").unwrap_or_default();
        let vdso_path = out.join("vdso.so");

        if target.contains("x86_64") {
            generate_vdso_elf(&header_path, &vdso_path, Arch::X86_64);
        } else if target.contains("aarch64") {
            generate_vdso_elf(&header_path, &vdso_path, Arch::Aarch64);
        } else if target.contains("riscv64") {
            generate_vdso_elf(&header_path, &vdso_path, Arch::Riscv64);
        } else {
            // Unknown arch or host build — empty stub
            std::fs::write(&vdso_path, b"").unwrap();
        }
        println!("cargo:rustc-env=VDSO_BIN={}", vdso_path.display());
    }
}

#[derive(Clone, Copy)]
enum Arch {
    X86_64,
    Aarch64,
    Riscv64,
}

/// Parse syscall names and numbers from zx-syscall-numbers.h.
fn parse_syscalls(header: &std::path::Path) -> Vec<(String, u32)> {
    let data = std::fs::read_to_string(header).expect("cannot read zx-syscall-numbers.h");
    let mut syscalls = Vec::new();
    for line in data.lines() {
        if !line.starts_with("#define ZX_SYS_") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let name = &parts[1][7..]; // strip "ZX_SYS_"
        if name == "COUNT" {
            continue;
        }
        let num: u32 = match parts[2].parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        syscalls.push((name.to_string(), num));
    }
    syscalls
}

/// Generate trampoline machine code for one syscall.
fn trampoline_code(arch: Arch, num: u32) -> Vec<u8> {
    match arch {
        Arch::X86_64 => {
            // mov %rcx, %r10; mov $num, %eax; syscall; ret
            // The C calling convention puts arg4 in rcx, but the
            // `syscall` instruction clobbers rcx (saves RIP there).
            // We save rcx to r10 before syscall.
            let mut code = vec![
                0x49, 0x89, 0xca, // mov %rcx, %r10
                0xb8, // mov imm32, %eax
            ];
            code.extend_from_slice(&num.to_le_bytes());
            code.extend_from_slice(&[0x0f, 0x05]); // syscall
            code.push(0xc3); // ret
            code
        }
        Arch::Aarch64 => {
            // mov x16, #num; svc #0; ret
            // movz x16, #num => 0xd2800010 | (num << 5)
            let movz = 0xd280_0010u32 | ((num & 0xFFFF) << 5);
            let svc = 0xd400_0001u32; // svc #0
            let ret = 0xd65f_03c0u32; // ret
            let mut code = Vec::new();
            code.extend_from_slice(&movz.to_le_bytes());
            code.extend_from_slice(&svc.to_le_bytes());
            code.extend_from_slice(&ret.to_le_bytes());
            code
        }
        Arch::Riscv64 => {
            // li a7, num; ecall; ret
            let mut code = Vec::new();
            if num < 2048 {
                // addi a7, x0, num
                let addi = 0x0000_0893u32 | (num << 20);
                code.extend_from_slice(&addi.to_le_bytes());
            } else {
                let hi = (num + 0x800) >> 12;
                let lo = (num as i32) - ((hi << 12) as i32);
                let lui = 0x0000_08b7u32 | (hi << 12);
                code.extend_from_slice(&lui.to_le_bytes());
                let addi = 0x0008_8893u32 | (((lo as u32) & 0xFFF) << 20);
                code.extend_from_slice(&addi.to_le_bytes());
            }
            code.extend_from_slice(&0x0000_0073u32.to_le_bytes()); // ecall
            code.extend_from_slice(&0x0000_8067u32.to_le_bytes()); // ret
            code
        }
    }
}

enum VdsoFunc {
    Syscall(&'static str),
    ReturnConst(u64),
}

/// Generate code that returns a constant value in rax/x0/a0.
fn return_const_code(arch: Arch, val: u64) -> Vec<u8> {
    match arch {
        Arch::X86_64 => {
            // mov $val, %rax; ret
            let mut code = vec![0x48, 0xb8]; // movabs imm64, %rax
            code.extend_from_slice(&val.to_le_bytes());
            code.push(0xc3); // ret
            code
        }
        Arch::Aarch64 => {
            // movz x0, #lo16; movk x0, #hi16, lsl #16; ret
            let lo = (val & 0xFFFF) as u32;
            let hi = ((val >> 16) & 0xFFFF) as u32;
            let movz = 0xd280_0000u32 | (lo << 5); // movz x0, #lo
            let ret = 0xd65f_03c0u32;
            let mut code = Vec::new();
            code.extend_from_slice(&movz.to_le_bytes());
            if hi != 0 {
                let movk = 0xf2a0_0000u32 | (hi << 5); // movk x0, #hi, lsl #16
                code.extend_from_slice(&movk.to_le_bytes());
            }
            code.extend_from_slice(&ret.to_le_bytes());
            code
        }
        Arch::Riscv64 => {
            // li a0, val; ret
            let mut code = Vec::new();
            if val < 2048 {
                let addi = 0x0000_0513u32 | ((val as u32) << 20);
                code.extend_from_slice(&addi.to_le_bytes());
            } else {
                // lui + addi for larger values
                let hi = ((val as u32) + 0x800) >> 12;
                let lo = (val as i32) - ((hi << 12) as i32);
                let lui = 0x0000_0537u32 | (hi << 12);
                code.extend_from_slice(&lui.to_le_bytes());
                let addi = 0x0005_0513u32 | (((lo as u32) & 0xFFF) << 20);
                code.extend_from_slice(&addi.to_le_bytes());
            }
            code.extend_from_slice(&0x0000_8067u32.to_le_bytes()); // ret
            code
        }
    }
}

/// ELF constants
const ET_DYN: u16 = 3;
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PF_R: u32 = 4;
const PF_X: u32 = 1;
const DT_HASH: i64 = 4;
const DT_STRTAB: i64 = 5;
const DT_SYMTAB: i64 = 6;
const DT_STRSZ: i64 = 10;
const DT_SYMENT: i64 = 11;
const DT_SONAME: i64 = 14;
const DT_NULL: i64 = 0;
const STB_GLOBAL: u8 = 1;
const STT_FUNC: u8 = 2;

/// Generate a minimal ELF shared library with syscall trampolines.
fn generate_vdso_elf(header: &std::path::Path, output: &std::path::Path, arch: Arch) {
    let syscalls = parse_syscalls(header);
    if syscalls.is_empty() {
        std::fs::write(output, b"").unwrap();
        return;
    }

    let (ei_class, ei_data, e_machine, ehdr_size, phdr_size, sym_size) = match arch {
        Arch::X86_64 => (2u8, 1u8, 62u16, 64usize, 56usize, 24usize),
        Arch::Aarch64 => (2, 1, 183, 64, 56, 24),
        Arch::Riscv64 => (2, 1, 243, 64, 56, 24),
    };

    // Build symbol names: for each syscall, _zx_foo and zx_foo
    let mut dynstr = vec![0u8]; // start with null byte
    let soname_off = dynstr.len();
    dynstr.extend_from_slice(b"libzircon.so\0");

    struct SymEntry {
        name_off: u32,
        code_offset: usize,
    }

    let mut text = Vec::new();
    let mut sym_entries = Vec::new();

    for (name, num) in &syscalls {
        let code = trampoline_code(arch, *num);
        let code_off = text.len();

        let name1 = format!("_zx_{name}\0");
        let name1_off = dynstr.len();
        dynstr.extend_from_slice(name1.as_bytes());
        sym_entries.push(SymEntry {
            name_off: name1_off as u32,
            code_offset: code_off,
        });

        let name2 = format!("zx_{name}\0");
        let name2_off = dynstr.len();
        dynstr.extend_from_slice(name2.as_bytes());
        sym_entries.push(SymEntry {
            name_off: name2_off as u32,
            code_offset: code_off,
        });

        text.extend_from_slice(&code);
    }

    // Add vDSO-only wrapper functions.
    // These are userspace functions in Fuchsia's vDSO that either:
    // - Return constants (page_size, num_cpus, physmem)
    // - Wrap kernel syscalls (clock_get_monotonic, deadline_after)
    // - Provide convenience APIs (channel_call, cprng_draw)
    //
    // For now, map them to their underlying syscalls or return constants.
    // This is sufficient for ld.so.1 to bootstrap.
    let vdso_wrappers: &[(&str, VdsoFunc)] = &[
        // Map to underlying kernel syscalls
        (
            "clock_get_monotonic",
            VdsoFunc::Syscall("clock_get_monotonic_via_kernel"),
        ),
        (
            "clock_get_boot",
            VdsoFunc::Syscall("clock_get_boot_via_kernel"),
        ),
        ("ticks_get", VdsoFunc::Syscall("ticks_get_via_kernel")),
        (
            "deadline_after",
            VdsoFunc::Syscall("clock_get_monotonic_via_kernel"),
        ), // approximate
        ("channel_call", VdsoFunc::Syscall("channel_call_noretry")),
        ("cprng_draw", VdsoFunc::Syscall("cprng_draw_once")),
        // Return constants
        ("system_get_page_size", VdsoFunc::ReturnConst(4096)),
        ("system_get_num_cpus", VdsoFunc::ReturnConst(1)),
        ("system_get_physmem", VdsoFunc::ReturnConst(0)),
        ("ticks_per_second", VdsoFunc::ReturnConst(1_000_000_000)),
        // thread_self must return non-zero for mutex owner tracking.
        // In Fuchsia this reads TLS, but ld.so.1 calls it before TLS
        // is set up. Any non-zero value works as a mutex owner ID.
        // After SetStartHandles, libc's own _zx_thread_self replaces this.
        ("thread_self", VdsoFunc::ReturnConst(0xFFFF_0001)),
        // vmar_root_self and process_self are libc globals set by
        // ld.so.1 during processargs handling. NOT vDSO functions.
        // status_get_string: returns a pointer to a status string.
        // Stub returns NULL — callers must handle NULL. This is needed
        // because ld.so.1 calls it via PLT before lazy binding works.
        ("status_get_string", VdsoFunc::ReturnConst(0)),
        ("utc_reference_swap", VdsoFunc::ReturnConst(0)),
        ("utc_reference_get", VdsoFunc::ReturnConst(0)),
        ("system_get_dcache_line_size", VdsoFunc::ReturnConst(64)),
        ("system_get_features", VdsoFunc::ReturnConst(0)),
        // Newer Fuchsia symbols needed by core-tests-standalone
        (
            "channel_call_etc",
            VdsoFunc::Syscall("channel_call_etc_noretry"),
        ),
        ("handle_check_valid", VdsoFunc::ReturnConst(0)), // stub: always valid
        ("system_get_version_string", VdsoFunc::ReturnConst(0)), // stub: null
        ("exception_get_string", VdsoFunc::ReturnConst(0)), // stub: null
        (
            "ticks_get_boot",
            VdsoFunc::Syscall("clock_get_boot_via_kernel"),
        ),
    ];

    for (name, func) in vdso_wrappers {
        let code = match func {
            VdsoFunc::Syscall(target) => {
                // Find the syscall number for the target
                if let Some((_, num)) = syscalls.iter().find(|(n, _)| n == target) {
                    trampoline_code(arch, *num)
                } else {
                    return_const_code(arch, 0) // fallback
                }
            }
            VdsoFunc::ReturnConst(val) => return_const_code(arch, *val),
        };
        let code_off = text.len();

        // Check if symbol already exists (some might overlap with syscalls)
        let name1 = format!("_zx_{name}\0");
        let already_exists = sym_entries.iter().any(|e| {
            let existing = &dynstr[e.name_off as usize..];
            let end = existing.iter().position(|&b| b == 0).unwrap_or(0);
            &existing[..end] == name1[..name1.len() - 1].as_bytes()
        });
        if already_exists {
            continue;
        }

        let name1_off = dynstr.len();
        dynstr.extend_from_slice(name1.as_bytes());
        sym_entries.push(SymEntry {
            name_off: name1_off as u32,
            code_offset: code_off,
        });

        let name2 = format!("zx_{name}\0");
        let name2_off = dynstr.len();
        dynstr.extend_from_slice(name2.as_bytes());
        sym_entries.push(SymEntry {
            name_off: name2_off as u32,
            code_offset: code_off,
        });

        text.extend_from_slice(&code);
    }

    let nsyms = sym_entries.len() + 1; // +1 for null symbol

    // Layout: [ELF header] [Phdrs] [.hash] [.dynsym] [.dynstr] [.text] [.dynamic]
    let num_phdrs = 3u16;
    let headers_end = ehdr_size + phdr_size * num_phdrs as usize;

    let hash_off = headers_end;
    let nbuckets = (nsyms * 4 / 3).max(1);
    let hash_size = (2 + nbuckets + nsyms) * 4;

    let dynsym_off = hash_off + hash_size;
    let dynsym_size = nsyms * sym_size;

    let dynstr_off = dynsym_off + dynsym_size;
    let dynstr_size = dynstr.len();

    let text_off = (dynstr_off + dynstr_size + 15) & !15; // align 16
    let text_size = text.len();

    let dynamic_off = (text_off + text_size + 7) & !7; // align 8
    let num_dyn_entries = 7;
    let dynamic_size = num_dyn_entries * 16;

    let file_size = dynamic_off + dynamic_size;
    let text_vaddr = text_off;

    let mut elf = vec![0u8; file_size];

    // ELF header
    elf[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
    elf[4] = ei_class;
    elf[5] = ei_data;
    elf[6] = 1; // EV_CURRENT
    elf[16..18].copy_from_slice(&ET_DYN.to_le_bytes());
    elf[18..20].copy_from_slice(&e_machine.to_le_bytes());
    elf[20..24].copy_from_slice(&1u32.to_le_bytes());
    elf[32..40].copy_from_slice(&(ehdr_size as u64).to_le_bytes());
    elf[52..54].copy_from_slice(&(ehdr_size as u16).to_le_bytes());
    elf[54..56].copy_from_slice(&(phdr_size as u16).to_le_bytes());
    elf[56..58].copy_from_slice(&num_phdrs.to_le_bytes());

    // Program headers
    let mut ph = ehdr_size;
    let load0_size = text_off + text_size;
    write_phdr(
        &mut elf,
        ph,
        PT_LOAD,
        PF_R | PF_X,
        0,
        0,
        load0_size as u64,
        load0_size as u64,
        0x1000,
    );
    ph += phdr_size;
    write_phdr(
        &mut elf,
        ph,
        PT_LOAD,
        PF_R,
        dynamic_off as u64,
        dynamic_off as u64,
        dynamic_size as u64,
        dynamic_size as u64,
        0x1000,
    );
    ph += phdr_size;
    write_phdr(
        &mut elf,
        ph,
        PT_DYNAMIC,
        PF_R,
        dynamic_off as u64,
        dynamic_off as u64,
        dynamic_size as u64,
        dynamic_size as u64,
        8,
    );

    // .hash (SysV)
    let mut hash_data = vec![0u32; 2 + nbuckets + nsyms];
    hash_data[0] = nbuckets as u32;
    hash_data[1] = nsyms as u32;
    for (i, sym) in sym_entries.iter().enumerate() {
        let sym_idx = i + 1;
        let name_bytes = &dynstr[sym.name_off as usize..];
        let name_end = name_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(name_bytes.len());
        let h = elf_hash(&name_bytes[..name_end]);
        let bucket = (h as usize) % nbuckets;
        hash_data[2 + nbuckets + sym_idx] = hash_data[2 + bucket];
        hash_data[2 + bucket] = sym_idx as u32;
    }
    for (i, val) in hash_data.iter().enumerate() {
        let off = hash_off + i * 4;
        elf[off..off + 4].copy_from_slice(&val.to_le_bytes());
    }

    // .dynsym
    for (i, sym) in sym_entries.iter().enumerate() {
        let off = dynsym_off + (i + 1) * sym_size;
        let sym_vaddr = (text_vaddr + sym.code_offset) as u64;
        elf[off..off + 4].copy_from_slice(&sym.name_off.to_le_bytes());
        elf[off + 4] = (STB_GLOBAL << 4) | STT_FUNC;
        elf[off + 6..off + 8].copy_from_slice(&1u16.to_le_bytes()); // st_shndx
        elf[off + 8..off + 16].copy_from_slice(&sym_vaddr.to_le_bytes());
    }

    // .dynstr
    elf[dynstr_off..dynstr_off + dynstr_size].copy_from_slice(&dynstr);

    // .text
    elf[text_off..text_off + text_size].copy_from_slice(&text);

    // .dynamic
    let mut dyn_off = dynamic_off;
    write_dyn(&mut elf, &mut dyn_off, DT_SONAME, soname_off as u64);
    write_dyn(&mut elf, &mut dyn_off, DT_HASH, hash_off as u64);
    write_dyn(&mut elf, &mut dyn_off, DT_SYMTAB, dynsym_off as u64);
    write_dyn(&mut elf, &mut dyn_off, DT_STRTAB, dynstr_off as u64);
    write_dyn(&mut elf, &mut dyn_off, DT_STRSZ, dynstr_size as u64);
    write_dyn(&mut elf, &mut dyn_off, DT_SYMENT, sym_size as u64);
    write_dyn(&mut elf, &mut dyn_off, DT_NULL, 0);

    std::fs::write(output, &elf).unwrap();
    eprintln!(
        "vDSO: generated {}-byte ELF with {} symbols ({} syscalls)",
        elf.len(),
        nsyms - 1,
        syscalls.len()
    );
}

fn write_phdr(
    elf: &mut [u8],
    off: usize,
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
) {
    elf[off..off + 4].copy_from_slice(&p_type.to_le_bytes());
    elf[off + 4..off + 8].copy_from_slice(&p_flags.to_le_bytes());
    elf[off + 8..off + 16].copy_from_slice(&p_offset.to_le_bytes());
    elf[off + 16..off + 24].copy_from_slice(&p_vaddr.to_le_bytes());
    elf[off + 24..off + 32].copy_from_slice(&p_vaddr.to_le_bytes());
    elf[off + 32..off + 40].copy_from_slice(&p_filesz.to_le_bytes());
    elf[off + 40..off + 48].copy_from_slice(&p_memsz.to_le_bytes());
    elf[off + 48..off + 56].copy_from_slice(&p_align.to_le_bytes());
}

fn write_dyn(elf: &mut [u8], off: &mut usize, tag: i64, val: u64) {
    elf[*off..*off + 8].copy_from_slice(&tag.to_le_bytes());
    elf[*off + 8..*off + 16].copy_from_slice(&val.to_le_bytes());
    *off += 16;
}

/// SysV ELF hash function.
fn elf_hash(name: &[u8]) -> u32 {
    let mut h: u32 = 0;
    for &b in name {
        h = (h << 4).wrapping_add(b as u32);
        let g = h & 0xf000_0000;
        if g != 0 {
            h ^= g >> 24;
        }
        h &= !g;
    }
    h
}
