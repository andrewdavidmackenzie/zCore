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
use noline::builder::EditorBuilder;

/// Bootstrap handle indices (must match userstart's forward order).
#[allow(dead_code)]
const H_ROOT_JOB: usize = 0;
const H_ROOT_RESOURCE: usize = 1;
#[allow(dead_code)]
const H_ZBI_VMO: usize = 2;

/// Console I/O adapter implementing embedded_io Read + Write traits.
/// Wraps debug_write (output) and debug_read (input via root resource).
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

#[no_mangle]
pub fn main() {
    zx::debug_write(b"petal shell v0.1\n");

    // --- Self-test (always runs, for CI) ---
    zx::debug_write(b"shell: running self-test\n");
    let commands = ["help", "echo hello world", "version"];
    for cmd in &commands {
        zx::debug_write(b"petal> ");
        zx::debug_write(cmd.as_bytes());
        zx::debug_write(b"\n");
        run_command(cmd);
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

    let root_resource = if status == 0 && actual_handles >= 2 {
        handles[H_ROOT_RESOURCE]
    } else {
        0
    };

    if root_resource == 0 {
        return;
    }

    // Interactive mode with noline editor
    zx::debug_write(b"\nType 'help' for commands, 'exit' to quit.\n\n");

    let mut io = Console {
        resource: root_resource,
    };

    let Ok(mut editor) = EditorBuilder::new_unbounded()
        .with_unbounded_history()
        .build_sync(&mut io)
    else {
        zx::debug_write(b"shell: failed to initialize editor\n");
        return;
    };

    loop {
        match editor.readline("petal> ", &mut io) {
            Ok(line) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() && run_command(trimmed) {
                    break;
                }
            }
            Err(_) => {
                zx::debug_write(b"\ngoodbye\n");
                break;
            }
        }
    }
}

// --- Builtins ---

/// Execute a command. Returns true if the shell should exit.
fn run_command(line: &str) -> bool {
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
        "exit" | "quit" => {
            zx::debug_write(b"goodbye\n");
            return true;
        }
        _ => {
            zx::debug_write(b"unknown command: ");
            zx::debug_write(parts[0].as_bytes());
            zx::debug_write(b"\n");
        }
    }
    false
}

fn cmd_help() {
    zx::debug_write(b"Available commands:\n");
    zx::debug_write(b"  help     - show this message\n");
    zx::debug_write(b"  echo     - print arguments\n");
    zx::debug_write(b"  version  - show shell version\n");
    zx::debug_write(b"  uptime   - show system uptime\n");
    zx::debug_write(b"  sysinfo  - show system information\n");
    zx::debug_write(b"  exit     - exit the shell\n");
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
    // Use clock_get with ZX_CLOCK_MONOTONIC (0) to get nanoseconds since boot.
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
    // Read vDSO constants from the mapped data page.
    let mut vdso_base: usize = 0;
    let status = unsafe {
        zx::sys::zx_object_get_property(
            0x1, // any valid handle (kernel falls back to calling process)
            6,   // ZX_PROP_PROCESS_VDSO_BASE_ADDRESS
            &mut vdso_base as *mut usize as *mut u8,
            core::mem::size_of::<usize>(),
        )
    };

    if status != 0 || vdso_base == 0 {
        zx::debug_write(b"sysinfo: cannot read vDSO constants\n");
        return;
    }

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

    let constants = unsafe { &*(vdso_base as *const VdsoConstants) };

    zx::debug_write(b"CPUs:    ");
    write_decimal(constants.max_num_cpus as usize);
    zx::debug_write(b"\n");

    zx::debug_write(b"Memory:  ");
    let mb = constants.physmem / (1024 * 1024);
    write_decimal(mb as usize);
    zx::debug_write(b" MiB\n");

    if constants.version_string_len > 0 {
        let len = constants.version_string_len as usize;
        let len = if len > 64 { 64 } else { len };
        zx::debug_write(b"Version: ");
        zx::debug_write(&constants.version_string[..len]);
        zx::debug_write(b"\n");
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
