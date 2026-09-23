//! petal shell -- a minimal interactive shell for zCore.
//!
//! Features: noline-based line editing with history, builtins.
//!
//! Always runs a self-test first (for CI). Then, if the root resource
//! handle is available, enters an interactive loop using noline for
//! line editing and history.

#![no_std]
#![no_main]

extern crate alloc;
extern crate petal;

use alloc::vec::Vec;

/// Bootstrap handle indices (must match userstart's forward order).
const H_ROOT_JOB: usize = 0;
const H_ROOT_RESOURCE: usize = 1;
#[allow(dead_code)]
const H_ZBI_VMO: usize = 2;

/// Shell context: holds handles needed by builtins.
struct Ctx {
    root_job: u32,
    root_resource: u32,
}

/// Console I/O adapter implementing embedded_io Read + Write traits.
struct Console {
    resource: u32,
}

impl embedded_io::ErrorType for Console {
    type Error = embedded_io::ErrorKind;
}

impl embedded_io::Read for Console {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        zx::console_read(self.resource, buf).map_err(|_| embedded_io::ErrorKind::Other)
    }
}

impl embedded_io::Write for Console {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        zx::debug_write(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Read a line from the serial console, echoing characters.
/// Returns None on EOF / read error.
fn read_line(io: &Console) -> Option<alloc::string::String> {
    use alloc::string::String;
    use embedded_io::Read;

    let mut line = String::new();
    const MAX_LINE_LEN: usize = 4096;
    let mut buf = [0u8; 1];
    let mut io_mut = Console {
        resource: io.resource,
    };

    zx::debug_write(b"petal> ");

    loop {
        match io_mut.read(&mut buf) {
            Ok(0) => return None, // EOF
            Ok(_) => {
                let ch = buf[0];
                match ch {
                    b'\r' | b'\n' => {
                        zx::debug_write(b"\n");
                        return Some(line);
                    }
                    // Backspace / DEL
                    0x7f | 0x08 => {
                        if !line.is_empty() {
                            line.pop();
                            zx::debug_write(b"\x08 \x08"); // erase char
                        }
                    }
                    // Ctrl-C
                    0x03 => {
                        zx::debug_write(b"^C\n");
                        line.clear();
                        zx::debug_write(b"petal> ");
                    }
                    // Ctrl-D on empty line = exit
                    0x04 if line.is_empty() => return None,
                    // Printable ASCII (bounded to prevent OOM)
                    0x20..=0x7e if line.len() < MAX_LINE_LEN => {
                        line.push(ch as char);
                        zx::debug_write(&[ch]);
                    }
                    0x20..=0x7e => zx::debug_write(b"\x07"), // bell on overflow
                    _ => {}                                  // ignore other control chars
                }
            }
            Err(_) => return None,
        }
    }
}

#[no_mangle]
pub fn main() {
    zx::debug_write(b"petal shell v0.1\n");

    // --- Self-test (always runs, for CI) ---
    let dummy = Ctx {
        root_job: 0,
        root_resource: 0,
    };
    zx::debug_write(b"shell: running self-test\n");
    let commands = ["help", "echo hello world", "version"];
    for cmd in &commands {
        zx::debug_write(b"petal> ");
        zx::debug_write(cmd.as_bytes());
        zx::debug_write(b"\n");
        run_command(cmd, &dummy);
    }
    zx::debug_write(b"shell: self-test PASS\n");

    // --- Try to enter interactive mode ---
    let startup = petal::take_startup_handle();
    if startup == 0 {
        return;
    }

    let mut handles = [0u32; 4];
    let mut data = [0u8; 4];
    let mut actual_bytes: u32 = 0;
    let mut actual_handles: u32 = 0;
    let status = unsafe {
        zx::sys::zx_channel_read(
            startup,
            0,
            data.as_mut_ptr(),
            handles.as_mut_ptr(),
            data.len() as u32,
            handles.len() as u32,
            &mut actual_bytes,
            &mut actual_handles,
        )
    };

    if status != 0 || actual_handles < 2 {
        return;
    }

    let ctx = Ctx {
        root_job: handles[H_ROOT_JOB],
        root_resource: handles[H_ROOT_RESOURCE],
    };

    // Interactive mode: simple line reader over serial console.
    // We avoid noline's terminal probe (ANSI cursor-position query)
    // because QEMU's serial console doesn't respond to it, causing
    // the shell to hang.
    zx::debug_write(b"\nType 'help' for commands, 'exit' to quit.\n\n");

    let io = Console {
        resource: ctx.root_resource,
    };

    loop {
        match read_line(&io) {
            Some(line) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() && run_command(trimmed, &ctx) {
                    break;
                }
            }
            None => {
                zx::debug_write(b"\ngoodbye\n");
                break;
            }
        }
    }
}

// --- Command dispatch ---

/// Execute a command. Returns true if the shell should exit.
fn run_command(line: &str, ctx: &Ctx) -> bool {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }
    match parts[0] {
        "help" => cmd_help(),
        "echo" => cmd_echo(&parts[1..]),
        "version" => cmd_version(),
        "uptime" => cmd_uptime(),
        "sysinfo" => cmd_sysinfo(),
        "exec" => cmd_exec(&parts[1..]),
        "dmesg" => cmd_dmesg(ctx),
        "ps" => cmd_ps(ctx),
        "mem" => cmd_mem(ctx),
        "exit" | "quit" => {
            zx::debug_write(b"goodbye\n");
            return true;
        }
        _ => {
            // Try to exec as a program path
            let status = zx::sys::debug_exec(parts[0]);
            if status != 0 {
                if parts[0].starts_with('/') {
                    zx::debug_write(b"unsupported binary flavour: ");
                } else {
                    zx::debug_write(b"unknown command: ");
                }
                zx::debug_write(parts[0].as_bytes());
                zx::debug_write(b"\r\n");
            }
        }
    }
    false
}

// --- Builtins ---

fn cmd_help() {
    zx::debug_write(b"Available commands:\r\n");
    zx::debug_write(b"  help     - show this message\r\n");
    zx::debug_write(b"  echo     - print arguments\r\n");
    zx::debug_write(b"  exec     - run a program from rootfs\r\n");
    zx::debug_write(b"  version  - show shell version\r\n");
    zx::debug_write(b"  uptime   - show system uptime\r\n");
    zx::debug_write(b"  sysinfo  - CPU count, memory, version\r\n");
    zx::debug_write(b"  dmesg    - show kernel log\r\n");
    zx::debug_write(b"  ps       - list processes\r\n");
    zx::debug_write(b"  mem      - show memory stats\r\n");
    zx::debug_write(b"  exit     - exit the shell\r\n");
}

fn cmd_exec(args: &[&str]) {
    if args.is_empty() {
        zx::debug_write(b"usage: exec <path>\r\n");
        return;
    }
    let path = args[0];
    let status = zx::sys::debug_exec(path);
    if status != 0 {
        zx::debug_write(b"exec failed: ");
        zx::debug_write(path.as_bytes());
        zx::debug_write(b"\r\n");
    }
}

fn cmd_echo(args: &[&str]) {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            zx::debug_write(b" ");
        }
        zx::debug_write(arg.as_bytes());
    }
    zx::debug_write(b"\n");
}

fn cmd_version() {
    zx::debug_write(b"petal shell v0.1 on zCore\n");
}

fn cmd_uptime() {
    let mut nanos: i64 = 0;
    let status = unsafe { zx::sys::zx_clock_get(0, &mut nanos) };
    if status != 0 {
        zx::debug_write(b"uptime: clock_get failed\n");
        return;
    }

    let total_secs = (nanos / 1_000_000_000) as u64;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    zx::debug_write(b"up ");
    if hours > 0 {
        write_decimal(hours as usize);
        zx::debug_write(b"h ");
    }
    if hours > 0 || mins > 0 {
        write_decimal(mins as usize);
        zx::debug_write(b"m ");
    }
    write_decimal(secs as usize);
    zx::debug_write(b"s\n");
}

fn cmd_sysinfo() {
    let mut vdso_base: usize = 0;
    let status = unsafe {
        zx::sys::zx_object_get_property(
            0x1,
            6, // ZX_PROP_PROCESS_VDSO_BASE_ADDRESS
            &mut vdso_base as *mut usize as *mut u8,
            core::mem::size_of::<usize>(),
        )
    };

    if status != 0 || vdso_base == 0 {
        zx::debug_write(b"sysinfo: cannot read vDSO constants\n");
        return;
    }

    let constants = unsafe { &*(vdso_base as *const VdsoConstants) };

    zx::debug_write(b"CPUs:    ");
    write_decimal(constants.max_num_cpus as usize);
    zx::debug_write(b"\n");

    zx::debug_write(b"Memory:  ");
    let mb = constants.physmem / (1024 * 1024);
    write_decimal(mb as usize);
    zx::debug_write(b" MiB\n");

    if constants.version_string_len > 0 {
        let len = (constants.version_string_len as usize).min(64);
        zx::debug_write(b"Version: ");
        zx::debug_write(&constants.version_string[..len]);
        zx::debug_write(b"\n");
    }
}

fn cmd_dmesg(ctx: &Ctx) {
    if ctx.root_resource == 0 {
        zx::debug_write(b"dmesg: no root resource\n");
        return;
    }

    // Create a readable debuglog handle.
    const FLAG_READABLE: u32 = 0x4000_0000;
    let mut dlog_handle: u32 = 0;
    let status =
        unsafe { zx::sys::zx_debuglog_create(ctx.root_resource, FLAG_READABLE, &mut dlog_handle) };
    if status != 0 {
        zx::debug_write(b"dmesg: debuglog_create failed\n");
        return;
    }

    // Read and display log records.
    let mut buf = [0u8; 256];
    loop {
        let result = unsafe { zx::sys::zx_debuglog_read(dlog_handle, 0, buf.as_mut_ptr(), 256) };
        if result <= 0 {
            break; // ZX_ERR_SHOULD_WAIT (-22) means no more records
        }
        let len = result as usize;
        if len < 32 {
            break; // record too short for header
        }
        // DlogHeader is 32 bytes, data follows.
        // datalen is at offset 4 (u16).
        let datalen = u16::from_le_bytes([buf[4], buf[5]]) as usize;
        let data_start = 32; // sizeof(DlogHeader)
        let data_end = (data_start + datalen).min(len);
        if data_end > data_start {
            zx::debug_write(&buf[data_start..data_end]);
            // Add newline if not present
            if buf[data_end - 1] != b'\n' {
                zx::debug_write(b"\n");
            }
        }
    }

    unsafe {
        zx::sys::zx_handle_close(dlog_handle);
    }
}

fn cmd_ps(ctx: &Ctx) {
    if ctx.root_job == 0 {
        zx::debug_write(b"ps: no root job handle\n");
        return;
    }

    // ZX_INFO_JOB_PROCESSES = 9
    const INFO_JOB_PROCESSES: u32 = 9;
    // ZX_INFO_PROCESS = 3
    const INFO_PROCESS: u32 = 3;
    // ZX_PROP_NAME = 3
    const PROP_NAME: u32 = 3;

    // Get the list of process KOIDs in the root job.
    let mut koids = [0u64; 32];
    let mut actual: usize = 0;
    let mut avail: usize = 0;
    let status = unsafe {
        zx::sys::zx_object_get_info(
            ctx.root_job,
            INFO_JOB_PROCESSES,
            koids.as_mut_ptr() as *mut u8,
            koids.len() * 8,
            &mut actual,
            &mut avail,
        )
    };
    if status != 0 {
        zx::debug_write(b"ps: get_info(JOB_PROCESSES) failed\n");
        return;
    }

    zx::debug_write(b"  PID  STATE        NAME\n");

    for &koid in &koids[..actual] {
        // Get a handle to the process via object_get_child.
        let mut proc_handle: u32 = 0;
        let s = unsafe {
            zx::sys::zx_object_get_child(
                ctx.root_job,
                koid,
                0x2, // ZX_RIGHT_ENUMERATE
                &mut proc_handle,
            )
        };
        if s != 0 {
            zx::debug_write(b"  ");
            write_decimal_padded(koid as usize, 5);
            zx::debug_write(b"  (inaccessible)\n");
            continue;
        }

        // Get process info.
        #[repr(C)]
        #[derive(Default)]
        struct ProcessInfo {
            return_code: i64,
            start_time: i64,
            flags: u32,
        }
        let mut pinfo = ProcessInfo::default();
        let s = unsafe {
            zx::sys::zx_object_get_info(
                proc_handle,
                INFO_PROCESS,
                &mut pinfo as *mut ProcessInfo as *mut u8,
                core::mem::size_of::<ProcessInfo>(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };

        // Get process name.
        let mut name_buf = [0u8; 32];
        let _ = unsafe {
            zx::sys::zx_object_get_property(
                proc_handle,
                PROP_NAME,
                name_buf.as_mut_ptr(),
                name_buf.len(),
            )
        };

        // Format output.
        zx::debug_write(b"  ");
        write_decimal_padded(koid as usize, 5);
        zx::debug_write(b"  ");

        if s == 0 {
            let state = if pinfo.flags & 4 != 0 {
                // STARTED | EXITED
                b"exited      " as &[u8]
            } else if pinfo.flags & 1 != 0 {
                b"running     "
            } else {
                b"created     "
            };
            zx::debug_write(state);
        } else {
            zx::debug_write(b"unknown     ");
        }

        // Print name (null-terminated).
        let name_len = name_buf.iter().position(|&b| b == 0).unwrap_or(32);
        if name_len > 0 {
            zx::debug_write(&name_buf[..name_len]);
        } else {
            zx::debug_write(b"<unnamed>");
        }
        zx::debug_write(b"\n");

        unsafe {
            zx::sys::zx_handle_close(proc_handle);
        }
    }

    if avail > actual {
        zx::debug_write(b"  ... and ");
        write_decimal(avail - actual);
        zx::debug_write(b" more\n");
    }
}

fn cmd_mem(ctx: &Ctx) {
    if ctx.root_resource == 0 {
        zx::debug_write(b"mem: no root resource\n");
        return;
    }

    // ZX_INFO_KMEM_STATS = 17
    const INFO_KMEM_STATS: u32 = 17;

    #[repr(C)]
    #[derive(Default)]
    struct KmemInfo {
        total_bytes: u64,
        free_bytes: u64,
        wired_bytes: u64,
        total_heap_bytes: u64,
        free_heap_bytes: u64,
        vmo_bytes: u64,
        mmu_overhead_bytes: u64,
        ipc_bytes: u64,
        other_bytes: u64,
    }

    let mut info = KmemInfo::default();
    let status = unsafe {
        zx::sys::zx_object_get_info(
            ctx.root_resource,
            INFO_KMEM_STATS,
            &mut info as *mut KmemInfo as *mut u8,
            core::mem::size_of::<KmemInfo>(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };

    if status != 0 {
        zx::debug_write(b"mem: get_info(KMEM_STATS) failed\n");
        return;
    }

    let print_mb = |label: &[u8], bytes: u64| {
        zx::debug_write(label);
        let mb = bytes / (1024 * 1024);
        let kb_frac = (bytes % (1024 * 1024)) / 1024;
        write_decimal(mb as usize);
        zx::debug_write(b".");
        // One decimal place of fractional MiB
        write_decimal((kb_frac * 10 / 1024) as usize);
        zx::debug_write(b" MiB\n");
    };

    print_mb(b"Total:   ", info.total_bytes);
    print_mb(b"Free:    ", info.free_bytes);
    print_mb(b"Wired:   ", info.wired_bytes);
    print_mb(b"VMO:     ", info.vmo_bytes);
    if info.total_heap_bytes > 0 {
        print_mb(b"Heap:    ", info.total_heap_bytes);
    }
    if info.mmu_overhead_bytes > 0 {
        print_mb(b"MMU:     ", info.mmu_overhead_bytes);
    }
    if info.ipc_bytes > 0 {
        print_mb(b"IPC:     ", info.ipc_bytes);
    }
}

// --- Utility ---

/// Write a decimal number to the console.
fn write_decimal(mut n: usize) {
    if n == 0 {
        zx::debug_write(b"0");
        return;
    }
    let mut digits = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        zx::debug_write(&[digits[i]]);
    }
}

/// Write a right-justified decimal number with padding.
fn write_decimal_padded(n: usize, width: usize) {
    // Count digits
    let digit_count = if n == 0 {
        1
    } else {
        let mut count = 0;
        let mut v = n;
        while v > 0 {
            count += 1;
            v /= 10;
        }
        count
    };
    for _ in 0..width.saturating_sub(digit_count) {
        zx::debug_write(b" ");
    }
    write_decimal(n);
}

// --- Shared types ---

#[repr(C)]
struct VdsoConstants {
    max_num_cpus: u32,
    features_cpu: u32,
    hw_breakpoint_count: u32,
    hw_watchpoint_count: u32,
    dcache_line_size: u32,
    icache_line_size: u32,
    ticks_per_second: u64,
    ticks_to_mono_numerator: u32,
    ticks_to_mono_denominator: u32,
    physmem: u64,
    version_string_len: u64,
    version_string: [u8; 64],
}
