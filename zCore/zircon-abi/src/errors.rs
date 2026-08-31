//! Zircon status/error codes.
//!
//! These match `zx_status_t` values defined by the Zircon kernel.

/// Zircon status code type (matches `zx_status_t`).
pub type ZxStatus = i32;

pub const ZX_OK: ZxStatus = 0;
pub const ZX_ERR_INTERNAL: ZxStatus = -1;
pub const ZX_ERR_NOT_SUPPORTED: ZxStatus = -2;
pub const ZX_ERR_NO_RESOURCES: ZxStatus = -3;
pub const ZX_ERR_NO_MEMORY: ZxStatus = -4;
pub const ZX_ERR_INVALID_ARGS: ZxStatus = -10;
pub const ZX_ERR_BAD_HANDLE: ZxStatus = -11;
pub const ZX_ERR_WRONG_TYPE: ZxStatus = -12;
pub const ZX_ERR_BAD_SYSCALL: ZxStatus = -13;
pub const ZX_ERR_OUT_OF_RANGE: ZxStatus = -14;
pub const ZX_ERR_BUFFER_TOO_SMALL: ZxStatus = -15;
pub const ZX_ERR_BAD_STATE: ZxStatus = -20;
pub const ZX_ERR_TIMED_OUT: ZxStatus = -21;
pub const ZX_ERR_SHOULD_WAIT: ZxStatus = -22;
pub const ZX_ERR_CANCELED: ZxStatus = -23;
pub const ZX_ERR_PEER_CLOSED: ZxStatus = -24;
pub const ZX_ERR_NOT_FOUND: ZxStatus = -25;
pub const ZX_ERR_ALREADY_EXISTS: ZxStatus = -26;
pub const ZX_ERR_ALREADY_BOUND: ZxStatus = -27;
pub const ZX_ERR_UNAVAILABLE: ZxStatus = -28;
pub const ZX_ERR_ACCESS_DENIED: ZxStatus = -30;
pub const ZX_ERR_IO: ZxStatus = -40;
pub const ZX_ERR_IO_REFUSED: ZxStatus = -41;
pub const ZX_ERR_IO_DATA_INTEGRITY: ZxStatus = -42;
pub const ZX_ERR_IO_DATA_LOSS: ZxStatus = -43;
pub const ZX_ERR_IO_NOT_PRESENT: ZxStatus = -44;
pub const ZX_ERR_IO_OVERRUN: ZxStatus = -45;
pub const ZX_ERR_IO_MISSED_DEADLINE: ZxStatus = -46;
pub const ZX_ERR_IO_INVALID: ZxStatus = -47;
