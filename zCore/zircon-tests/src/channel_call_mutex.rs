//! Channel call mutex tests.
//!
//! Ports of Fuchsia's channel call mutex test suite. Tests that
//! mutexes held across channel_call work correctly.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ChannelCallMutexTest, MutexHeldAcrossCallChain)
#[test]
#[ignore = "requires multi-threaded channel call"]
fn mutex_held_across_call_chain() {
    todo!("requires multi-threaded channel call");
}
