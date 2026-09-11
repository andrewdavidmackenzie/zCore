//! Zircon ABI struct definitions.
//!
//! These match the C struct layouts defined in Fuchsia's
//! `zircon/system/public/zircon/types.h` and related headers.

use crate::consts::{ZxKoid, ZxRights, ZxSignals};
use crate::errors::ZxStatus;

/// Handle value type (alias for clarity in struct fields).
pub type HandleValue = u32;

// ── Channel call arguments ──────────────────────────────────────────

/// Arguments for `zx_channel_call` (`zx_channel_call_args_t`).
#[repr(C)]
#[derive(Debug)]
pub struct ChannelCallArgs {
    /// Pointer to bytes to write.
    pub wr_bytes: *const u8,
    /// Pointer to handles to write.
    pub wr_handles: *const HandleValue,
    /// Pointer to buffer for read bytes.
    pub rd_bytes: *mut u8,
    /// Pointer to buffer for read handles.
    pub rd_handles: *mut HandleValue,
    /// Number of bytes to write.
    pub wr_num_bytes: u32,
    /// Number of handles to write.
    pub wr_num_handles: u32,
    /// Size of read bytes buffer.
    pub rd_num_bytes: u32,
    /// Size of read handles buffer.
    pub rd_num_handles: u32,
}

// ── Handle info structs ─────────────────────────────────────────────

/// Information about a handle and its object (`zx_info_handle_basic_t`).
#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct HandleBasicInfo {
    /// The kernel object ID of the object this handle refers to.
    pub koid: ZxKoid,
    /// The rights associated with this handle.
    pub rights: ZxRights,
    /// The object type (e.g., Channel=4, Process=1).
    pub obj_type: u32,
    /// The kernel object ID of the related object (e.g., other end of channel).
    pub related_koid: ZxKoid,
    /// Properties (1 if handle has WAIT right, 0 otherwise).
    pub props: u32,
    /// Padding to align struct.
    pub padding: u32,
}

/// Handle info returned by `zx_channel_read_etc` (`zx_handle_info_t`).
#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct HandleInfo {
    /// The handle value.
    pub handle: HandleValue,
    /// The object type.
    pub obj_type: u32,
    /// The rights on the handle.
    pub rights: ZxRights,
    /// Reserved, must be zero.
    pub unused: u32,
}

/// Handle disposition for `zx_channel_write_etc` (`zx_handle_disposition_t`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HandleDisposition {
    /// Operation to perform (e.g., move or duplicate).
    pub op: u32,
    /// The handle to operate on.
    pub handle: HandleValue,
    /// Expected object type (0 for any).
    pub type_: u32,
    /// Desired rights (0 for same rights).
    pub rights: ZxRights,
    /// Result status (set by kernel on return).
    pub result: ZxStatus,
}

// ── Wait item ───────────────────────────────────────────────────────

/// An item for `zx_object_wait_many` (`zx_wait_item_t`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WaitItem {
    /// Handle to wait on.
    pub handle: HandleValue,
    /// Signals to wait for.
    pub waitfor: ZxSignals,
    /// Signals that were observed (output).
    pub pending: ZxSignals,
}

// ── Port packet ─────────────────────────────────────────────────────

/// A port packet (`zx_port_packet_t`).
///
/// This is an opaque 48-byte structure matching the Zircon ABI layout.
/// The internal union fields depend on the packet type.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PortPacket {
    /// Key associated with the packet.
    pub key: u64,
    /// Packet type.
    pub type_: u32,
    /// Status code.
    pub status: ZxStatus,
    /// Payload data (type-dependent, 32 bytes).
    pub data: [u8; 32],
}

// ── Info structs ────────────────────────────────────────────────────

/// Process information (`zx_info_process_t`).
#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct ProcessInfo {
    /// The return code of the process (valid if `has_exited` is true).
    pub return_code: i64,
    /// Whether the process has been started.
    pub started: bool,
    /// Whether the process has exited.
    pub has_exited: bool,
    /// Whether a debugger is attached.
    pub debugger_attached: bool,
    /// Padding to 16 bytes.
    pub padding1: [u8; 5],
}

/// Thread information (`zx_info_thread_t`).
#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct ThreadInfo {
    /// Thread state (e.g., running, blocked, etc.).
    pub state: u32,
    /// Exception channel type the thread is waiting in.
    pub wait_exception_channel_type: u32,
    /// CPU affinity mask.
    pub cpu_affinity_mask: [u64; 8],
}

// ── Exception report ────────────────────────────────────────────────

/// Exception header within an exception report.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ExceptionHeader {
    /// Size of the exception report.
    pub size: u32,
    /// Exception type.
    pub type_: u32,
}

/// Architecture-specific exception context.
///
/// This is a 24-byte opaque blob whose interpretation depends on the
/// architecture.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ExceptionContext {
    /// Architecture-dependent context data (24 bytes).
    pub arch: [u8; 24],
}

/// Full exception report (`zx_exception_report_t`).
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ExceptionReport {
    /// Exception header.
    pub header: ExceptionHeader,
    /// Architecture-specific exception context.
    pub context: ExceptionContext,
}
