//! Additional page-size tests (not 1:1 C++ ports).

use zircon_object::vm::PAGE_SIZE;

/// Page size mask for alignment checks.
/// (No direct C++ binary equivalent — additional coverage.)
#[test]
fn page_size_mask() {
    let _ctx = crate::helpers::TestContext::new();
    let mask = PAGE_SIZE - 1;

    // Aligned addresses.
    let zero: usize = 0;
    assert_eq!(zero & mask, 0);
    assert_eq!(PAGE_SIZE & mask, 0);
    assert_eq!((2 * PAGE_SIZE) & mask, 0);

    // Unaligned addresses.
    assert_ne!(1 & mask, 0);
    assert_ne!((PAGE_SIZE + 1) & mask, 0);
}

/// PAGE_SIZE is a compile-time constant.
/// (No direct C++ binary equivalent — additional coverage.)
#[test]
fn page_size_const() {
    let _ctx = crate::helpers::TestContext::new();
    // PAGE_SIZE should be usable as a const expression.
    const PS: usize = PAGE_SIZE;
    let ps = PS;
    assert!(ps >= 4096);
    assert!(ps.is_power_of_two());
}
