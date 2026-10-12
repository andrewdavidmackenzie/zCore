//! Standalone ELF load tests.
//!
//! Ports of Fuchsia's standalone ELF load test suite. Tests
//! the standalone ELF loader.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(StandaloneElfLoadTests, Load)
#[test]
#[ignore = "requires ELF loader infrastructure"]
fn load() {
    todo!("requires ELF loader infrastructure");
}
