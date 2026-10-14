//! Additional zCore-specific tests for vmo_zero functionality.
//!
//! These are NOT 1:1 ports of C++ core-tests. They exercise
//! kernel-object-level APIs and zCore-specific behavior.

use crate::helpers::TestContext;
use zircon_object::signal::{Pager, Port};
use zircon_object::vm::{VmObject, PAGE_SIZE};

/// Zero pages in a pager-backed VMO (pre-supplied, no faulting).
#[test]
fn hidden_no_content() {
    let _ctx = TestContext::new();
    let pager = Pager::new();
    let port = Port::new(0).unwrap();

    let vmo = pager
        .create_vmo(0, &port, 0, (2 * PAGE_SIZE) as u64)
        .unwrap();

    // Supply pages with data.
    let src = VmObject::new_paged(2);
    src.write(0, &[0xAA; 8]).unwrap();
    src.write(PAGE_SIZE, &[0xBB; 8]).unwrap();
    pager
        .supply_pages(&vmo, 0, (2 * PAGE_SIZE) as u64, &src, 0)
        .unwrap();

    // Verify data is present.
    let mut buf = [0u8; 8];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0xAA; 8]);

    // Zero the first page.
    vmo.zero(0, PAGE_SIZE).unwrap();

    // First page should be zero.
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0u8; 8], "first page should be zero");

    // Second page should still have data.
    vmo.read(PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(buf, [0xBB; 8], "second page should be unchanged");
}

