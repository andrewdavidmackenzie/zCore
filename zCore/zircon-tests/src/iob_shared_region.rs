//! IOB shared region tests.
//!
//! Ports of Fuchsia's IOB shared region test suite. IOB shared
//! regions provide shared memory between kernel and userspace.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(IobSharedRegion, CreateSucceeds)
#[test]
#[ignore = "requires IOB shared region infrastructure"]
fn create_succeeds() {
    todo!("requires IOB shared region infrastructure");
}

/// C++: TEST(IobSharedRegion, HandleInfoAndLifetime)
#[test]
#[ignore = "requires IOB shared region infrastructure"]
fn handle_info_and_lifetime() {
    todo!("requires IOB shared region infrastructure");
}

/// C++: TEST(IobSharedRegion, InvalidArgs)
#[test]
#[ignore = "requires IOB shared region infrastructure"]
fn invalid_args() {
    todo!("requires IOB shared region infrastructure");
}

/// C++: TEST(IobSharedRegion, OutOfRange)
#[test]
#[ignore = "requires IOB shared region infrastructure"]
fn out_of_range() {
    todo!("requires IOB shared region infrastructure");
}
