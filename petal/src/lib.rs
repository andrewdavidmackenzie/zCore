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

/// Parsed bootstrap handles from a processargs channel message.
pub struct Bootstrap {
    /// Message data (contains processargs header + handle_info).
    pub data: [u8; 256],
    /// Number of valid bytes in `data`.
    pub data_len: usize,
    /// Handle values received from the channel.
    pub handles: [u32; 8],
    /// Number of valid handles.
    pub num_handles: usize,
}

impl Bootstrap {
    /// Read the startup channel and parse the processargs message.
    pub fn read() -> Self {
        let ch = take_startup_handle();
        assert!(ch != 0, "no startup handle");

        let mut b = Bootstrap {
            data: [0u8; 256],
            handles: [0u32; 8],
            data_len: 0,
            num_handles: 0,
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
        assert!(s == 0, "channel_read failed");
        unsafe { zx::sys::zx_handle_close(ch) };
        b.data_len = ab as usize;
        b.num_handles = ah as usize;
        b
    }

    /// Look up a handle by PA_* type tag.
    ///
    /// Returns the handle value, or 0 (`ZX_HANDLE_INVALID`) if not found.
    pub fn find(&self, pa_type: u32) -> u32 {
        let msg = &self.data[..self.data_len];
        let header = match zircon_abi::processargs::ZxProcArgs::from_bytes(msg) {
            Some(h) => h,
            None => return 0,
        };
        match header.find_handle(msg, self.num_handles, pa_type) {
            Some(idx) if idx < self.handles.len() => self.handles[idx],
            _ => 0,
        }
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
