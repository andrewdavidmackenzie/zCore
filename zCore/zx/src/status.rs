//! Zircon status code wrapper.

use zircon_abi::errors::{ZxStatus, ZX_OK};

/// A Zircon status code, used as the error type in `Result<T, Status>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status(pub ZxStatus);

impl Status {
    /// Success.
    pub const OK: Status = Status(ZX_OK);

    /// Convert a raw status into a `Result`.
    ///
    /// `ZX_OK` (0) maps to `Ok(())`, any other value maps to `Err(Status)`.
    pub fn ok(raw: ZxStatus) -> Result<(), Status> {
        if raw == ZX_OK {
            Ok(())
        } else {
            Err(Status(raw))
        }
    }

    /// Get the raw `ZxStatus` value.
    pub fn raw(self) -> ZxStatus {
        self.0
    }
}

impl core::fmt::Display for Status {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ZxError({})", self.0)
    }
}
