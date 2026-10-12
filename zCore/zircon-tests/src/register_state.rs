//! Register state tests.
//!
//! Ports of Fuchsia's register state test suite. Tests
//! architecture-specific register state handling.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(RegisterStateTest, ContextSwitchOfGsBase)
#[test]
#[ignore = "requires hardware register state testing"]
fn context_switch_of_gs_base() {
    todo!("requires hardware register state testing");
}
