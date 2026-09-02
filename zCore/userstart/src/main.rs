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
const K_HANDLECOUNT: usize = 15;

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

    check(
        "channel_read",
        unsafe {
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
        },
    );

    debug_print(b"userstart: received bootstrap handles\n");

    // Close the bootstrap channel -- we've read all the handles
    unsafe { zx_handle_close(bootstrap_handle) };

    let _proc_self = handles[K_PROC_SELF];
    let vmar_self = handles[K_VMARROOT_SELF];
    let root_job = handles[K_ROOTJOB];
    let zbi_vmo = handles[K_ZBI];

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
            0,        // vmar_offset (anywhere)
            zbi_vmo,
            0,        // vmo_offset
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

    // Step 6: Create a VMO with the program code and map it
    let code_size = program_data.len();
    let code_pages = (code_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let map_size = code_pages * PAGE_SIZE;

    #[allow(unused_mut)]
    let mut code_vmo: HandleValue = ZX_HANDLE_INVALID;
    check("vmo_create", unsafe {
        zx_vmo_create(map_size as u64, 0, &mut code_vmo)
    });

    check("vmo_write", unsafe {
        zx_vmo_write(code_vmo, program_data.as_ptr(), 0, code_size)
    });

    // Make the VMO executable so we can map it with PERM_EXECUTE
    let mut exec_vmo: HandleValue = ZX_HANDLE_INVALID;
    check("vmo_replace_as_executable", unsafe {
        zx_vmo_replace_as_executable(code_vmo, ZX_HANDLE_INVALID, &mut exec_vmo)
    });
    // The original handle is consumed by replace_as_executable
    code_vmo = exec_vmo;

    let mut entry_addr: usize = 0;
    check("vmar_map(code)", unsafe {
        zx_vmar_map(
            init_vmar,
            ZX_VM_PERM_READ | ZX_VM_PERM_EXECUTE,
            0,        // vmar_offset (anywhere)
            code_vmo,
            0,        // vmo_offset
            map_size,
            &mut entry_addr,
        )
    });

    // Step 7: Create a stack for the init program
    let stack_pages = 8;
    let stack_size = stack_pages * PAGE_SIZE;
    let mut stack_vmo: HandleValue = ZX_HANDLE_INVALID;
    check("vmo_create(stack)", unsafe {
        zx_vmo_create(stack_size as u64, 0, &mut stack_vmo)
    });

    let mut stack_base: usize = 0;
    check("vmar_map(stack)", unsafe {
        zx_vmar_map(
            init_vmar,
            ZX_VM_PERM_READ | ZX_VM_PERM_WRITE,
            0,
            stack_vmo,
            0,
            stack_size,
            &mut stack_base,
        )
    });

    let stack_top = stack_base + stack_size;

    // Step 8: Start the init process
    // Pass ZX_HANDLE_INVALID as arg1 (init doesn't need a channel for now)
    debug_print(b"userstart: entry=");
    // Print entry_addr as hex (simple hex printer for no_std)
    let mut hex_buf = [b'0'; 16];
    let mut val = entry_addr;
    for i in (0..16).rev() {
        let nibble = (val & 0xf) as u8;
        hex_buf[i] = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
        val >>= 4;
    }
    debug_print(&hex_buf);
    debug_print(b" stack=");
    val = stack_top;
    for i in (0..16).rev() {
        let nibble = (val & 0xf) as u8;
        hex_buf[i] = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
        val >>= 4;
    }
    debug_print(&hex_buf);
    debug_print(b"\n");
    debug_print(b"userstart: starting init process\n");
    check("process_start", unsafe {
        zx_process_start(
            init_proc,
            init_thread,
            entry_addr,
            stack_top,
            ZX_HANDLE_INVALID, // arg1_handle
            0,                 // arg2
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
        zx_handle_close(code_vmo);
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

/// Panic handler.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug_write(b"userstart: PANIC!\n");
    process_exit(1);
}
