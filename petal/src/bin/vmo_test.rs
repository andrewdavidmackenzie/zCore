//! petal VMO test -- exercises Zircon VMO syscalls.
//!
//! Tests: vmo_create, vmo_write, vmo_read, vmo_get_size, handle_close (via drop).

#![no_std]
#![no_main]

extern crate petal;

use zx::{Status, Vmo};

#[no_mangle]
pub fn main() {
    zx::debug_write(b"vmo_test: starting\n");

    // Create a VMO
    let vmo = Vmo::create(4096).expect_ok("vmo_create");
    zx::debug_write(b"vmo_test: VMO created\n");

    // Check size
    let size = vmo.get_size().expect_ok("vmo_get_size");

    if size == 4096 {
        zx::debug_write(b"vmo_test: size verified (4096)\n");
    } else {
        zx::debug_write(b"vmo_test: FAIL - unexpected size\n");
        zx::Process::exit(1);
    }

    // Write data
    let data = b"Hello from VMO!";
    vmo.write(data, 0).expect_ok("vmo_write");
    zx::debug_write(b"vmo_test: data written\n");

    // Read it back
    let mut buf = [0u8; 64];
    vmo.read(&mut buf[..data.len()], 0).expect_ok("vmo_read");

    if &buf[..data.len()] == data {
        zx::debug_write(b"vmo_test: data verified\n");
    } else {
        zx::debug_write(b"vmo_test: FAIL - data mismatch\n");
        zx::Process::exit(1);
    }

    // Handle closed automatically on drop

    zx::debug_write(b"vmo_test: PASS\n");
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
                zx::debug_write(b"vmo_test: FAIL - ");
                zx::debug_write(name.as_bytes());
                zx::debug_write(b" failed\n");
                zx::Process::exit(1);
            }
        }
    }
}
