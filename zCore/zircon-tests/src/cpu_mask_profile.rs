//! CPU mask profile tests.
//!
//! Ports of Fuchsia's CPU mask profile test suite. Tests CPU
//! affinity mask profiles for thread scheduling.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(CpuMaskProfile, ApplyProfile)
#[test]
#[ignore = "requires CPU mask profile infrastructure"]
fn apply_profile() {
    todo!("requires CPU mask profile infrastructure");
}

/// C++: TEST(CpuMaskProfile, EmptyMaskIsValid)
#[test]
#[ignore = "requires CPU mask profile infrastructure"]
fn empty_mask_is_valid() {
    todo!("requires CPU mask profile infrastructure");
}
