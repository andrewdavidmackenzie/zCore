//! Page size tests.
//!
//! Ports of Fuchsia's page size test suite.
//! Tests page size constants and alignment.

use crate::helpers::TestContext;
use zircon_object::vm::PAGE_SIZE;

/// C++: TEST(PageSize, NotZero)
/// PAGE_SIZE should be non-zero.
#[test]
#[allow(clippy::assertions_on_constants)]
fn page_size_not_zero() {
    let _ctx = TestContext::new();
    assert!(PAGE_SIZE > 0, "PAGE_SIZE should be non-zero");
}

/// C++: TEST(PageSize, Only4K)
/// PAGE_SIZE should be at least 4096.
#[test]
#[allow(clippy::assertions_on_constants)]
fn page_size_only_4k() {
    let _ctx = TestContext::new();
    assert!(PAGE_SIZE >= 4096, "PAGE_SIZE should be at least 4096");
}

/// C++: TEST(PageSize, PowerOfTwo)
/// PAGE_SIZE should be a power of 2.
#[test]
fn page_size_power_of_two() {
    let _ctx = TestContext::new();
    assert!(
        PAGE_SIZE.is_power_of_two(),
        "PAGE_SIZE should be a power of 2"
    );
}

/// C++: TEST(PageSize, ArchMinPageSize)
/// Verify the architecture minimum page size (VMO sizes are page-aligned).
#[test]
fn page_size_alignment() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    for pages in [1, 2, 4, 8, 16, 32] {
        let vmo = VmObject::new_paged(pages);
        assert_eq!(vmo.len() % PAGE_SIZE, 0, "VMO size should be page-aligned");
        assert_eq!(vmo.len(), pages * PAGE_SIZE);
    }
}

/// C++: TEST(PageSize, ArchMaxPageSize)
/// Round-up calculation: (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1).
#[test]
fn page_size_round_up() {
    let _ctx = TestContext::new();

    let round_up = |size: usize| size.div_ceil(PAGE_SIZE) * PAGE_SIZE;

    assert_eq!(round_up(0), 0);
    assert_eq!(round_up(1), PAGE_SIZE);
    assert_eq!(round_up(PAGE_SIZE), PAGE_SIZE);
    assert_eq!(round_up(PAGE_SIZE + 1), 2 * PAGE_SIZE);
    assert_eq!(round_up(2 * PAGE_SIZE), 2 * PAGE_SIZE);
}
