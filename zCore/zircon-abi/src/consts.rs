//! Zircon ABI constants.
//!
//! Syscall numbers match `zx-syscall-numbers.h`, VM flags and other
//! constants match the canonical Zircon ABI defined in the Fuchsia
//! repository (`zircon/system/public/zircon/`).

// ── Syscall numbers ─────────────────────────────────────────────────

// Channels
pub const SYS_CHANNEL_CREATE: u32 = 3;
pub const SYS_CHANNEL_READ: u32 = 4;
pub const SYS_CHANNEL_WRITE: u32 = 6;

// Clock
pub const SYS_CLOCK_GET_MONOTONIC_VIA_KERNEL: u32 = 12;
pub const SYS_CLOCK_GET_DETAILS: u32 = 15;

// CPRNG
pub const SYS_CPRNG_DRAW_ONCE: u32 = 17;
pub const SYS_CPRNG_ADD_ENTROPY: u32 = 18;

// Debug
pub const SYS_DEBUG_READ: u32 = 19;
pub const SYS_DEBUG_WRITE: u32 = 20;
pub const SYS_DEBUG_SEND_COMMAND: u32 = 21;
pub const SYS_DEBUGLOG_CREATE: u32 = 22;
pub const SYS_DEBUGLOG_WRITE: u32 = 23;
pub const SYS_DEBUGLOG_READ: u32 = 24;

// Event
pub const SYS_EVENT_CREATE: u32 = 25;
pub const SYS_EVENTPAIR_CREATE: u32 = 26;

// Exception
pub const SYS_EXCEPTION_GET_THREAD: u32 = 27;
pub const SYS_EXCEPTION_GET_PROCESS: u32 = 28;

// FIFO
pub const SYS_FIFO_CREATE: u32 = 29;
pub const SYS_FIFO_READ: u32 = 30;
pub const SYS_FIFO_WRITE: u32 = 31;

// Framebuffer (deprecated in Fuchsia)
pub const SYS_FRAMEBUFFER_GET_INFO: u32 = 32;
pub const SYS_FRAMEBUFFER_SET_RANGE: u32 = 33;

// Futex
pub const SYS_FUTEX_WAIT: u32 = 34;
pub const SYS_FUTEX_WAKE: u32 = 35;
pub const SYS_FUTEX_REQUEUE: u32 = 36;
pub const SYS_FUTEX_WAKE_SINGLE_OWNER: u32 = 37;
pub const SYS_FUTEX_REQUEUE_SINGLE_OWNER: u32 = 38;
pub const SYS_FUTEX_GET_OWNER: u32 = 39;

// Handles
pub const SYS_HANDLE_CLOSE: u32 = 42;
pub const SYS_HANDLE_CLOSE_MANY: u32 = 43;
pub const SYS_HANDLE_DUPLICATE: u32 = 44;
pub const SYS_HANDLE_REPLACE: u32 = 45;

// Interrupt
pub const SYS_INTERRUPT_BIND_VCPU: u32 = 52;

// I/O ports
pub const SYS_IOPORTS_RELEASE: u32 = 55;

// Job
pub const SYS_JOB_CREATE: u32 = 56;
pub const SYS_JOB_SET_POLICY: u32 = 57;

// Nanosleep
pub const SYS_NANOSLEEP: u32 = 62;

// Ticks
pub const SYS_TICKS_GET_VIA_KERNEL: u32 = 63;

// MSI
pub const SYS_MSI_ALLOCATE: u32 = 64;
pub const SYS_MSI_CREATE: u32 = 65;

// Mtrace
pub const SYS_MTRACE_CONTROL: u32 = 66;

// Objects
pub const SYS_OBJECT_WAIT_ONE: u32 = 67;
pub const SYS_OBJECT_WAIT_MANY: u32 = 68;
pub const SYS_OBJECT_WAIT_ASYNC: u32 = 69;
pub const SYS_OBJECT_SIGNAL: u32 = 70;
pub const SYS_OBJECT_SIGNAL_PEER: u32 = 71;
pub const SYS_OBJECT_GET_PROPERTY: u32 = 72;
pub const SYS_OBJECT_SET_PROPERTY: u32 = 73;
pub const SYS_OBJECT_GET_INFO: u32 = 74;
pub const SYS_OBJECT_GET_CHILD: u32 = 75;
pub const SYS_OBJECT_SET_PROFILE: u32 = 76;

// PCI
pub const SYS_PCI_RESET_DEVICE: u32 = 85;

// Port
pub const SYS_PORT_CREATE: u32 = 96;
pub const SYS_PORT_QUEUE: u32 = 97;
pub const SYS_PORT_WAIT: u32 = 98;

// Process/Thread
pub const SYS_PROCESS_EXIT: u32 = 100;
pub const SYS_PROCESS_CREATE: u32 = 101;
pub const SYS_PROCESS_START: u32 = 102;
pub const SYS_PROCESS_READ_MEMORY: u32 = 103;
pub const SYS_PROCESS_WRITE_MEMORY: u32 = 104;

// Profile
pub const SYS_PROFILE_CREATE: u32 = 105;

// Resource
pub const SYS_RESOURCE_CREATE: u32 = 106;

// SMC
pub const SYS_SMC_CALL: u32 = 107;

// Socket
pub const SYS_SOCKET_CREATE: u32 = 108;
pub const SYS_SOCKET_WRITE: u32 = 109;
pub const SYS_SOCKET_READ: u32 = 110;
pub const SYS_SOCKET_SHUTDOWN: u32 = 111;

// Stream
pub const SYS_STREAM_CREATE: u32 = 112;
pub const SYS_STREAM_WRITEV: u32 = 113;
pub const SYS_STREAM_WRITEV_AT: u32 = 114;
pub const SYS_STREAM_READV: u32 = 115;
pub const SYS_STREAM_READV_AT: u32 = 116;
pub const SYS_STREAM_SEEK: u32 = 117;

// System
pub const SYS_SYSTEM_GET_EVENT: u32 = 129;
pub const SYS_SYSTEM_MEXEC: u32 = 130;
pub const SYS_SYSTEM_MEXEC_PAYLOAD_GET: u32 = 131;
pub const SYS_SYSTEM_POWERCTL: u32 = 132;

// Task
pub const SYS_TASK_SUSPEND_TOKEN: u32 = 134;
pub const SYS_TASK_CREATE_EXCEPTION_CHANNEL: u32 = 135;
pub const SYS_TASK_KILL: u32 = 136;

pub const SYS_THREAD_EXIT: u32 = 137;
pub const SYS_THREAD_CREATE: u32 = 138;
pub const SYS_THREAD_START: u32 = 139;

// Timer
pub const SYS_TIMER_CREATE: u32 = 142;
pub const SYS_TIMER_SET: u32 = 143;
pub const SYS_TIMER_CANCEL: u32 = 144;

// VMAR
pub const SYS_VMAR_ALLOCATE: u32 = 150;
pub const SYS_VMAR_DESTROY: u32 = 151;
pub const SYS_VMAR_MAP: u32 = 152;
pub const SYS_VMAR_UNMAP: u32 = 153;
pub const SYS_VMAR_PROTECT: u32 = 154;
pub const SYS_VMAR_OP_RANGE: u32 = 155;

// VMO
pub const SYS_VMO_CREATE: u32 = 156;
pub const SYS_VMO_READ: u32 = 157;
pub const SYS_VMO_WRITE: u32 = 158;
pub const SYS_VMO_GET_SIZE: u32 = 159;
pub const SYS_VMO_SET_SIZE: u32 = 160;
pub const SYS_VMO_OP_RANGE: u32 = 161;
pub const SYS_VMO_CREATE_CHILD: u32 = 162;
pub const SYS_VMO_SET_CACHE_POLICY: u32 = 163;
pub const SYS_VMO_REPLACE_AS_EXECUTABLE: u32 = 164;

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
