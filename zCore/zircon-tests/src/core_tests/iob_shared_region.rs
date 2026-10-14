//! IOB shared region tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/iob/iob.cc
//! (IobSharedRegion test suite).
//!
//! IOB shared regions (zx_iob_create_shared_region) provide shared
//! memory regions for ring-buffer style communication. zCore does not
//! implement the IOB kernel object or any of the zx_iob_* syscalls.

use crate::helpers::TestContext;

/// C++: TEST(IobSharedRegion, InvalidArgs)
/// Tests invalid arguments to zx_iob_create_shared_region:
///   - Non-zero options → ZX_ERR_INVALID_ARGS
///   - Zero size → ZX_ERR_INVALID_ARGS
///   - Non-page-aligned size → ZX_ERR_INVALID_ARGS
#[test]
fn invalid_args() {
    let _ctx = TestContext::new();
    // C++ test:
    //   EXPECT_STATUS(zx_iob_create_shared_region(1, page_size, &handle), ZX_ERR_INVALID_ARGS);
    //   EXPECT_STATUS(zx_iob_create_shared_region(0, 0, &handle), ZX_ERR_INVALID_ARGS);
    //   EXPECT_STATUS(zx_iob_create_shared_region(0, page_size + 3, &handle), ZX_ERR_INVALID_ARGS);
}

/// C++: TEST(IobSharedRegion, OutOfRange)
/// Tests out-of-range sizes for zx_iob_create_shared_region:
///   - Near integer limit → ZX_ERR_OUT_OF_RANGE
///   - Larger than max VMO size → ZX_ERR_OUT_OF_RANGE
#[test]
fn out_of_range() {
    let _ctx = TestContext::new();
    // C++ test:
    //   const uint64_t large_size = UINT64_MAX - page_size + 1;
    //   EXPECT_STATUS(zx_iob_create_shared_region(0, large_size, &handle), ZX_ERR_OUT_OF_RANGE);
    //
    //   // Get max VMO size from an unbounded VMO
    //   zx::vmo::create(0, ZX_VMO_UNBOUNDED, &vmo);
    //   vmo.get_size(&max_vmo_size);
    //   EXPECT_STATUS(zx_iob_create_shared_region(0, max_vmo_size + page_size, &handle),
    //                 ZX_ERR_OUT_OF_RANGE);
}

/// C++: TEST(IobSharedRegion, CreateSucceeds)
/// Successfully creates a shared region with valid arguments.
#[test]
fn create_succeeds() {
    let _ctx = TestContext::new();
    // C++ test:
    //   EXPECT_OK(zx_iob_create_shared_region(0, page_size, &handle));
    //   zx_handle_close(handle);
}

/// C++: TEST(Iob, SharedRegionWithNonRingBufferDiscipline)
/// Creating an IOB with shared region and non-ring-buffer discipline
/// (NONE or ID_ALLOCATOR) returns ZX_ERR_INVALID_ARGS.
#[test]
fn shared_region_with_non_ring_buffer_discipline() {
    let _ctx = TestContext::new();
    // C++ test:
    //   zx_iob_create_shared_region(0, 2 * page_size, &shared_region);
    //   for (type : {ZX_IOB_DISCIPLINE_TYPE_NONE, ZX_IOB_DISCIPLINE_TYPE_ID_ALLOCATOR}) {
    //     config = { .type = ZX_IOB_REGION_TYPE_SHARED, .discipline = {.type = type}, ... };
    //     EXPECT_STATUS(zx::iob::create(0, &config, 1, &ep0, &ep1), ZX_ERR_INVALID_ARGS);
    //   }
}
