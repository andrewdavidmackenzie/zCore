//! petal runtime -- provides _start entry point, panic handler,
//! and global allocator for `alloc` support.
//!
//! Petal programs define `pub fn main()` and this runtime handles
//! the boilerplate. The startup handle from userstart is available
//! via `petal::take_startup_handle()`.

#![no_std]

extern crate alloc;

mod alloc_impl;

use core::sync::atomic::{AtomicU32, Ordering};

extern "Rust" {
    /// The user's main function.
    fn main();
}

static STARTUP_HANDLE: AtomicU32 = AtomicU32::new(0);

/// Take the startup handle passed by userstart.
/// Returns 0 (ZX_HANDLE_INVALID) if already taken or not set.
pub fn take_startup_handle() -> u32 {
    STARTUP_HANDLE.swap(0, Ordering::SeqCst)
}

/// Parsed bootstrap handles from a processargs channel message pair.
///
/// The launcher (kernel → userstart → program, or the rootfs spawn
/// path) sends two messages (#884): message 1 carries the program
/// bootstrap handles (process identity, job, resource, vDSO, ZBI);
/// message 2 carries extended system handles (sub-resources,
/// boot-options, UTC clock). `find` resolves tags across both.
pub struct Bootstrap {
    /// Message 1 data (contains processargs header + handle_info).
    pub data: [u8; 1024],
    /// Number of valid bytes in `data`.
    pub data_len: usize,
    /// Handle values received from message 1.
    pub handles: [u32; 32],
    /// Number of valid handles in message 1.
    pub num_handles: usize,
    /// Message 2 data (extended system handles). Empty when the
    /// launcher sent only one message.
    pub data2: alloc::vec::Vec<u8>,
    /// Handle values received from message 2.
    pub handles2: alloc::vec::Vec<u32>,
}

impl Bootstrap {
    /// Read the startup channel and parse the processargs messages.
    ///
    /// Returns `None` if no startup handle is available or the first
    /// channel read fails. A missing second message is tolerated
    /// (older launchers send only one); a larger-than-expected second
    /// message is retried with an exact-size buffer (the failed read
    /// reports the needed sizes and leaves the message queued, per
    /// Zircon semantics).
    pub fn read() -> Option<Self> {
        let ch = take_startup_handle();
        if ch == 0 {
            return None;
        }

        let mut b = Bootstrap {
            data: [0u8; 1024],
            handles: [0u32; 32],
            data_len: 0,
            num_handles: 0,
            data2: alloc::vec::Vec::new(),
            handles2: alloc::vec::Vec::new(),
        };
        let mut ab: u32 = 0;
        let mut ah: u32 = 0;
        let s = unsafe {
            zx::sys::zx_channel_read(
                ch,
                0,
                b.data.as_mut_ptr(),
                b.handles.as_mut_ptr(),
                b.data.len() as u32,
                b.handles.len() as u32,
                &mut ab,
                &mut ah,
            )
        };
        if s == 0 {
            b.data_len = ab as usize;
            b.num_handles = ah as usize;
            // Message 2 (extended system handles, #884) — optional.
            let read2 = |data: &mut [u8],
                         handles: &mut [u32],
                         ab: &mut u32,
                         ah: &mut u32|
             -> zircon_abi::errors::ZxStatus {
                unsafe {
                    zx::sys::zx_channel_read(
                        ch,
                        0,
                        data.as_mut_ptr(),
                        handles.as_mut_ptr(),
                        data.len() as u32,
                        handles.len() as u32,
                        ab,
                        ah,
                    )
                }
            };
            let mut data2 = alloc::vec![0u8; 512];
            let mut handles2 = alloc::vec![0u32; 16];
            let mut ab2: u32 = 0;
            let mut ah2: u32 = 0;
            let mut s2 = read2(&mut data2, &mut handles2, &mut ab2, &mut ah2);
            if s2 == zircon_abi::errors::ZX_ERR_BUFFER_TOO_SMALL {
                // Grow to the reported sizes and retry — the message
                // was not consumed by the failed read.
                data2 = alloc::vec![0u8; ab2 as usize];
                handles2 = alloc::vec![0u32; ah2 as usize];
                s2 = read2(&mut data2, &mut handles2, &mut ab2, &mut ah2);
            }
            if s2 == 0 {
                data2.truncate(ab2 as usize);
                handles2.truncate(ah2 as usize);
                b.data2 = data2;
                b.handles2 = handles2;
            }
        }
        unsafe { zx::sys::zx_handle_close(ch) };
        if s != 0 {
            return None;
        }
        Some(b)
    }

    /// Look up a handle by PA_* type tag, searching message 1 first
    /// and then message 2.
    ///
    /// Returns the handle value, or 0 (`ZX_HANDLE_INVALID`) if not found.
    pub fn find(&self, pa_type: u32) -> u32 {
        let msg = &self.data[..self.data_len];
        if let Some(header) = zircon_abi::processargs::ZxProcArgs::from_bytes(msg) {
            if let Some(idx) = header.find_handle(msg, self.num_handles, pa_type) {
                if idx < self.handles.len() {
                    return self.handles[idx];
                }
            }
        }
        if !self.handles2.is_empty() {
            let msg2 = &self.data2[..];
            if let Some(header) = zircon_abi::processargs::ZxProcArgs::from_bytes(msg2) {
                if let Some(idx) = header.find_handle(msg2, self.handles2.len(), pa_type) {
                    if idx < self.handles2.len() {
                        return self.handles2[idx];
                    }
                }
            }
        }
        0
    }

    /// Number of arguments (argv) in message 1, per the processargs
    /// protocol (NUL-separated strings at `args_off`).
    pub fn arg_count(&self) -> u32 {
        let msg = &self.data[..self.data_len];
        zircon_abi::processargs::ZxProcArgs::from_bytes(msg)
            .map(|h| h.args_num)
            .unwrap_or(0)
    }

    /// The `i`-th argument (argv[i]). argv[0] is conventionally the
    /// program name/path as invoked.
    pub fn arg(&self, i: u32) -> Option<&[u8]> {
        let msg = &self.data[..self.data_len];
        zircon_abi::processargs::ZxProcArgs::from_bytes(msg).and_then(|h| h.arg(msg, i))
    }
}

/// Entry point -- called by the kernel when the process starts.
#[no_mangle]
pub extern "C" fn _start(startup_handle: u32, _vdso_base: usize) -> ! {
    // Initialize the heap allocator before calling main.
    alloc_impl::init_heap();
    STARTUP_HANDLE.store(startup_handle, Ordering::SeqCst);
    unsafe { main() };
    zx::Process::exit(0);
}

/// Panic handler -- writes a message and exits with code 1.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    zx::debug_write(b"petal: PANIC!\n");
    zx::Process::exit(1);
}
