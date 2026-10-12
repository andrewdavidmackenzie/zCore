//! Page size tests.
//!
//! Ports of Fuchsia's page size test suite. Tests that the system
//! page size meets expected constraints.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(PageSize, ArchMaxPageSize)
#[test]
#[ignore = "needs page size query syscall"]
fn arch_max_page_size() {
    todo!("needs page size query syscall");
}

/// C++: TEST(PageSize, ArchMinPageSize)
#[test]
#[ignore = "needs page size query syscall"]
fn arch_min_page_size() {
    todo!("needs page size query syscall");
}

/// C++: TEST(PageSize, NotZero)
#[test]
#[ignore = "needs page size query syscall"]
fn not_zero() {
    todo!("needs page size query syscall");
}

/// C++: TEST(PageSize, Only4K)
#[test]
#[ignore = "needs page size query syscall"]
fn only4_k() {
    todo!("needs page size query syscall");
}

/// C++: TEST(PageSize, PowerOfTwo)
#[test]
#[ignore = "needs page size query syscall"]
fn power_of_two() {
    todo!("needs page size query syscall");
}
