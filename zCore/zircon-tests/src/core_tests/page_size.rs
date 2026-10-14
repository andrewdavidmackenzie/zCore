//! Page size tests.
//!
//! Ports of Fuchsia's page size test suite.
//! Tests page size constants and alignment.

use crate::helpers::TestContext;
use zircon_object::vm::PAGE_SIZE;

/// C++: TEST(PageSizeTest, PageSizeIsValid)
/// PAGE_SIZE should be a power of 2 and at least 4096.
#[test]
fn page_size_is_valid() {
    let _ctx = TestContext::new();
    assert!(PAGE_SIZE >= 4096, "PAGE_SIZE should be >= 4096");
    assert!(
        PAGE_SIZE.is_power_of_two(),
        "PAGE_SIZE should be a power of 2"
    );
}

/// C++: TEST(PageSizeTest, PageSizeAlignment)
/// VMO sizes should be page-aligned.
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

/// C++: TEST(PageSizeTest, PageSizeRoundUp)
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

/// C++: TEST(PageSizeTest, PageSizeMask)
/// Page size mask for alignment checks.
#[test]
fn page_size_mask() {
    let _ctx = TestContext::new();
    let mask = PAGE_SIZE - 1;

    // Aligned addresses.
    assert_eq!(0 & mask, 0);
    assert_eq!(PAGE_SIZE & mask, 0);
    assert_eq!((2 * PAGE_SIZE) & mask, 0);

    // Unaligned addresses.
    assert_ne!(1 & mask, 0);
    assert_ne!((PAGE_SIZE + 1) & mask, 0);
}

/// C++: TEST(PageSizeTest, PageSizeConst)
/// PAGE_SIZE is a compile-time constant.
#[test]
fn page_size_const() {
    let _ctx = TestContext::new();
    // PAGE_SIZE should be usable as a const expression.
    const PS: usize = PAGE_SIZE;
    assert!(PS >= 4096);
    assert!(PS.is_power_of_two());
}
