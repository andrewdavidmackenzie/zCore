//! userstart: The first userspace process in zCore's Zircon mode.
//!
//! This is zCore's equivalent of Fuchsia's `userboot`. It:
//! 1. Receives bootstrap handles from the kernel via a channel
//! 2. Reads the ZBI VMO to find the bootfs
//! 3. Finds the init program in the bootfs
//! 4. Creates a new process, maps the program code, and starts it
//!
//! The init program (e.g., petal's hello) receives its own channel
//! with the bootstrap handles forwarded from the kernel.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use zircon_abi::consts::*;
use zircon_abi::errors::*;
use zircon_abi::syscall::*;
use zircon_abi::zbi;

// Bootstrap handle indices (must match kernel's K_* constants)
const K_PROC_SELF: usize = 0;
const K_VMARROOT_SELF: usize = 1;
const K_ROOTJOB: usize = 2;
const K_ZBI: usize = 4;
const K_FIRSTVDSO: usize = 5;
const K_HANDLECOUNT: usize = 15;

// vDSO data page offset (matches Fuchsia's ELF layout)
const VDSO_DATA_OFFSET: usize = 0x7000;

// Page size (4 KiB)
const PAGE_SIZE: usize = 4096;

/// Debug print helper.
fn debug_print(msg: &[u8]) {
    debug_write(msg);
}

/// Check a syscall result, panic on error.
fn check(name: &str, status: ZxStatus) {
    if status != ZX_OK {
        debug_print(b"userstart: syscall failed: ");
        debug_print(name.as_bytes());
        debug_print(b"\n");
        process_exit(1);
    }
}

/// Entry point -- receives the bootstrap channel handle from the kernel.
///
/// The kernel passes the channel handle as the first argument (in x0/rdi).
/// The second argument (x1/rsi) is 0.
#[no_mangle]
pub extern "C" fn _start(bootstrap_handle: HandleValue, _arg2: usize) -> ! {
    debug_print(b"userstart: starting\n");

    // Step 1: Read bootstrap handles from the channel
    let mut data_buf = [0u8; 1024]; // for cmdline
    let mut handles = [ZX_HANDLE_INVALID; K_HANDLECOUNT];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;

    check("channel_read", unsafe {
        zx_channel_read(
            bootstrap_handle,
            0, // options
            data_buf.as_mut_ptr(),
            handles.as_mut_ptr(),
            data_buf.len() as u32,
            K_HANDLECOUNT as u32,
            &mut actual_bytes,
            &mut actual_handles,
        )
    });

    debug_print(b"userstart: received bootstrap handles\n");

    // Close the bootstrap channel -- we've read all the handles
    unsafe { zx_handle_close(bootstrap_handle) };

    let _proc_self = handles[K_PROC_SELF];
    let vmar_self = handles[K_VMARROOT_SELF];
    let root_job = handles[K_ROOTJOB];
    let zbi_vmo = handles[K_ZBI];
    let vdso_vmo = handles[K_FIRSTVDSO];

    // Step 2: Read the ZBI VMO to find the init program
    let mut zbi_size: usize = 0;
    check("vmo_get_size", unsafe {
        zx_vmo_get_size(zbi_vmo, &mut zbi_size)
    });

    if zbi_size == 0 || zbi_size > 16 * 1024 * 1024 {
        debug_print(b"userstart: ZBI size invalid\n");
        unsafe { zx_process_exit(1) };
    }

    // Map the ZBI VMO into our address space to read it
    let mut zbi_addr: usize = 0;
    check("vmar_map(zbi)", unsafe {
        zx_vmar_map(
            vmar_self,
            ZX_VM_PERM_READ,
            0, // vmar_offset (anywhere)
            zbi_vmo,
            0, // vmo_offset
            zbi_size,
            &mut zbi_addr,
        )
    });

    let zbi_data = unsafe { core::slice::from_raw_parts(zbi_addr as *const u8, zbi_size) };

    // Step 3: Find the init program in the bootfs
    let (name, program_data) = match zbi::find_first_bootfs_entry(zbi_data) {
        Some(entry) => entry,
        None => {
            debug_print(b"userstart: no program found in ZBI bootfs\n");
            unsafe { zx_process_exit(1) };
        }
    };

    debug_print(b"userstart: loading '");
    debug_print(name);
    debug_print(b"'\n");

    // Step 4: Create a new process for the init program
    let proc_name = b"init";
    let mut init_proc: HandleValue = ZX_HANDLE_INVALID;
    let mut init_vmar: HandleValue = ZX_HANDLE_INVALID;
    check("process_create", unsafe {
        zx_process_create(
            root_job,
            proc_name.as_ptr(),
            proc_name.len(),
            0, // options
            &mut init_proc,
            &mut init_vmar,
        )
    });

    // Step 5: Create a thread in the new process
    let thread_name = b"init-main";
    let mut init_thread: HandleValue = ZX_HANDLE_INVALID;
    check("thread_create", unsafe {
        zx_thread_create(
            init_proc,
            thread_name.as_ptr(),
            thread_name.len(),
            0, // options
            &mut init_thread,
        )
    });

    // Step 6: Load program as ELF, mapping each PT_LOAD segment
    // with correct permissions (RX for code, RW for data).
    let (entry_addr, map_end) = load_elf(program_data, init_vmar);

    // Step 7: Create a stack for the init program
    let stack_pages = 8;
    let stack_size = stack_pages * PAGE_SIZE;
    let mut stack_vmo: HandleValue = ZX_HANDLE_INVALID;
    check("vmo_create(stack)", unsafe {
        zx_vmo_create(stack_size as u64, 0, &mut stack_vmo)
    });

    // Map stack above the loaded segments
    let stack_offset = map_end + PAGE_SIZE;
    let mut stack_base: usize = 0;
    check("vmar_map(stack)", unsafe {
        zx_vmar_map(
            init_vmar,
            ZX_VM_PERM_READ | ZX_VM_PERM_WRITE | ZX_VM_SPECIFIC,
            stack_offset,
            stack_vmo,
            0,
            stack_size,
            &mut stack_base,
        )
    });

    let stack_top = stack_base + stack_size;

    // Step 7b: Map vDSO into init process at a high address
    // to avoid interfering with code/stack regions.
    // Map code pages (0-6) as RX and data page (7) as R.
    let vdso_base_addr = stack_top + 0x10000;
    let mut vdso_code_addr: usize = 0;
    let mut vdso_data_addr: usize = 0;
    // Map code pages (read + execute)
    let s = unsafe {
        zx_vmar_map(
            init_vmar,
            ZX_VM_PERM_READ | ZX_VM_PERM_EXECUTE | ZX_VM_SPECIFIC,
            vdso_base_addr,
            vdso_vmo,
            0,                // offset 0 in VMO
            VDSO_DATA_OFFSET, // pages 0-6
            &mut vdso_code_addr,
        )
    };
    if s != ZX_OK {
        debug_print(b"userstart: vDSO code map failed\n");
        vdso_code_addr = 0;
    } else {
        debug_print(b"userstart: vDSO code mapped\n");
    }
    // Map data page (read-only) right after code
    let s = unsafe {
        zx_vmar_map(
            init_vmar,
            ZX_VM_PERM_READ | ZX_VM_SPECIFIC,
            vdso_base_addr + VDSO_DATA_OFFSET,
            vdso_vmo,
            VDSO_DATA_OFFSET, // offset 0x7000
            PAGE_SIZE,
            &mut vdso_data_addr,
        )
    };
    if s != ZX_OK {
        debug_print(b"userstart: warning: failed to map vDSO data\n");
    }

    // Step 8: Create a channel to forward bootstrap handles to init
    let mut init_channel_local: HandleValue = ZX_HANDLE_INVALID;
    let mut init_channel_remote: HandleValue = ZX_HANDLE_INVALID;
    check("channel_create", unsafe {
        zx_channel_create(0, &mut init_channel_local, &mut init_channel_remote)
    });

    // Forward the remaining bootstrap handles to init via the channel.
    // We pass: root job and the ZBI VMO.
    let forward_handles = [root_job, zbi_vmo];
    check("channel_write", unsafe {
        zx_channel_write(
            init_channel_local,
            0,
            core::ptr::null(), // no data bytes
            0,
            forward_handles.as_ptr(),
            forward_handles.len() as u32,
        )
    });

    debug_print(b"userstart: starting init process\n");
    check("process_start", unsafe {
        zx_process_start(
            init_proc,
            init_thread,
            entry_addr,
            stack_top,
            init_channel_remote, // pass channel to init
            vdso_code_addr,      // vDSO base address
        )
    });

    debug_print(b"userstart: init process started, waiting for it to exit\n");

    // Wait for the init process to terminate
    let mut observed: u32 = 0;
    check("object_wait_one", unsafe {
        zx_object_wait_one(
            init_proc,
            ZX_PROCESS_TERMINATED,
            i64::MAX, // ZX_TIME_INFINITE
            &mut observed,
        )
    });

    debug_print(b"userstart: init process exited, shutting down\n");

    // Small delay to let any pending UART output from init drain
    for _ in 0..100_000 {
        core::hint::spin_loop();
    }

    // Close our handles and exit
    unsafe {
        zx_handle_close(init_proc);
        zx_handle_close(init_thread);
        zx_handle_close(init_vmar);
        // code_vmo is local to load_elf/load_flat and already consumed
        zx_handle_close(stack_vmo);
        zx_handle_close(zbi_vmo);
        // Close remaining bootstrap handles
        for &h in &handles {
            if h != ZX_HANDLE_INVALID {
                zx_handle_close(h);
            }
        }
        zx_process_exit(0);
    }
}

/// Load an ELF binary into a process, mapping each PT_LOAD segment.
/// Returns (entry_addr, map_end) where map_end is the highest mapped address.
fn load_elf(data: &[u8], vmar: HandleValue) -> (usize, usize) {
    // Minimal ELF64 header parsing (no external crate).
    // Check magic
    if data.len() < 64 || &data[0..4] != b"\x7fELF" {
        // Not an ELF -- fall back to flat binary loading
        debug_print(b"userstart: not ELF, loading as flat binary\n");
        return load_flat(data, vmar);
    }

    let e_entry = u64::from_le_bytes(data[24..32].try_into().unwrap()) as usize;
    let e_phoff = u64::from_le_bytes(data[32..40].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(data[54..56].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(data[56..58].try_into().unwrap()) as usize;

    const PT_LOAD: u32 = 1;

    let base: usize = 0x10000; // load base to avoid null page

    // Create a single VMO large enough for all segments.
    // Find the total size first.
    let mut total_size: usize = 0;
    for i in 0..e_phnum {
        let ph = &data[e_phoff + i * e_phentsize..];
        let p_type = u32::from_le_bytes(ph[0..4].try_into().unwrap());
        if p_type != PT_LOAD {
            continue;
        }
        let p_vaddr = u64::from_le_bytes(ph[16..24].try_into().unwrap()) as usize;
        let p_memsz = u64::from_le_bytes(ph[40..48].try_into().unwrap()) as usize;
        let seg_end = p_vaddr + p_memsz;
        if seg_end > total_size {
            total_size = seg_end;
        }
    }

    let total_pages = total_size.div_ceil(PAGE_SIZE);
    let vmo_size = total_pages * PAGE_SIZE;

    let mut code_vmo: HandleValue = ZX_HANDLE_INVALID;
    check("vmo_create(elf)", unsafe {
        zx_vmo_create(vmo_size as u64, 0, &mut code_vmo)
    });

    // Write each PT_LOAD segment into the VMO at its virtual address offset.
    for i in 0..e_phnum {
        let ph = &data[e_phoff + i * e_phentsize..];
        let p_type = u32::from_le_bytes(ph[0..4].try_into().unwrap());
        if p_type != PT_LOAD {
            continue;
        }
        let p_offset = u64::from_le_bytes(ph[8..16].try_into().unwrap()) as usize;
        let p_vaddr = u64::from_le_bytes(ph[16..24].try_into().unwrap()) as usize;
        let p_filesz = u64::from_le_bytes(ph[32..40].try_into().unwrap()) as usize;

        if p_filesz > 0 && p_offset + p_filesz <= data.len() {
            check("vmo_write(seg)", unsafe {
                zx_vmo_write(
                    code_vmo,
                    data[p_offset..].as_ptr(),
                    p_vaddr as u64,
                    p_filesz,
                )
            });
        }
    }

    // Make executable
    let mut exec_vmo: HandleValue = ZX_HANDLE_INVALID;
    check("vmo_replace_as_executable", unsafe {
        zx_vmo_replace_as_executable(code_vmo, ZX_HANDLE_INVALID, &mut exec_vmo)
    });
    code_vmo = exec_vmo;

    // Map the whole VMO with RWX (segments share the VMO).
    // The kernel will enforce per-page permissions via page faults.
    let mut mapped_addr: usize = 0;
    check("vmar_map(elf)", unsafe {
        zx_vmar_map(
            vmar,
            ZX_VM_PERM_READ
                | ZX_VM_PERM_WRITE
                | ZX_VM_PERM_EXECUTE
                | ZX_VM_SPECIFIC
                | ZX_VM_MAP_RANGE,
            base,
            code_vmo,
            0,
            vmo_size,
            &mut mapped_addr,
        )
    });

    let map_end = base + vmo_size;
    let entry = base + e_entry;
    debug_print(b"userstart: ELF loaded\n");
    (entry, map_end)
}

/// Fallback: load flat binary (no ELF headers).
fn load_flat(data: &[u8], vmar: HandleValue) -> (usize, usize) {
    let code_size = data.len();
    let code_pages = code_size.div_ceil(PAGE_SIZE);
    let map_size = code_pages * PAGE_SIZE;

    let mut code_vmo: HandleValue = ZX_HANDLE_INVALID;
    check("vmo_create", unsafe {
        zx_vmo_create(map_size as u64, 0, &mut code_vmo)
    });
    check("vmo_write", unsafe {
        zx_vmo_write(code_vmo, data.as_ptr(), 0, code_size)
    });
    let mut exec_vmo: HandleValue = ZX_HANDLE_INVALID;
    check("vmo_replace_as_executable", unsafe {
        zx_vmo_replace_as_executable(code_vmo, ZX_HANDLE_INVALID, &mut exec_vmo)
    });
    code_vmo = exec_vmo;

    let code_base: usize = 0x10000;
    let mut entry_addr: usize = 0;
    check("vmar_map(flat)", unsafe {
        zx_vmar_map(
            vmar,
            ZX_VM_PERM_READ | ZX_VM_PERM_EXECUTE | ZX_VM_SPECIFIC | ZX_VM_MAP_RANGE,
            code_base,
            code_vmo,
            0,
            map_size,
            &mut entry_addr,
        )
    });
    (code_base, code_base + map_size)
}

/// Panic handler.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug_write(b"userstart: PANIC!\n");
    process_exit(1);
}
