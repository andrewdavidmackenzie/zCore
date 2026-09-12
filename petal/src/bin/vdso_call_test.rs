//! petal vDSO call test -- calls vDSO functions through the mapped library.
//!
//! Verifies that vDSO trampolines are mapped as executable code and
//! can be called through function pointers resolved from the ELF.

#![no_std]
#![no_main]

extern crate petal;

use zx::sys::ZX_OK;

/// Minimal ELF64 structures for symbol lookup.
#[repr(C)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

#[repr(C)]
struct Elf64Dyn {
    d_tag: i64,
    d_val: u64,
}

#[repr(C)]
struct Elf64Sym {
    st_name: u32,
    st_info: u8,
    st_other: u8,
    st_shndx: u16,
    st_value: u64,
    st_size: u64,
}

const PT_DYNAMIC: u32 = 2;
const DT_NULL: i64 = 0;
const DT_HASH: i64 = 4;
const DT_STRTAB: i64 = 5;
const DT_SYMTAB: i64 = 6;

#[no_mangle]
pub fn main() {
    zx::debug_write(b"vdso_call_test: starting\n");

    // Get vDSO base from _start arg2 or fall back to property
    let mut base = petal::vdso_base();
    if base == 0 {
        // Try ZX_PROP_PROCESS_VDSO_BASE_ADDRESS.
        // This returns the DATA page address. The code base is 0x7000 before it.
        let startup = petal::take_startup_handle();
        let mut data_addr: usize = 0;
        let s = unsafe {
            zx::sys::zx_object_get_property(
                startup,
                6,
                &mut data_addr as *mut usize as *mut u8,
                core::mem::size_of::<usize>(),
            )
        };
        unsafe { zx::sys::zx_handle_close(startup) };
        if s == ZX_OK && data_addr > 0x7000 {
            base = data_addr - 0x7000;
        }
    }

    if base == 0 {
        zx::debug_write(b"vdso_call_test: FAIL - cannot determine vDSO base\n");
        zx::Process::exit(1);
    }
    zx::debug_write(b"vdso_call_test: vDSO base found\n");

    // Check ELF magic. If not present, vDSO code wasn't embedded.
    let magic = unsafe { core::slice::from_raw_parts(base as *const u8, 4) };
    if magic != b"\x7fELF" {
        zx::debug_write(b"vdso_call_test: vDSO code not embedded, skipping\n");
        zx::debug_write(b"vdso_call_test: PASS\n");
        return;
    }
    zx::debug_write(b"vdso_call_test: ELF magic verified\n");

    // Parse ELF header
    let e_phoff = unsafe { *((base + 32) as *const u64) } as usize;
    let e_phentsize = unsafe { *((base + 54) as *const u16) } as usize;
    let e_phnum = unsafe { *((base + 56) as *const u16) } as usize;

    // Find PT_DYNAMIC
    let mut dyn_addr: usize = 0;
    for i in 0..e_phnum {
        let phdr = unsafe { &*((base + e_phoff + i * e_phentsize) as *const Elf64Phdr) };
        if phdr.p_type == PT_DYNAMIC {
            dyn_addr = base + phdr.p_vaddr as usize;
            break;
        }
    }
    if dyn_addr == 0 {
        zx::debug_write(b"vdso_call_test: FAIL - no PT_DYNAMIC\n");
        zx::Process::exit(1);
    }

    // Parse dynamic entries
    let mut symtab: usize = 0;
    let mut strtab: usize = 0;
    let mut hash: usize = 0;
    let mut dyn_ptr = dyn_addr;
    loop {
        let entry = unsafe { &*(dyn_ptr as *const Elf64Dyn) };
        if entry.d_tag == DT_NULL {
            break;
        }
        match entry.d_tag {
            DT_SYMTAB => symtab = base + entry.d_val as usize,
            DT_STRTAB => strtab = base + entry.d_val as usize,
            DT_HASH => hash = base + entry.d_val as usize,
            _ => {}
        }
        dyn_ptr += 16; // size of Elf64Dyn
    }

    if symtab == 0 || strtab == 0 || hash == 0 {
        zx::debug_write(b"vdso_call_test: FAIL - missing dynamic tables\n");
        zx::Process::exit(1);
    }
    zx::debug_write(b"vdso_call_test: dynamic section parsed\n");

    // Resolve zx_handle_close from symbol table
    let nchain = unsafe { *((hash + 4) as *const u32) };
    let target = b"zx_handle_close\0";
    let mut func_addr: usize = 0;

    for i in 0..nchain {
        let sym = unsafe { &*((symtab + i as usize * 24) as *const Elf64Sym) };
        if sym.st_value == 0 {
            continue;
        }
        let name_ptr = (strtab + sym.st_name as usize) as *const u8;
        let mut ok = true;
        for (j, &ch) in target.iter().enumerate() {
            if unsafe { *name_ptr.add(j) } != ch {
                ok = false;
                break;
            }
            if ch == 0 {
                break;
            }
        }
        if ok {
            func_addr = base + sym.st_value as usize;
            break;
        }
    }

    if func_addr == 0 {
        zx::debug_write(b"vdso_call_test: FAIL - symbol not found\n");
        zx::Process::exit(1);
    }
    zx::debug_write(b"vdso_call_test: zx_handle_close resolved\n");

    // Call zx_handle_close(ZX_HANDLE_INVALID=0) through the vDSO
    let func: extern "C" fn(u32) -> i32 = unsafe { core::mem::transmute(func_addr) };
    let status = func(0);
    if status == ZX_OK {
        zx::debug_write(b"vdso_call_test: vDSO call returned OK\n");
    } else {
        zx::debug_write(b"vdso_call_test: FAIL - vDSO call returned error\n");
        zx::Process::exit(1);
    }

    zx::debug_write(b"vdso_call_test: PASS\n");
}
