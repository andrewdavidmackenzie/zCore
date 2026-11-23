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
/// Kernel file VMO (named files like "boot-options.txt").
pub const PA_VMO_KERNEL_FILE: u32 = 0x1C;
/// Root resource handle.
pub const PA_RESOURCE: u32 = 0x3F;
/// MMIO sub-resource.
pub const PA_MMIO_RESOURCE: u32 = 0x50;
/// IRQ sub-resource.
pub const PA_IRQ_RESOURCE: u32 = 0x51;
/// I/O port sub-resource (x86-only).
pub const PA_IOPORT_RESOURCE: u32 = 0x52;
/// System sub-resource.
pub const PA_SYSTEM_RESOURCE: u32 = 0x54;
/// UTC clock handle.
pub const PA_CLOCK_UTC: u32 = 0x40;

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
    /// Copies the header from the byte buffer to avoid unaligned access.
    /// Returns `None` if the buffer is too small or the protocol/version
    /// magic doesn't match.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let size = core::mem::size_of::<Self>();
        if data.len() < size {
            return None;
        }
        // Copy to a properly aligned local to avoid unaligned reads.
        let mut header = Self {
            protocol: 0,
            version: 0,
            handle_info_off: 0,
            args_off: 0,
            args_num: 0,
            environ_off: 0,
            environ_num: 0,
            names_off: 0,
            names_num: 0,
        };
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                &mut header as *mut Self as *mut u8,
                size,
            );
        }
        if header.protocol != ZX_PROCARGS_PROTOCOL || header.version != ZX_PROCARGS_VERSION {
            return None;
        }
        Some(header)
    }

    /// Read a single `handle_info` entry from the message data.
    ///
    /// Returns the `u32` entry at position `index`, decoded from raw
    /// bytes to avoid alignment issues.
    fn read_handle_info_entry(&self, data: &[u8], index: usize) -> Option<u32> {
        let off = (self.handle_info_off as usize).checked_add(index.checked_mul(4)?)?;
        let end = off.checked_add(4)?;
        if end > data.len() {
            return None;
        }
        Some(u32::from_ne_bytes(data[off..end].try_into().ok()?))
    }

    /// Find the handle index for a given type tag.
    ///
    /// Scans the `handle_info` entries for the first one whose type
    /// matches `handle_type` (ignoring the argument field).  Returns
    /// the index into the handles array.
    pub fn find_handle(&self, data: &[u8], num_handles: usize, handle_type: u32) -> Option<usize> {
        for i in 0..num_handles {
            if let Some(entry) = self.read_handle_info_entry(data, i) {
                if pa_hnd_type(entry) == handle_type {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Get the `i`-th argument (argv[i]).
    ///
    /// Per the processargs protocol, arguments are NUL-terminated
    /// strings packed at `args_off` in the message data; `args_num`
    /// counts them. argv[0] is conventionally the program name/path
    /// as invoked — Fuchsia's launchers and userboot both guarantee
    /// this.
    pub fn arg<'a>(&self, data: &'a [u8], i: u32) -> Option<&'a [u8]> {
        if i >= self.args_num {
            return None;
        }
        let mut off = self.args_off as usize;
        let mut idx = 0u32;
        while off < data.len() {
            let nul = off + data[off..].iter().position(|&b| b == 0)?;
            if idx == i {
                return Some(&data[off..nul]);
            }
            idx += 1;
            off = nul + 1;
        }
        None
    }
}

/// Write a processargs message into `buf` (no allocation).
///
/// Wire format: `[ZxProcArgs header (36 bytes)][handle_info u32 array][argv]`
///
/// `argv` is NUL-separated and NUL-terminated (e.g. `"arg1\0arg2\0"`).
/// Pass an empty slice if there are no arguments.
///
/// Returns the number of bytes written, or 0 if `buf` is too small
/// (callers control buffer sizes; a silent truncation would corrupt
/// the message). This is the single definition of the processargs
/// wire format for writers — kernel-side launchers and the userspace
/// `userstart` both use it (see #884).
pub fn write_message(handle_info: &[u32], argv: &[u8], buf: &mut [u8]) -> usize {
    let header_size = core::mem::size_of::<ZxProcArgs>();
    let handle_info_off = header_size as u32;
    let args_off = handle_info_off + (handle_info.len() as u32) * 4;
    let args_num = if argv.is_empty() {
        0u32
    } else {
        argv.iter().filter(|&&b| b == 0).count() as u32
    };
    let total_size = (args_off as usize) + argv.len();

    if buf.len() < total_size {
        return 0;
    }

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
    buf[..header_size].copy_from_slice(header.as_bytes());
    let handle_info_bytes = unsafe {
        core::slice::from_raw_parts(handle_info.as_ptr().cast::<u8>(), handle_info.len() * 4)
    };
    buf[header_size..total_size - argv.len()].copy_from_slice(handle_info_bytes);
    buf[total_size - argv.len()..total_size].copy_from_slice(argv);
    total_size
}

/// Build a processargs message data buffer (requires alloc).
///
/// Wire format: `[ZxProcArgs header (36 bytes)][handle_info u32 array][argv]`
///
/// `argv` is NUL-separated and NUL-terminated (e.g. `"arg1\0arg2\0"`).
/// Pass an empty slice if there are no arguments.
#[cfg(feature = "alloc")]
pub fn build_message(handle_info: &[u32], argv: &[u8]) -> alloc::vec::Vec<u8> {
    let mut buf = alloc::vec![0u8; write_message_size(handle_info, argv)];
    let len = write_message(handle_info, argv, &mut buf);
    debug_assert_eq!(len, buf.len());
    buf
}

/// The exact buffer size `write_message` needs for these inputs.
pub fn write_message_size(handle_info: &[u32], argv: &[u8]) -> usize {
    core::mem::size_of::<ZxProcArgs>() + handle_info.len() * 4 + argv.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: write_message → parse → find_handle / arg.
    #[test]
    fn write_message_round_trip() {
        let handle_info = [
            pa_hnd(PA_PROC_SELF, 0),
            pa_hnd(PA_VMO_VDSO, 0),
            pa_hnd(PA_JOB_DEFAULT, 0),
        ];
        let argv = b"bin/hello\0--flag\0";
        let mut buf = [0u8; 256];
        let len = write_message(&handle_info, argv, &mut buf);
        assert_eq!(len, write_message_size(&handle_info, argv));
        assert!(len > 0);

        let msg = &buf[..len];
        let header = ZxProcArgs::from_bytes(msg).expect("valid header");
        assert_eq!(header.args_num, 2);
        assert_eq!(
            header.find_handle(msg, 3, PA_VMO_VDSO),
            Some(1),
            "tag lookup after round-trip"
        );
        assert_eq!(header.arg(msg, 0), Some(&b"bin/hello"[..]));
        assert_eq!(header.arg(msg, 1), Some(&b"--flag"[..]));
        assert_eq!(header.arg(msg, 2), None);
    }

    /// write_message refuses to truncate: a too-small buffer yields 0.
    #[test]
    fn write_message_rejects_small_buffer() {
        let handle_info = [pa_hnd(PA_PROC_SELF, 0)];
        let mut buf = [0u8; 8]; // far too small
        assert_eq!(write_message(&handle_info, b"", &mut buf), 0);
    }

    /// The alloc frontend produces byte-identical output to the
    /// no-alloc writer.
    #[cfg(feature = "alloc")]
    #[test]
    fn build_message_matches_write_message() {
        let handle_info = [pa_hnd(PA_PROC_SELF, 0), pa_hnd(PA_JOB_DEFAULT, 0)];
        let argv = b"hello\0";
        let via_alloc = build_message(&handle_info, argv);
        let mut buf = [0u8; 128];
        let len = write_message(&handle_info, argv, &mut buf);
        assert_eq!(&via_alloc[..], &buf[..len]);
    }
}
