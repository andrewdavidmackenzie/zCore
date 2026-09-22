//! Objects for Task Management.

use super::*;
use alloc::sync::Arc;

mod exception;
mod job;
mod job_policy;
mod process;
/// Process spawning from ELF binaries.
#[cfg(feature = "elf")]
pub mod spawn;
mod suspend_token;
mod thread;

pub use {
    self::exception::*, self::job::*, self::job_policy::*, self::process::*,
    self::suspend_token::*, self::thread::*,
};

/// Task (Thread, Process, or Job)
pub trait Task: Sync + Send {
    /// Kill the task. The task do not terminate immediately when killed.
    /// It will terminate after all its children are terminated or some cleanups are finished.
    fn kill(&self);

    /// Suspend the task. Currently only thread or process handles may be suspended.
    fn suspend(&self);

    /// Resume the task
    fn resume(&self);

    /// Get the exceptionate.
    fn exceptionate(&self) -> Arc<Exceptionate>;

    /// Get the debug exceptionate.
    fn debug_exceptionate(&self) -> Arc<Exceptionate>;
}

/// The return code set when a task is killed via zx_task_kill().
pub const TASK_RETCODE_SYSCALL_KILL: i64 = -1028;

/// Kernel personality — determines syscall ABI and process model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Personality {
    /// Linux personality: syscall via x8 (aarch64), 6 args,
    /// LinuxProcess extension, POSIX signal model.
    Linux,
    /// Zircon personality: syscall via x16 (aarch64), 8 args,
    /// no extension, handle-based IPC.
    Zircon,
}

// ELF e_ident field indices (from the ELF specification).
/// Index of OS/ABI byte in `e_ident`.
const EI_OSABI: usize = 7;

/// ELF magic number (`\x7fELF`).
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF OS/ABI value for zCore Zircon personality binaries.
///
/// Set in `e_ident[EI_OSABI]` at build time by xtask.
/// The kernel checks this field during `execve` to determine
/// which personality to use for the new process.
///
/// Value 0xFC is in the OS-specific range (64-255) of the ELF spec,
/// avoiding conflicts with standard ELFOSABI values (NONE=0, Linux=3, etc.).
pub const ELFOSABI_ZIRCON: u8 = 0xFC;

impl Personality {
    /// Detect personality from ELF binary data.
    ///
    /// Checks `e_ident[EI_OSABI]`:
    /// - `ELFOSABI_ZIRCON` (0xFC) → Zircon
    /// - anything else → Linux (default, compatible with standard ELFs)
    pub fn from_elf(data: &[u8]) -> Self {
        if data.len() > EI_OSABI && data[0..4] == ELF_MAGIC && data[EI_OSABI] == ELFOSABI_ZIRCON {
            Personality::Zircon
        } else {
            Personality::Linux
        }
    }
}
