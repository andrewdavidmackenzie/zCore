//! Standalone ELF load tests.
//!
//! Ports of Fuchsia's standalone ELF load test suite. Tests
//! the standalone ELF loader.
//! zCore has no standalone ELF loader API. This test verifies
//! that VMOs can hold ELF-like data and be read back correctly.

use crate::helpers::TestContext;
use zircon_object::vm::VmObject;

/// C++: TEST(StandaloneElfLoadTests, Load)
/// Verify that ELF-like binary data can be stored in a VMO and
/// read back correctly, simulating the data path of an ELF loader.
#[test]
fn load() {
    let _ctx = TestContext::new();

    // Create a VMO to hold ELF-like data.
    let vmo = VmObject::new_paged(1);

    // Write a minimal ELF magic header.
    let elf_magic = [0x7Fu8, b'E', b'L', b'F'];
    vmo.write(0, &elf_magic).unwrap();

    // Read it back and verify.
    let mut buf = [0u8; 4];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, &elf_magic, "ELF magic should round-trip through VMO");

    // Write more "ELF header" data.
    let header = [0u8; 64]; // Simulated ELF header
    vmo.write(0, &header).unwrap();
    let mut read_header = [0xFFu8; 64];
    vmo.read(0, &mut read_header).unwrap();
    assert_eq!(read_header, header);
}
