//! Hotplug tests.
//!
//! Ports of Fuchsia's hotplug test suite. Tests CPU
//! hotplug/unplug functionality.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(HotplugTests, KHotplugTest)
#[test]
#[ignore = "requires CPU hotplug infrastructure"]
fn k_hotplug_test() {
    todo!("requires CPU hotplug infrastructure");
}
