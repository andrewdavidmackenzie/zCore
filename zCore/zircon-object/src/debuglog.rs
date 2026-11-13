//! Objects for Kernel Debuglog.
use {
    super::*,
    crate::object::*,
    alloc::{sync::Arc, vec::Vec},
    core::sync::atomic::{AtomicU64, Ordering},
    lock::Mutex,
};

/// Global sequence counter for debuglog records.
static DLOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

static DLOG: spin::Lazy<Mutex<DlogBuffer>> = spin::Lazy::new(|| {
    Mutex::new(DlogBuffer {
        buf: Vec::with_capacity(0x1000),
    })
});

/// Debuglog - Kernel debuglog
///
/// ## SYNOPSIS
///
/// Debuglog objects allow userspace to read and write to kernel debug logs.
pub struct DebugLog {
    base: KObjectBase,
    flags: u32,
    read_offset: Mutex<usize>,
}

struct DlogBuffer {
    /// Append only buffer
    buf: Vec<u8>,
}

impl_kobject!(DebugLog);

impl DebugLog {
    /// Create a new `DebugLog`.
    pub fn create(flags: u32) -> Arc<Self> {
        Arc::new(DebugLog {
            base: KObjectBase::new(),
            flags,
            read_offset: Default::default(),
        })
    }

    /// Read a log record into `buf`. Returns the number of bytes userspace
    /// should see (`DLOG_MAX_LEN` on success, 0 when no records remain).
    pub fn read(&self, buf: &mut [u8]) -> usize {
        let mut offset = self.read_offset.lock();
        let (wire_size, user_size) = DLOG.lock().read_at(*offset, buf);
        *offset += wire_size;
        user_size
    }

    /// Write a log.
    pub fn write(&self, severity: Severity, flags: u32, tid: u64, pid: u64, data: &str) {
        DLOG.lock()
            .write(severity, flags | self.flags, tid, pid, data.as_bytes());
    }
}

/// Fuchsia's `zx_log_record_t` layout (40 bytes).
///
/// ```c
/// typedef struct zx_log_record {
///     uint64_t sequence;
///     uint8_t padding1[4];
///     uint16_t datalen;
///     uint8_t severity;
///     uint8_t flags;
///     zx_instant_boot_t timestamp;
///     uint64_t pid;
///     uint64_t tid;
///     char data[];
/// } zx_log_record_t;
/// ```
#[repr(C)]
#[derive(Debug)]
struct DlogHeader {
    sequence: u64,
    padding1: [u8; 4],
    datalen: u16,
    severity: Severity,
    flags: u8,
    timestamp: u64,
    pid: u64,
    tid: u64,
}

/// Log entry severity. Used for coarse filtering of log messages.
#[allow(missing_docs)]
#[repr(u8)]
#[derive(Debug)]
pub enum Severity {
    Trace = 0x10,
    Debug = 0x20,
    Info = 0x30,
    Warning = 0x40,
    Error = 0x50,
    Fatal = 0x60,
}

const HEADER_SIZE: usize = core::mem::size_of::<DlogHeader>();

/// Maximum total record size (header + data), matching Fuchsia's
/// `ZX_LOG_RECORD_MAX`.
pub const DLOG_MAX_LEN: usize = 256;

/// Maximum data payload per record, matching Fuchsia's
/// `ZX_LOG_RECORD_DATA_MAX = ZX_LOG_RECORD_MAX - sizeof(zx_log_record_t)`.
pub const DLOG_MAX_DATA: usize = DLOG_MAX_LEN - HEADER_SIZE;

impl DlogBuffer {
    /// Read one record at offset. Copies the header and data payload into
    /// `buf` (which must be at least `DLOG_MAX_LEN` bytes), zero-filling the
    /// remainder. Returns `(wire_size, user_size)` where `wire_size` is the
    /// number of bytes consumed from the internal buffer (for offset
    /// advancement) and `user_size` is `DLOG_MAX_LEN` (what userspace sees).
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> (usize, usize) {
        assert!(buf.len() >= DLOG_MAX_LEN);
        if offset >= self.buf.len() {
            return (0, 0);
        }
        // Read the header.
        let header_end = offset + HEADER_SIZE;
        buf[..HEADER_SIZE].copy_from_slice(&self.buf[offset..header_end]);
        // Read datalen from the copied header bytes at offset 12 (u16 LE).
        // Avoids an unsafe misaligned cast to DlogHeader.
        let datalen = u16::from_ne_bytes([buf[12], buf[13]]) as usize;
        let wire_size = HEADER_SIZE + align_up_4(datalen);
        // Copy data payload.
        let data_end = (offset + HEADER_SIZE + datalen).min(self.buf.len());
        let actual_data = data_end - (offset + HEADER_SIZE);
        buf[HEADER_SIZE..HEADER_SIZE + actual_data]
            .copy_from_slice(&self.buf[offset + HEADER_SIZE..data_end]);
        // Zero-fill remainder of buffer up to DLOG_MAX_LEN.
        for b in &mut buf[HEADER_SIZE + actual_data..DLOG_MAX_LEN] {
            *b = 0;
        }
        (wire_size, DLOG_MAX_LEN)
    }

    fn write(&mut self, severity: Severity, flags: u32, tid: u64, pid: u64, data: &[u8]) {
        let datalen = data.len().min(DLOG_MAX_DATA);
        let wire_size = HEADER_SIZE + align_up_4(datalen);
        let sequence = DLOG_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let header = DlogHeader {
            sequence,
            padding1: [0; 4],
            datalen: datalen as u16,
            severity,
            flags: flags as u8,
            timestamp: hal_impl::timer::timer_now().as_nanos() as u64,
            pid,
            tid,
        };
        // Serialize header fields to bytes in native byte order.
        self.buf.extend_from_slice(&header.sequence.to_ne_bytes());
        self.buf.extend_from_slice(&header.padding1);
        self.buf.extend_from_slice(&header.datalen.to_ne_bytes());
        self.buf.push(header.severity as u8);
        self.buf.push(header.flags);
        self.buf.extend_from_slice(&header.timestamp.to_ne_bytes());
        self.buf.extend_from_slice(&header.pid.to_ne_bytes());
        self.buf.extend_from_slice(&header.tid.to_ne_bytes());
        self.buf.extend_from_slice(&data[..datalen]);
        // Pad to 4-byte alignment.
        let padding = wire_size - HEADER_SIZE - datalen;
        if padding > 0 {
            self.buf.extend_from_slice(&[0u8; 3][..padding]);
        }
    }
}

fn align_up_4(x: usize) -> usize {
    (x + 3) & !3
}
