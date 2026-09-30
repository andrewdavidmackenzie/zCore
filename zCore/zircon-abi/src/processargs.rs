//! Fuchsia processargs protocol definitions.
//!
//! When a Zircon process is created, the kernel sends bootstrap handles
//! and metadata to the new process via a channel message in the
//! `zx_proc_args_t` wire format defined in `zircon/processargs.h`.
//!
//! This module provides:
//! - The [`ZxProcArgs`] header struct
//! - `PA_*` handle-type constants
//! - The [`pa_hnd`] encoding helper
//! - A no-alloc [`ProcessargsParser`] for extracting handles by type

/// Processargs protocol magic number.
pub const ZX_PROCARGS_PROTOCOL: u32 = 0x4150_585d;

/// Processargs protocol version.
pub const ZX_PROCARGS_VERSION: u32 = 0x0000_1000;

// ── Handle type constants (from zircon/processargs.h) ──────────────

/// Process's own handle.
pub const PA_PROC_SELF: u32 = 0x01;
/// Main thread handle.
pub const PA_THREAD_SELF: u32 = 0x02;
/// Default job for the process.
pub const PA_JOB_DEFAULT: u32 = 0x03;
/// Root VMAR handle.
pub const PA_VMAR_ROOT: u32 = 0x04;
/// VMAR where the ELF image was loaded.
pub const PA_VMAR_LOADED: u32 = 0x05;
/// Loader-service channel (dynamic linking).
pub const PA_LDSVC_LOADER: u32 = 0x10;
/// vDSO VMO.
pub const PA_VMO_VDSO: u32 = 0x11;
/// Original executable VMO (when using interpreter).
pub const PA_VMO_EXECUTABLE: u32 = 0x14;
/// Boot data VMO (ZBI, boot-options, etc.).
pub const PA_VMO_BOOTDATA: u32 = 0x1A;
/// Root resource handle.
pub const PA_RESOURCE: u32 = 0x3F;
/// MMIO sub-resource.
pub const PA_MMIO_RESOURCE: u32 = 0x50;
/// IRQ sub-resource.
pub const PA_IRQ_RESOURCE: u32 = 0x51;
/// System sub-resource.
pub const PA_SYSTEM_RESOURCE: u32 = 0x54;
/// UTC clock handle.
pub const PA_CLOCK_UTC: u32 = 0x56;

// ── Wire format ────────────────────────────────────────────────────

/// Encode a handle type and argument into a `handle_info` entry.
///
/// The lower 16 bits are the type, the upper 16 bits are the argument.
pub const fn pa_hnd(handle_type: u32, arg: u32) -> u32 {
    (handle_type & 0xFFFF) | ((arg & 0xFFFF) << 16)
}

/// Extract the type from a `handle_info` entry.
pub const fn pa_hnd_type(handle_info: u32) -> u32 {
    handle_info & 0xFFFF
}

/// Extract the argument from a `handle_info` entry.
pub const fn pa_hnd_arg(handle_info: u32) -> u32 {
    (handle_info >> 16) & 0xFFFF
}

/// Processargs message header.
///
/// This is the wire format of `zx_proc_args_t` from
/// `zircon/processargs.h`.  The struct is written directly to the
/// channel message data — no serialisation layer.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ZxProcArgs {
    /// Must be [`ZX_PROCARGS_PROTOCOL`].
    pub protocol: u32,
    /// Must be [`ZX_PROCARGS_VERSION`].
    pub version: u32,
    /// Byte offset of the `handle_info` `u32` array within the message.
    pub handle_info_off: u32,
    /// Byte offset of the NUL-separated argv strings.
    pub args_off: u32,
    /// Number of argv entries.
    pub args_num: u32,
    /// Byte offset of the NUL-separated environ strings.
    pub environ_off: u32,
    /// Number of environ entries.
    pub environ_num: u32,
    /// Byte offset of the NUL-separated name strings.
    pub names_off: u32,
    /// Number of name entries.
    pub names_num: u32,
}

// Compile-time check: must be exactly 36 bytes with no padding.
const _: () = assert!(core::mem::size_of::<ZxProcArgs>() == 36);

impl ZxProcArgs {
    /// View the struct as raw bytes for wire-format serialisation.
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self as *const Self as *const u8,
                core::mem::size_of::<Self>(),
            )
        }
    }

    /// Parse a `ZxProcArgs` header from raw message bytes.
    ///
    /// Returns `None` if the buffer is too small or the protocol/version
    /// magic doesn't match.
    pub fn from_bytes(data: &[u8]) -> Option<&Self> {
        if data.len() < core::mem::size_of::<Self>() {
            return None;
        }
        let header = unsafe { &*(data.as_ptr() as *const Self) };
        if header.protocol != ZX_PROCARGS_PROTOCOL || header.version != ZX_PROCARGS_VERSION {
            return None;
        }
        Some(header)
    }

    /// Return the handle_info slice from a message data buffer.
    ///
    /// Each entry is a `u32` produced by [`pa_hnd`].  The number of
    /// entries equals the number of handles in the message.
    pub fn handle_info<'a>(&self, data: &'a [u8], num_handles: usize) -> Option<&'a [u32]> {
        let off = self.handle_info_off as usize;
        let end = off + num_handles * 4;
        if end > data.len() {
            return None;
        }
        // Safety: u32 alignment is guaranteed by the 4-byte-aligned offset
        // in the processargs spec, and we verified bounds above.
        let ptr = unsafe { data.as_ptr().add(off) as *const u32 };
        Some(unsafe { core::slice::from_raw_parts(ptr, num_handles) })
    }

    /// Find the handle index for a given type tag.
    ///
    /// Scans the `handle_info` array for the first entry whose type
    /// matches `handle_type` (ignoring the argument field).  Returns
    /// the index into the handles array.
    pub fn find_handle(&self, data: &[u8], num_handles: usize, handle_type: u32) -> Option<usize> {
        let info = self.handle_info(data, num_handles)?;
        info.iter()
            .position(|&entry| pa_hnd_type(entry) == handle_type)
    }
}

/// Build a processargs message data buffer (requires alloc).
///
/// Wire format: `[ZxProcArgs header (36 bytes)][handle_info u32 array][argv]`
///
/// `argv` is NUL-separated and NUL-terminated (e.g. `"arg1\0arg2\0"`).
/// Pass an empty slice if there are no arguments.
#[cfg(feature = "alloc")]
pub fn build_message(handle_info: &[u32], argv: &[u8]) -> alloc::vec::Vec<u8> {
    let header_size = core::mem::size_of::<ZxProcArgs>();
    let handle_info_off = header_size as u32;
    let args_off = handle_info_off + (handle_info.len() as u32) * 4;
    let args_num = if argv.is_empty() {
        0u32
    } else {
        argv.iter().filter(|&&b| b == 0).count() as u32
    };
    let total_size = (args_off as usize) + argv.len();

    let header = ZxProcArgs {
        protocol: ZX_PROCARGS_PROTOCOL,
        version: ZX_PROCARGS_VERSION,
        handle_info_off,
        args_off,
        args_num,
        environ_off: total_size as u32,
        environ_num: 0,
        names_off: total_size as u32,
        names_num: 0,
    };

    let handle_info_bytes = unsafe {
        core::slice::from_raw_parts(handle_info.as_ptr().cast::<u8>(), handle_info.len() * 4)
    };
    [header.as_bytes(), handle_info_bytes, argv].concat()
}
