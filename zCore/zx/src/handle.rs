//! RAII handle types.

use crate::Status;
use zircon_abi::consts::ZxHandle;
use zircon_abi::syscall;

/// A raw handle value (0 = invalid).
pub type RawHandle = ZxHandle;

/// An owned Zircon kernel object handle.
///
/// Automatically calls `zx_handle_close` on drop. Use
/// `into_raw()` to take ownership without closing.
#[derive(Debug)]
#[repr(transparent)]
pub struct Handle(RawHandle);

impl Handle {
    /// Create a `Handle` from a raw value.
    ///
    /// # Safety
    /// The caller must ensure the raw handle is valid and not owned
    /// elsewhere.
    pub unsafe fn from_raw(raw: RawHandle) -> Self {
        Handle(raw)
    }

    /// Get the raw handle value without consuming the handle.
    pub fn raw(&self) -> RawHandle {
        self.0
    }

    /// Consume the handle and return the raw value without closing it.
    pub fn into_raw(self) -> RawHandle {
        let raw = self.0;
        core::mem::forget(self);
        raw
    }

    /// Returns true if the handle is invalid (zero).
    pub fn is_invalid(&self) -> bool {
        self.0 == 0
    }

    /// Duplicate this handle with the given rights.
    pub fn duplicate(&self, rights: u32) -> Result<Handle, Status> {
        let mut out: RawHandle = 0;
        Status::ok(unsafe { syscall::zx_handle_duplicate(self.0, rights, &mut out) })?;
        Ok(Handle(out))
    }

    /// Replace this handle with one having the given rights.
    /// Consumes self.
    pub fn replace(self, rights: u32) -> Result<Handle, Status> {
        let mut out: RawHandle = 0;
        let raw = self.into_raw();
        Status::ok(unsafe { syscall::zx_handle_replace(raw, rights, &mut out) })?;
        Ok(Handle(out))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { syscall::zx_handle_close(self.0) };
        }
    }
}

/// A borrowed reference to a handle. Does not close on drop.
#[derive(Debug, Clone, Copy)]
pub struct HandleRef<'a> {
    raw: RawHandle,
    _phantom: core::marker::PhantomData<&'a Handle>,
}

impl<'a> HandleRef<'a> {
    /// Get the raw handle value.
    pub fn raw(&self) -> RawHandle {
        self.raw
    }
}

/// Trait for types that wrap a `Handle`.
pub trait HandleBased: Sized {
    /// Construct from a raw `Handle`.
    fn from_handle(handle: Handle) -> Self;

    /// Get a reference to the inner `Handle`.
    fn handle(&self) -> &Handle;

    /// Consume and return the inner `Handle`.
    fn into_handle(self) -> Handle;

    /// Get the raw handle value.
    fn raw_handle(&self) -> RawHandle {
        self.handle().raw()
    }

    /// Get a borrowed handle reference.
    fn as_handle_ref(&self) -> HandleRef<'_> {
        HandleRef {
            raw: self.handle().raw(),
            _phantom: core::marker::PhantomData,
        }
    }
}

/// Implement `HandleBased` for a newtype wrapping `Handle`.
#[macro_export]
macro_rules! impl_handle_based {
    ($ty:ty) => {
        impl $crate::HandleBased for $ty {
            fn from_handle(handle: $crate::Handle) -> Self {
                Self(handle)
            }

            fn handle(&self) -> &$crate::Handle {
                &self.0
            }

            fn into_handle(self) -> $crate::Handle {
                self.0
            }
        }
    };
}
