//! Pthread TLS tests.
//!
//! Ports of Fuchsia's pthread thread-local storage test suite (PthreadTls).
//! Tests TLS key creation, get/set operations, and destructor callbacks.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(PthreadTls, PthreadTls)
#[test]
#[ignore = "requires pthread TLS infrastructure"]
fn pthread_tls() {
    todo!("requires pthread TLS infrastructure");
}
