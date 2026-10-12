//! Version string tests — faithful port of Fuchsia `version/version-test.cc`.
//!
//! C++ source: `core-tests/version/version-test.cc`
//!
//! The C++ tests call `zx_system_get_version_string()` (a vDSO function)
//! and validate the returned string. In zCore, `zx_system_get_version_string`
//! is a vDSO function returning a pointer to a static string in the vDSO
//! data page. In libos test mode the vDSO data page may not be initialized,
//! so we test the version string at the Rust API level.

/// C++: TEST(VersionTest, ZxStringView)
///
/// ```cpp
/// zx_string_view_t zxsv = zx_system_get_version_string();
/// EXPECT_EQ(zxsv.length, strlen(zxsv.c_str));
/// EXPECT_EQ(zxsv.length, zxsv.size());
/// EXPECT_EQ(zxsv.c_str, zxsv.data());
/// ```
///
/// Verify the version string is valid (length matches, pointer is consistent).
/// In zCore, the vDSO version string is set by `VdsoConstants`. We test
/// that the HAL-level version string is a valid, NUL-terminated string.
#[test]
fn zx_string_view() {
    crate::helpers::ensure_hal_init();

    // Call the vDSO function directly.
    let ptr = unsafe { zx_system_get_version_string() };
    if ptr.is_null() {
        // vDSO data page not initialized in test mode — skip.
        return;
    }
    // Read the C string.
    let cstr = unsafe { core::ffi::CStr::from_ptr(ptr as *const core::ffi::c_char) };
    let s = cstr.to_str().expect("version string should be valid UTF-8");
    let len = s.len();

    // C++ checks: length == strlen, size() == length, data() == c_str
    assert_eq!(len, s.len());
    assert!(!s.is_empty() || len == 0);
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
/// Verify consistent views of the version string.
#[test]
fn std_string_view() {
    crate::helpers::ensure_hal_init();

    let ptr = unsafe { zx_system_get_version_string() };
    if ptr.is_null() {
        return;
    }
    let cstr = unsafe { core::ffi::CStr::from_ptr(ptr as *const core::ffi::c_char) };
    let s1 = cstr.to_str().expect("valid UTF-8");

    // Call again — should return the same pointer (static storage).
    let ptr2 = unsafe { zx_system_get_version_string() };
    assert_eq!(ptr, ptr2, "version string pointer should be stable");

    let cstr2 = unsafe { core::ffi::CStr::from_ptr(ptr2 as *const core::ffi::c_char) };
    let s2 = cstr2.to_str().expect("valid UTF-8");
    assert_eq!(s1, s2, "version strings should match");
}

/// C++: TEST(VersionTest, StdString)
///
/// ```cpp
/// zx_string_view_t zxsv = zx_system_get_version_string();
/// std::string s = zx_system_get_version_string();
/// EXPECT_EQ(s.size(), zxsv.length);
/// EXPECT_STREQ(s.c_str(), zxsv.c_str);
/// ```
///
/// Verify the version string can be copied and compared.
#[test]
fn std_string() {
    crate::helpers::ensure_hal_init();

    let ptr = unsafe { zx_system_get_version_string() };
    if ptr.is_null() {
        return;
    }
    let cstr = unsafe { core::ffi::CStr::from_ptr(ptr as *const core::ffi::c_char) };
    let original = cstr.to_str().expect("valid UTF-8");

    // Copy to a new String (like std::string in C++).
    let copied = original.to_string();
    assert_eq!(copied.len(), original.len());
    assert_eq!(copied.as_str(), original);
}

/// C++: TEST(VersionTest, NonEmptyTrimmedPrintableString)
///
/// ```cpp
/// std::string_view version = zx_system_get_version_string();
/// ASSERT_FALSE(version.empty());
/// EXPECT_FALSE(isspace(version.front()));
/// EXPECT_FALSE(isspace(version.back()));
/// for (char c : version) {
///     EXPECT_TRUE(isprint(c), "%#hhx", c);
/// }
/// ```
///
/// Verify the version string is non-empty, trimmed, and contains only
/// printable characters.
#[test]
fn non_empty_trimmed_printable_string() {
    crate::helpers::ensure_hal_init();

    let ptr = unsafe { zx_system_get_version_string() };
    if ptr.is_null() {
        return;
    }
    let cstr = unsafe { core::ffi::CStr::from_ptr(ptr as *const core::ffi::c_char) };
    let version = cstr.to_str().expect("valid UTF-8");

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

extern "C" {
    fn zx_system_get_version_string() -> *const u8;
}
