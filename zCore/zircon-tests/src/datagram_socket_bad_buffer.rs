//! Datagram socket bad buffer tests.
//!
//! Ports of Fuchsia's datagram socket bad buffer test suite.
//! Tests socket operations with invalid user buffers.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(DatagramSocketBadBufferTest, Read)
#[test]
#[ignore = "requires user-buffer fault injection"]
fn read() {
    todo!("requires user-buffer fault injection");
}
