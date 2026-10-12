//! Object child tests.
//!
//! Ports of Fuchsia's object child test suite. Tests
//! zx_object_get_child with invalid handles.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ObjectChildTest, InvalidHandleReturnsBadHandle)
#[test]
#[ignore = "requires object child enumeration"]
fn invalid_handle_returns_bad_handle() {
    todo!("requires object child enumeration");
}
