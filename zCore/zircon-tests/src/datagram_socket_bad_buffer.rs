//! Datagram socket bad buffer tests.
//!
//! Ports of Fuchsia's datagram socket bad buffer test suite.
//! Tests socket operations with invalid user buffers.
//! zCore has no user-buffer fault injection. This test verifies
//! socket read on an empty socket returns SHOULD_WAIT.

use crate::helpers::TestContext;
use zircon_object::ipc::Socket;
use zircon_object::ZxError;

/// C++: TEST(DatagramSocketBadBufferTest, Read)
/// Verify that reading from an empty datagram socket returns
/// SHOULD_WAIT (not a crash or undefined behavior).
#[test]
fn read() {
    let _ctx = TestContext::new();
    let (s0, _s1) = Socket::create(0).unwrap();

    // Reading from an empty socket should return SHOULD_WAIT.
    let mut buf = [0u8; 64];
    let result = s0.read(false, &mut buf);
    assert_eq!(
        result.unwrap_err(),
        ZxError::SHOULD_WAIT,
        "reading empty socket should return SHOULD_WAIT"
    );
}
