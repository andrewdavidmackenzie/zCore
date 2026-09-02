//! Zircon syscall number constants.
//!
//! These match the values in `zx-syscall-numbers.h` from the Zircon kernel
//! source tree (under `zircon/` in the Fuchsia repository).

// Channels
pub const SYS_CHANNEL_CREATE: u32 = 3;
pub const SYS_CHANNEL_READ: u32 = 4;
pub const SYS_CHANNEL_WRITE: u32 = 6;

// Debug
pub const SYS_DEBUG_WRITE: u32 = 20;
pub const SYS_DEBUGLOG_CREATE: u32 = 22;
pub const SYS_DEBUGLOG_WRITE: u32 = 23;

// Handles
pub const SYS_HANDLE_CLOSE: u32 = 42;
pub const SYS_HANDLE_DUPLICATE: u32 = 44;

// Objects
pub const SYS_OBJECT_WAIT_ONE: u32 = 67;
pub const SYS_OBJECT_SIGNAL: u32 = 70;
pub const SYS_OBJECT_GET_INFO: u32 = 74;

// Process/Thread
pub const SYS_PROCESS_EXIT: u32 = 100;
pub const SYS_PROCESS_CREATE: u32 = 101;
pub const SYS_PROCESS_START: u32 = 102;
pub const SYS_THREAD_EXIT: u32 = 137;
pub const SYS_THREAD_CREATE: u32 = 138;
pub const SYS_THREAD_START: u32 = 139;

// VMO
pub const SYS_VMO_CREATE: u32 = 156;
pub const SYS_VMO_READ: u32 = 157;
pub const SYS_VMO_WRITE: u32 = 158;
pub const SYS_VMO_GET_SIZE: u32 = 159;

// VMAR
pub const SYS_VMAR_ALLOCATE: u32 = 150;
pub const SYS_VMAR_MAP: u32 = 152;
pub const SYS_VMAR_UNMAP: u32 = 153;
pub const SYS_VMAR_PROTECT: u32 = 154;

// Futex
pub const SYS_FUTEX_WAIT: u32 = 34;
pub const SYS_FUTEX_WAKE: u32 = 35;

// Port
pub const SYS_PORT_CREATE: u32 = 96;
pub const SYS_PORT_WAIT: u32 = 98;

// Timer
pub const SYS_TIMER_CREATE: u32 = 142;
pub const SYS_NANOSLEEP: u32 = 62;

// VMAR map option flags (matches zx_vm_option_t)
pub const ZX_VM_PERM_READ: u32 = 1 << 0;
pub const ZX_VM_PERM_WRITE: u32 = 1 << 1;
pub const ZX_VM_PERM_EXECUTE: u32 = 1 << 3;
pub const ZX_VM_SPECIFIC: u32 = 1 << 4;
pub const ZX_VM_CAN_MAP_READ: u32 = 1 << 17;
pub const ZX_VM_CAN_MAP_WRITE: u32 = 1 << 18;
pub const ZX_VM_CAN_MAP_EXECUTE: u32 = 1 << 19;
