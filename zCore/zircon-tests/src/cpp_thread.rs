//! C++ thread tests.
//!
//! Ports of Fuchsia's C++ thread test suite. Tests C++ std::thread
//! integration with Zircon threads.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(CppThreadTest, CreateAndVerifyThreadHandle)
#[test]
#[ignore = "requires C++ thread infrastructure"]
fn create_and_verify_thread_handle() {
    todo!("requires C++ thread infrastructure");
}
