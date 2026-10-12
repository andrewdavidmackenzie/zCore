//! Bad segment selector tests.
//!
//! Ports of Fuchsia's bad-segsel test suite. Tests x86-specific
//! segment selector validation and fault handling.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(BadSegselTest, JumpToFirstLDTEntry)
#[test]
#[ignore = "requires x86 segment selector hardware"]
fn jump_to_first_ldt_entry() {
    todo!("requires x86 segment selector hardware");
}

/// C++: TEST(BadSegselTest, JumpToLastGDTEntry)
#[test]
#[ignore = "requires x86 segment selector hardware"]
fn jump_to_last_gdt_entry() {
    todo!("requires x86 segment selector hardware");
}

/// C++: TEST(BadSegselTest, LoadFirstLDTEntry)
#[test]
#[ignore = "requires x86 segment selector hardware"]
fn load_first_ldt_entry() {
    todo!("requires x86 segment selector hardware");
}

/// C++: TEST(BadSegselTest, LoadLastGDTEntry)
#[test]
#[ignore = "requires x86 segment selector hardware"]
fn load_last_gdt_entry() {
    todo!("requires x86 segment selector hardware");
}

/// C++: TEST(BadSegselTest, TestAllGDTSelectors)
#[test]
#[ignore = "requires x86 segment selector hardware"]
fn test_all_gdt_selectors() {
    todo!("requires x86 segment selector hardware");
}
