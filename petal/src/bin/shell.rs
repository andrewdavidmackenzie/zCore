//! petal shell -- a minimal interactive shell for zCore.
//!
//! Exercises: alloc (String, Vec), command parsing, builtins.
//! Runs a self-test with canned commands.

#![no_std]
#![no_main]

extern crate alloc;
extern crate petal;

use alloc::vec::Vec;

const PROMPT: &[u8] = b"petal> ";

#[no_mangle]
pub fn main() {
    zx::debug_write(b"petal shell v0.1\n");
    zx::debug_write(b"shell: running self-test\n");

    let commands = ["help", "echo hello world", "version"];
    for cmd in &commands {
        zx::debug_write(PROMPT);
        zx::debug_write(cmd.as_bytes());
        zx::debug_write(b"\n");
        run_command(cmd);
    }

    zx::debug_write(b"shell: self-test PASS\n");
}

/// Execute a command.
fn run_command(line: &str) {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return;
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
            zx::Process::exit(0);
        }
        _ => {
            zx::debug_write(b"unknown command: ");
            zx::debug_write(parts[0].as_bytes());
            zx::debug_write(b"\n");
        }
    }
}
