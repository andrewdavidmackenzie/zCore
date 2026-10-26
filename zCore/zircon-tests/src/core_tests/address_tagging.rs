//! Address tagging tests (core-tests-standalone subset).
//!
//! Only the x86_64 system features test is in the core-tests-standalone
//! binary. The ARM64-only TBI tests live in `extra_tests/address_tagging_tbi.rs`.

use crate::helpers::TestContext;

/// Return the address_tagging features bitmask from VdsoConstants.
fn get_address_tagging_features() -> u32 {
    let constants = hal_impl::vdso::vdso_constants();
    constants.features.address_tagging
}

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesX86_64)
///
/// On x86_64, TBI is never available. The C++ test calls
/// `zx_system_get_features(ZX_FEATURE_KIND_ADDRESS_TAGGING, &features)`
/// and asserts `features == 0`.
///
/// We read from `VdsoConstants.features.address_tagging` directly, which
/// is the kernel-side source of truth backing the vDSO function.
#[test]
fn address_tagging_get_system_features_x86_64() {
    let _ctx = TestContext::new();
    let features = get_address_tagging_features();
    // On x86_64 (and on any platform without TBI), features must be 0.
    assert_eq!(
        features, 0,
        "address tagging features should be 0 on x86_64"
    );
}
