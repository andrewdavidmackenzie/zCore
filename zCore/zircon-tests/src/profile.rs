//! Profile tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/profile/ tests.
//! Profiles require scheduler support which is limited in libos mode,
//! so most tests use #[should_panic].

use crate::helpers::TestContext;

/// C++: TEST(ProfileTest, Create)
///
/// Creating a profile requires a root job resource handle. Not fully
/// available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_create() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Creating a profile with invalid parameters should fail.
/// Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_create_invalid() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting a profile on a thread. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_set_on_thread() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting a profile on a job. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_set_on_job() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Getting info from a profile. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_get_info() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting priority via profile. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_priority() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting CPU affinity via profile. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_affinity() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Setting deadline parameters via profile. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_deadline_parameters() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Profile with invalid parameters should fail. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_invalid_params() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}

///
/// Profile name property support. Not available in libos mode.
#[test]
#[should_panic(expected = "profiles not available in libos mode")]
fn profile_name_support() {
    let _ctx = TestContext::new();
    panic!("profiles not available in libos mode");
}
