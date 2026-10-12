//! Bad access tests.
//!
//! Ports of Fuchsia's user-space memory fault test suite.
//! zCore has no user-space fault injection. These tests verify
//! that VMO/VMAR APIs properly reject out-of-bounds access.

use crate::helpers::TestContext;
use zircon_object::vm::{VmObject, PAGE_SIZE};
use zircon_object::ZxError;

/// C++: TEST(BadAccessTest, ReadOutOfBounds)
/// Reading past VMO end should fail.
#[test]
fn read_out_of_bounds() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let mut buf = [0u8; 4];
    let result = vmo.read(2 * PAGE_SIZE, &mut buf);
    assert!(result.is_err(), "read past VMO end should fail");
}

/// C++: TEST(BadAccessTest, DecommitOutOfBounds)
/// Decommit past VMO end should return OUT_OF_RANGE.
#[test]
fn decommit_out_of_bounds() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    assert_eq!(
        vmo.decommit(0, 2 * PAGE_SIZE).unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(BadAccessTest, ZeroOutOfBounds)
/// Zero past VMO end should return OUT_OF_RANGE.
#[test]
fn zero_out_of_bounds() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    assert_eq!(
        vmo.zero(0, 2 * PAGE_SIZE).unwrap_err(),
        ZxError::OUT_OF_RANGE
    );
}

/// C++: TEST(BadAccessTest, MapBeyondVmo)
/// Mapping beyond VMO size should fail.
#[test]
fn map_beyond_vmo() {
    use zircon_object::task::{Job, Process};
    use zircon_object::vm::MMUFlags;

    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "bad-access").unwrap();
    let vmar = proc.vmar();
    let vmo = VmObject::new_paged(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;

    let result = vmar.map(None, vmo, 0, 2 * PAGE_SIZE, flags);
    assert!(result.is_err(), "mapping beyond VMO should fail");
}

/// C++: TEST(BadAccessTest, WriteToReadOnly)
/// VMO with uncached policy rejects read/write.
#[test]
fn write_to_read_only() {
    use zircon_object::vm::CachePolicy;

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.set_cache_policy(CachePolicy::Uncached).unwrap();

    // Uncached VMO rejects read/write.
    let mut buf = [0u8; 1];
    assert_eq!(vmo.read(0, &mut buf).unwrap_err(), ZxError::BAD_STATE);
    assert_eq!(vmo.write(0, &[42]).unwrap_err(), ZxError::BAD_STATE);
}
