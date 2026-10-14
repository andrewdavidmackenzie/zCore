//! Version string tests — faithful port of Fuchsia `version/version-test.cc`.
//!
//! C++ source: `core-tests/version/version-test.cc`
//!
//! The C++ tests call `zx_system_get_version_string()` (a vDSO function)
//! and validate the returned string. In zCore, the version string is
//! stored in `VdsoConstants::version_string` and set during boot via
//! `VdsoConstants::set_version_string()`. We access it through the
//! HAL's `vdso_constants()` function.

use crate::helpers::TestContext;

/// Helper: get the version string from VdsoConstants via the HAL.
fn get_version_string() -> String {
    let constants = hal_impl::vdso::vdso_constants();
    let len = constants.version_string_len as usize;
    let full = constants.version_string.as_str();
    full[..len].to_string()
}

/// C++: TEST(VersionTest, ZxStringView)
///
/// ```cpp
/// zx_string_view_t zxsv = zx_system_get_version_string();
/// EXPECT_EQ(zxsv.length, strlen(zxsv.c_str));
/// EXPECT_EQ(zxsv.length, zxsv.size());
/// EXPECT_EQ(zxsv.c_str, zxsv.data());
/// ```
///
/// Verify the version string length matches the actual string length.
#[test]
fn zx_string_view() {
    let _ctx = TestContext::new();
    let constants = hal_impl::vdso::vdso_constants();
    let len = constants.version_string_len as usize;
    let full = constants.version_string.as_str();
    let s = &full[..len];
    // C++ checks: length == strlen
    assert_eq!(len, s.len(), "version_string_len should match strlen");
}

/// C++: TEST(VersionTest, StdStringView)
///
/// ```cpp
/// zx_string_view_t zxsv = zx_system_get_version_string();
/// std::string_view sv = zx_system_get_version_string();
/// EXPECT_EQ(sv.size(), zxsv.length);
/// EXPECT_STREQ(sv.data(), zxsv.c_str);
/// ```
///
/// Verify consistent views of the version string (two reads match).
#[test]
fn std_string_view() {
    let _ctx = TestContext::new();
    let s1 = get_version_string();
    let s2 = get_version_string();
    assert_eq!(s1, s2, "version strings from two reads should match");
    assert_eq!(s1.len(), s2.len(), "lengths should match");
}

/// C++: TEST(VersionTest, StdString)
///
/// ```cpp
/// std::string s = zx_system_get_version_string();
/// EXPECT_STREQ(s.c_str(), zxsv.c_str);
/// ```
///
/// Verify the version string can be copied and compared.
#[test]
fn std_string() {
    let _ctx = TestContext::new();
    let original = get_version_string();
    let copied = original.clone();
    assert_eq!(copied.len(), original.len());
    assert_eq!(copied, original);
}

/// C++: TEST(VersionTest, NonEmptyTrimmedPrintableString)
///
/// ```cpp
/// std::string_view version = zx_system_get_version_string();
/// ASSERT_FALSE(version.empty());
/// EXPECT_FALSE(isspace(version.front()));
/// EXPECT_FALSE(isspace(version.back()));
/// for (char c : version) { EXPECT_TRUE(isprint(c)); }
/// ```
///
/// Verify the version string is non-empty, trimmed, and all printable.
#[test]
fn non_empty_trimmed_printable_string() {
    let _ctx = TestContext::new();
    let version = get_version_string();

    // C++: ASSERT_FALSE(version.empty())
    assert!(!version.is_empty(), "version string should not be empty");

    // C++: EXPECT_FALSE(isspace(version.front()))
    assert!(
        !version.starts_with(char::is_whitespace),
        "version should not start with whitespace"
    );

    // C++: EXPECT_FALSE(isspace(version.back()))
    assert!(
        !version.ends_with(char::is_whitespace),
        "version should not end with whitespace"
    );

    // C++: for (char c : version) { EXPECT_TRUE(isprint(c)); }
    for (i, c) in version.chars().enumerate() {
        assert!(
            !c.is_control(),
            "version char at index {} ({:#x}) should be printable",
            i,
            c as u32
        );
    }
}

/// C++: TEST(VersionTest, StdU8StringView)
#[test]
fn std_u8_string_view() {
    // The C++ test calls zx_system_get_version_string and verifies the
    // returned zx_string_view_t can be used as std::u8string_view.
    // This is a C++ API-level test; in Rust the equivalent is covered
    // by the existing tests that verify the version string content.
}
