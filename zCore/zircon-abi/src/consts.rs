//! Zircon ABI constants.
//!
//! Syscall numbers are auto-generated from `zx-syscall-numbers.h` by
//! `build.rs` so they stay in sync with the kernel dispatcher.
//! VM flags and other constants match the canonical Zircon ABI defined
//! in the Fuchsia repository (`zircon/system/public/zircon/`).

// ── Syscall numbers (auto-generated from zx-syscall-numbers.h) ──────
include!(concat!(env!("OUT_DIR"), "/syscall_numbers.rs"));

// ── Type aliases ────────────────────────────────────────────────────

/// Raw handle value (matches `zx_handle_t`).
pub type ZxHandle = u32;
/// Kernel object ID (matches `zx_koid_t`).
pub type ZxKoid = u64;
/// Virtual address (matches `zx_vaddr_t`).
pub type ZxVaddr = usize;
/// Physical address (matches `zx_paddr_t`).
pub type ZxPaddr = usize;
/// Rights bitmask (matches `zx_rights_t`).
pub type ZxRights = u32;
/// Signals bitmask (matches `zx_signals_t`).
pub type ZxSignals = u32;

// ── Rights ──────────────────────────────────────────────────────────

pub const ZX_RIGHT_NONE: ZxRights = 0;
pub const ZX_RIGHT_DUPLICATE: ZxRights = 1 << 0;
pub const ZX_RIGHT_TRANSFER: ZxRights = 1 << 1;
pub const ZX_RIGHT_READ: ZxRights = 1 << 2;
pub const ZX_RIGHT_WRITE: ZxRights = 1 << 3;
pub const ZX_RIGHT_EXECUTE: ZxRights = 1 << 4;
pub const ZX_RIGHT_MAP: ZxRights = 1 << 5;
pub const ZX_RIGHT_GET_PROPERTY: ZxRights = 1 << 6;
pub const ZX_RIGHT_SET_PROPERTY: ZxRights = 1 << 7;
pub const ZX_RIGHT_ENUMERATE: ZxRights = 1 << 8;
pub const ZX_RIGHT_DESTROY: ZxRights = 1 << 9;
pub const ZX_RIGHT_SET_POLICY: ZxRights = 1 << 10;
pub const ZX_RIGHT_GET_POLICY: ZxRights = 1 << 11;
pub const ZX_RIGHT_SIGNAL: ZxRights = 1 << 12;
pub const ZX_RIGHT_SIGNAL_PEER: ZxRights = 1 << 13;
pub const ZX_RIGHT_WAIT: ZxRights = 1 << 14;
pub const ZX_RIGHT_INSPECT: ZxRights = 1 << 15;
pub const ZX_RIGHT_MANAGE_JOB: ZxRights = 1 << 16;
pub const ZX_RIGHT_MANAGE_PROCESS: ZxRights = 1 << 17;
pub const ZX_RIGHT_MANAGE_THREAD: ZxRights = 1 << 18;
pub const ZX_RIGHT_APPLY_PROFILE: ZxRights = 1 << 19;
pub const ZX_RIGHT_MANAGE_SOCKET: ZxRights = 1 << 20;
pub const ZX_RIGHT_OP_CHILDREN: ZxRights = 1 << 22;
pub const ZX_RIGHT_RESIZE: ZxRights = 1 << 23;
pub const ZX_RIGHT_ATTACH_VMO: ZxRights = 1 << 24;
pub const ZX_RIGHT_MANAGE_VMO: ZxRights = 1 << 25;
pub const ZX_RIGHT_SAME_RIGHTS: ZxRights = 1 << 31;

/// Default rights for most objects.
pub const ZX_DEFAULT_CHANNEL_RIGHTS: ZxRights = ZX_RIGHT_TRANSFER
    | ZX_RIGHT_READ
    | ZX_RIGHT_WRITE
    | ZX_RIGHT_SIGNAL
    | ZX_RIGHT_SIGNAL_PEER
    | ZX_RIGHT_WAIT
    | ZX_RIGHT_INSPECT;

// ── Signals ─────────────────────────────────────────────────────────

// Generic object signals (bits 0-7 have per-type meaning)
pub const ZX_SIGNAL_NONE: ZxSignals = 0;

// User signals (available for all objects)
pub const ZX_USER_SIGNAL_0: ZxSignals = 1 << 24;
pub const ZX_USER_SIGNAL_1: ZxSignals = 1 << 25;
pub const ZX_USER_SIGNAL_2: ZxSignals = 1 << 26;
pub const ZX_USER_SIGNAL_3: ZxSignals = 1 << 27;
pub const ZX_USER_SIGNAL_4: ZxSignals = 1 << 28;
pub const ZX_USER_SIGNAL_5: ZxSignals = 1 << 29;
pub const ZX_USER_SIGNAL_6: ZxSignals = 1 << 30;
pub const ZX_USER_SIGNAL_7: ZxSignals = 1 << 31;

// Object lifecycle
/// Object handle has been closed (peer for paired objects).
pub const ZX_SIGNAL_HANDLE_CLOSED: ZxSignals = 1 << 23;

// Process/Thread signals
/// Process or thread has terminated.
pub const ZX_PROCESS_TERMINATED: ZxSignals = 1 << 3;
pub const ZX_THREAD_TERMINATED: ZxSignals = 1 << 3;
pub const ZX_THREAD_RUNNING: ZxSignals = 1 << 4;
pub const ZX_THREAD_SUSPENDED: ZxSignals = 1 << 5;

// Job signals
pub const ZX_JOB_TERMINATED: ZxSignals = 1 << 3;
pub const ZX_JOB_NO_JOBS: ZxSignals = 1 << 4;
pub const ZX_JOB_NO_PROCESSES: ZxSignals = 1 << 5;
pub const ZX_JOB_NO_CHILDREN: ZxSignals = 1 << 6;

// Task signals (shared by Job, Process, Thread)
pub const ZX_TASK_TERMINATED: ZxSignals = 1 << 3;

// Channel signals
pub const ZX_CHANNEL_READABLE: ZxSignals = 1 << 0;
pub const ZX_CHANNEL_WRITABLE: ZxSignals = 1 << 1;
pub const ZX_CHANNEL_PEER_CLOSED: ZxSignals = 1 << 2;

// Socket signals
pub const ZX_SOCKET_READABLE: ZxSignals = 1 << 0;
pub const ZX_SOCKET_WRITABLE: ZxSignals = 1 << 1;
pub const ZX_SOCKET_PEER_CLOSED: ZxSignals = 1 << 2;
pub const ZX_SOCKET_PEER_WRITE_DISABLED: ZxSignals = 1 << 4;
pub const ZX_SOCKET_WRITE_DISABLED: ZxSignals = 1 << 5;
pub const ZX_SOCKET_READ_THRESHOLD: ZxSignals = 1 << 10;
pub const ZX_SOCKET_WRITE_THRESHOLD: ZxSignals = 1 << 11;

// Port signals
pub const ZX_PORT_READABLE: ZxSignals = 1 << 0; // ZX_READABLE alias

// Timer signals
pub const ZX_TIMER_SIGNALED: ZxSignals = 1 << 3;

// Event signals
pub const ZX_EVENT_SIGNALED: ZxSignals = 1 << 3;
pub const ZX_EVENTPAIR_SIGNALED: ZxSignals = 1 << 3;
pub const ZX_EVENTPAIR_PEER_CLOSED: ZxSignals = 1 << 2;

// FIFO signals
pub const ZX_FIFO_READABLE: ZxSignals = 1 << 0;
pub const ZX_FIFO_WRITABLE: ZxSignals = 1 << 1;
pub const ZX_FIFO_PEER_CLOSED: ZxSignals = 1 << 2;

// ── VMAR / VM option flags ──────────────────────────────────────────
//
// These match Fuchsia's canonical zx_vm_option_t bit positions.

#[allow(clippy::identity_op)]
pub const ZX_VM_PERM_READ: u32 = 1 << 0;
pub const ZX_VM_PERM_WRITE: u32 = 1 << 1;
pub const ZX_VM_PERM_EXECUTE: u32 = 1 << 2;
pub const ZX_VM_COMPACT: u32 = 1 << 3;
pub const ZX_VM_SPECIFIC: u32 = 1 << 4;
pub const ZX_VM_SPECIFIC_OVERWRITE: u32 = 1 << 5;
pub const ZX_VM_CAN_MAP_SPECIFIC: u32 = 1 << 6;
pub const ZX_VM_CAN_MAP_READ: u32 = 1 << 7;
pub const ZX_VM_CAN_MAP_WRITE: u32 = 1 << 8;
pub const ZX_VM_CAN_MAP_EXECUTE: u32 = 1 << 9;
pub const ZX_VM_MAP_RANGE: u32 = 1 << 10;
pub const ZX_VM_REQUIRE_NON_RESIZABLE: u32 = 1 << 11;
pub const ZX_VM_ALLOW_FAULTS: u32 = 1 << 12;
