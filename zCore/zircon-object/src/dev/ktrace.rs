//! Kernel trace ring buffer.
//!
//! Provides a fixed-size ring buffer for kernel trace records in FXT
//! (Fuchsia Trace Format). Controlled via `zx_ktrace_control` (start,
//! stop, rewind) and read via `zx_ktrace_read`.
//!
//! Currently the buffer is allocated but no instrumentation hooks
//! write records into it. This unblocks userspace tools that call
//! the ktrace syscalls during boot.

use crate::object::ZxError;
use crate::ZxResult;
use alloc::vec;
use alloc::vec::Vec;
use lock::Mutex;

/// Default trace buffer size (1 MiB). Fuchsia uses 32 MiB by default
/// but we start smaller since we don't emit records yet.
const DEFAULT_KTRACE_BUF_SIZE: usize = 1024 * 1024;

/// Kernel trace buffer — singleton, protected by Mutex.
pub struct KTraceBuffer {
    /// Ring buffer storage.
    buf: Vec<u8>,
    /// Number of valid bytes written (monotonic, not wrapped).
    write_offset: usize,
    /// Whether tracing is currently active.
    active: bool,
    /// Bitmask of enabled trace groups.
    group_mask: u32,
}

impl KTraceBuffer {
    /// Create a new trace buffer with the given size in bytes.
    fn new(size: usize) -> Self {
        Self {
            buf: vec![0u8; size],
            write_offset: 0,
            active: false,
            group_mask: 0,
        }
    }

    /// Control tracing: start (0), stop (1), rewind (2).
    pub fn control(&mut self, action: u32, options: u32) -> ZxResult {
        match action {
            KTRACE_ACTION_START => {
                self.active = true;
                if options != 0 {
                    self.group_mask = options;
                }
                Ok(())
            }
            KTRACE_ACTION_STOP => {
                self.active = false;
                Ok(())
            }
            KTRACE_ACTION_REWIND => {
                self.write_offset = 0;
                Ok(())
            }
            _ => Err(ZxError::INVALID_ARGS),
        }
    }

    /// Read trace data starting at `offset`, up to `len` bytes.
    /// Returns the slice of available data.
    pub fn read(&self, offset: usize, len: usize) -> (usize, &[u8]) {
        if offset >= self.write_offset {
            return (0, &[]);
        }
        let available = self.write_offset - offset;
        let to_copy = core::cmp::min(available, len);
        // Handle wrap-around within the buffer.
        let buf_offset = offset % self.buf.len();
        let end = core::cmp::min(buf_offset + to_copy, self.buf.len());
        (end - buf_offset, &self.buf[buf_offset..end])
    }

    /// Whether tracing is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Current group mask.
    pub fn group_mask(&self) -> u32 {
        self.group_mask
    }
}

// Control action constants (matching Fuchsia's KTRACE_ACTION_*).
const KTRACE_ACTION_START: u32 = 0;
const KTRACE_ACTION_STOP: u32 = 1;
const KTRACE_ACTION_REWIND: u32 = 2;

/// Global kernel trace buffer singleton.
static KTRACE_BUFFER: Mutex<Option<KTraceBuffer>> = Mutex::new(None);

/// Initialize the global ktrace buffer (call once at boot).
pub fn ktrace_init() {
    let mut guard = KTRACE_BUFFER.lock();
    if guard.is_none() {
        *guard = Some(KTraceBuffer::new(DEFAULT_KTRACE_BUF_SIZE));
    }
}

/// Access the global ktrace buffer for control operations.
pub fn ktrace_control(action: u32, options: u32) -> ZxResult {
    let mut guard = KTRACE_BUFFER.lock();
    let buf = guard.get_or_insert_with(|| KTraceBuffer::new(DEFAULT_KTRACE_BUF_SIZE));
    buf.control(action, options)
}

/// Read from the global ktrace buffer.
/// Returns `(bytes_read, data_slice)`.
pub fn ktrace_read(offset: usize, len: usize) -> (usize, Vec<u8>) {
    let guard = KTRACE_BUFFER.lock();
    if let Some(buf) = guard.as_ref() {
        let (n, slice) = buf.read(offset, len);
        (n, slice.to_vec())
    } else {
        (0, Vec::new())
    }
}
