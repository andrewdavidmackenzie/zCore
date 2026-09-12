//! petal channel test -- exercises Zircon channel syscalls.
//!
//! Tests: channel_create, channel_write, channel_read, handle_close (via drop).

#![no_std]
#![no_main]

extern crate petal;

use zx::{Channel, Status};

#[no_mangle]
pub fn main() {
    zx::debug_write(b"channel_test: starting\n");

    // Create a channel pair
    let (ch0, ch1) = Channel::create().expect_ok("channel_create");
    zx::debug_write(b"channel_test: channel created\n");

    // Write a message (no handles)
    let msg = b"hello channel!";
    ch0.write_bytes(msg).expect_ok("channel_write");
    zx::debug_write(b"channel_test: message written\n");

    // Read it back from the other end (no handles)
    let mut buf = [0u8; 64];
    let actual_bytes = ch1.read_bytes(&mut buf).expect_ok("channel_read");

    if actual_bytes == msg.len() && &buf[..msg.len()] == msg {
        zx::debug_write(b"channel_test: message verified\n");
    } else {
        zx::debug_write(b"channel_test: FAIL - message mismatch\n");
        zx::Process::exit(1);
    }

    // Handles closed automatically on drop

    zx::debug_write(b"channel_test: PASS\n");
}

/// Extension trait for Result to use in no_std petal programs.
trait ExpectOk<T> {
    fn expect_ok(self, name: &str) -> T;
}

impl<T> ExpectOk<T> for Result<T, Status> {
    fn expect_ok(self, name: &str) -> T {
        match self {
            Ok(v) => v,
            Err(_) => {
                zx::debug_write(b"channel_test: FAIL - ");
                zx::debug_write(name.as_bytes());
                zx::debug_write(b" failed\n");
                zx::Process::exit(1);
            }
        }
    }
}
