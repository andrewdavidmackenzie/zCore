//! petal shell -- a minimal interactive shell for zCore.
//!
//! Exercises: alloc (String, Vec), console I/O, command parsing, builtins.
//!
//! Always runs a self-test first (for CI). Then, if the root resource
//! handle is available, enters an interactive loop reading from the
//! serial console via debug_read.

#![no_std]
#![no_main]

extern crate alloc;
extern crate petal;

use alloc::string::String;
use alloc::vec::Vec;

const PROMPT: &[u8] = b"petal> ";

/// Bootstrap handle indices (must match userstart's forward order).
#[allow(dead_code)]
const H_ROOT_JOB: usize = 0;
const H_ROOT_RESOURCE: usize = 1;
#[allow(dead_code)]
const H_ZBI_VMO: usize = 2;

#[no_mangle]
pub fn main() {
    zx::debug_write(b"petal shell v0.1\n");

    // --- Self-test (always runs, for CI) ---
    zx::debug_write(b"shell: running self-test\n");
    let commands = ["help", "echo hello world", "version"];
    for cmd in &commands {
        zx::debug_write(PROMPT);
        zx::debug_write(cmd.as_bytes());
        zx::debug_write(b"\n");
        run_command(cmd);
    }
    zx::debug_write(b"shell: self-test PASS\n");

    // --- Try to enter interactive mode ---
    let startup = petal::take_startup_handle();
    if startup == 0 {
        return; // no startup handle, exit after self-test
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
        return; // no root resource, exit after self-test
    }

    // Interactive mode
    zx::debug_write(b"\nType 'help' for commands, 'exit' to quit.\n\n");
    interactive_loop(root_resource);
}

/// Interactive command loop using debug_read for input.
fn interactive_loop(resource: u32) {
    loop {
        zx::debug_write(PROMPT);

        match read_line(resource) {
            Some(line) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() && run_command(trimmed) {
                    break;
                }
            }
            None => {
                // EOF (Ctrl-D on empty line) or read error.
                zx::debug_write(b"\ngoodbye\n");
                break;
            }
        }
    }
}

/// Read a line from the console, echoing characters back.
/// Returns None on EOF (Ctrl-D on empty line) or read error.
fn read_line(resource: u32) -> Option<String> {
    let mut line = String::new();
    let mut buf = [0u8; 1];

    loop {
        match zx::console_read(resource, &mut buf) {
            Ok(0) => return None, // EOF
            Ok(_) => {
                let ch = buf[0];
                match ch {
                    b'\n' | b'\r' => {
                        zx::debug_write(b"\n");
                        return Some(line);
                    }
                    0x04 => {
                        // Ctrl-D: EOF if line is empty
                        if line.is_empty() {
                            return None;
                        }
                    }
                    0x08 | 0x7f => {
                        // Backspace / DEL
                        if !line.is_empty() {
                            line.pop();
                            zx::debug_write(b"\x08 \x08");
                        }
                    }
                    0x15 => {
                        // Ctrl-U: clear line
                        while !line.is_empty() {
                            line.pop();
                            zx::debug_write(b"\x08 \x08");
                        }
                    }
                    _ if ch >= 0x20 => {
                        line.push(ch as char);
                        zx::debug_write(&[ch]);
                    }
                    _ => {} // ignore other control characters
                }
            }
            Err(_) => return None,
        }
    }
}

/// Execute a command. Returns true if the shell should exit.
fn run_command(line: &str) -> bool {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }
    match parts[0] {
        "help" => {
            zx::debug_write(b"Available commands:\n");
            zx::debug_write(b"  help     - show this message\n");
            zx::debug_write(b"  echo     - print arguments\n");
            zx::debug_write(b"  version  - show shell version\n");
            zx::debug_write(b"  exit     - exit the shell\n");
        }
        "echo" => {
            for (i, arg) in parts[1..].iter().enumerate() {
                if i > 0 {
                    zx::debug_write(b" ");
                }
                zx::debug_write(arg.as_bytes());
            }
            zx::debug_write(b"\n");
        }
        "version" => {
            zx::debug_write(b"petal shell v0.1 on zCore\n");
        }
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
