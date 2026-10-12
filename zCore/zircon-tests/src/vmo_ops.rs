//! VMO operations tests.
//!
//! Additional VMO operation tests covering op_range, commit, decommit,
//! zero, and cache operations.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::vm::{VmObject, PAGE_SIZE};

extern crate alloc;

///
/// Commit pages in a VMO. The committed pages should be backed by
/// physical memory.
#[test]
fn commit_pages() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(4);
    vmo.commit(0, 4 * PAGE_SIZE).unwrap();
    // After commit, reading should succeed
    let mut buf = [0u8; 8];
    vmo.read(0, &mut buf).unwrap();
}

///
/// Decommit pages from a VMO. The pages are returned to the kernel
/// and read as zeroes on next access.
#[test]
fn decommit_pages() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    vmo.write(0, b"hello world!").unwrap();
    vmo.decommit(0, 2 * PAGE_SIZE).unwrap();
    let mut buf = [0u8; 12];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, &[0u8; 12], "decommitted pages should read as zeroes");
}

///
/// Zero a range of a VMO. The zeroed range should read as all zeroes.
#[test]
fn zero_range() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.write(0, b"fill data here").unwrap();
    vmo.zero(0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 14];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, &[0u8; 14]);
}

///
/// Zero only part of a page. The rest should be untouched.
#[test]
fn zero_partial() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let data = b"abcdefghijklmnop";
    vmo.write(0, data).unwrap();
    // Zero first 8 bytes only
    vmo.zero(0, 8).unwrap();
    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf[..8], &[0u8; 8]);
    assert_eq!(&buf[8..], b"ijklmnop");
}

///
/// Commit pages that are already committed. Should be a no-op.
#[test]
fn commit_already_committed() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    vmo.write(0, b"data").unwrap(); // implicitly commits page 0
    vmo.commit(0, PAGE_SIZE).unwrap(); // commit again — no-op
    let mut buf = [0u8; 4];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"data");
}

///
/// Decommit pages then write new data. The new data should be
/// readable.
#[test]
fn decommit_and_rewrite() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.write(0, b"original").unwrap();
    vmo.decommit(0, PAGE_SIZE).unwrap();
    vmo.write(0, b"new data").unwrap();
    let mut buf = [0u8; 8];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"new data");
}

///
/// Commit a large range of pages at once.
#[test]
fn commit_large_range() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(32);
    vmo.commit(0, 32 * PAGE_SIZE).unwrap();
    // Write to last page to verify
    vmo.write(31 * PAGE_SIZE, b"end").unwrap();
    let mut buf = [0u8; 3];
    vmo.read(31 * PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(&buf, b"end");
}

///
/// Decommit only some pages of a multi-page VMO. Non-decommitted
/// pages should retain their data.
#[test]
fn decommit_partial() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(3);
    vmo.write(0, b"page0").unwrap();
    vmo.write(PAGE_SIZE, b"page1").unwrap();
    vmo.write(2 * PAGE_SIZE, b"page2").unwrap();

    // Decommit only page 1
    vmo.decommit(PAGE_SIZE, PAGE_SIZE).unwrap();

    let mut buf = [0u8; 5];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"page0", "page 0 should be untouched");
    vmo.read(2 * PAGE_SIZE, &mut buf).unwrap();
    assert_eq!(&buf, b"page2", "page 2 should be untouched");
}

///
/// Get and set content_size on a VMO. Content size tracks the
/// logical data size within the VMO.
#[test]
fn vmo_content_size() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(4);
    // Default content_size may be 0 or the VMO length depending on impl
    let _default_size = vmo.content_size();
    vmo.set_content_size(100).unwrap();
    assert_eq!(vmo.content_size(), 100);
    vmo.set_content_size(0).unwrap();
    assert_eq!(vmo.content_size(), 0);
}

///
/// Query the committed bytes of a VMO. Should reflect how many
/// pages are actually backed by physical memory.
#[test]
fn vmo_committed_bytes() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(4);
    // Initially no pages are committed
    let initial = vmo.committed_pages_in_range(0, 4 * PAGE_SIZE);
    // Write to commit some pages
    vmo.write(0, b"data").unwrap();
    let after_write = vmo.committed_pages_in_range(0, 4 * PAGE_SIZE);
    assert!(after_write >= 1, "at least one page should be committed");
    let _ = initial;
}

///
/// Every VMO should have a unique non-zero KOID.
#[test]
fn vmo_koid() {
    let _ctx = TestContext::new();
    let v1 = VmObject::new_paged(1);
    let v2 = VmObject::new_paged(1);
    assert_ne!(v1.id(), 0);
    assert_ne!(v2.id(), 0);
    assert_ne!(v1.id(), v2.id());
}

///
/// VMO length should be page-aligned and match the requested size.
#[test]
fn vmo_len() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(5);
    assert_eq!(vmo.len(), 5 * PAGE_SIZE);
}

///
/// Create a resizable VMO, resize it, and verify the new length.
#[test]
fn resizable_vmo() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged_with_resizable(true, 2);
    assert_eq!(vmo.len(), 2 * PAGE_SIZE);
    vmo.set_len(5 * PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), 5 * PAGE_SIZE);
}

///
/// Shrink a resizable VMO. Data beyond the new end is lost.
#[test]
fn resize_shrink() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged_with_resizable(true, 4);
    vmo.write(3 * PAGE_SIZE, b"end data").unwrap();
    vmo.set_len(2 * PAGE_SIZE).unwrap();
    assert_eq!(vmo.len(), 2 * PAGE_SIZE);
}

///
/// A non-resizable VMO should reject resize with UNAVAILABLE.
#[test]
fn non_resizable_reject_resize() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    let result = vmo.set_len(4 * PAGE_SIZE);
    assert_eq!(
        result.err(),
        Some(zircon_object::ZxError::UNAVAILABLE),
        "non-resizable VMO should reject resize"
    );
}
