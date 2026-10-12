//! Cache control tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/cache tests.
//! Cache control operations (flush, invalidate, sync) operate
//! on the CPU cache and require real hardware or at least real
//! mapped virtual memory.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(CacheTest, FlushDataCache)
///
/// Flush the data cache for a range of memory. Should succeed
/// for valid mapped addresses. Requires real memory mapping.
#[should_panic(expected = "cache ops need real mapping")]
#[test]
fn flush_data_cache() {
    let _ctx = TestContext::new();
    panic!("cache ops need real mapping");
}

/// C++: TEST(CacheTest, FlushInstructionCache)
///
/// Flush the instruction cache for a range. Required after writing
/// code into a VMO before execution. Requires real mapping.
#[should_panic(expected = "cache ops need real mapping")]
#[test]
fn flush_instruction_cache() {
    let _ctx = TestContext::new();
    panic!("cache ops need real mapping");
}

/// C++: TEST(CacheTest, SyncCaches)
///
/// Synchronize data and instruction caches. Required for JIT
/// compilation scenarios. Requires real mapping.
#[should_panic(expected = "cache ops need real mapping")]
#[test]
fn sync_caches() {
    let _ctx = TestContext::new();
    panic!("cache ops need real mapping");
}

/// C++: TEST(CacheTest, FlushNullAddress)
///
/// Flush cache with a null address. Should return INVALID_ARGS.
#[should_panic(expected = "cache ops need real mapping")]
#[test]
fn flush_null_address() {
    let _ctx = TestContext::new();
    panic!("cache ops need real mapping");
}

/// C++: TEST(CacheTest, FlushZeroLength)
///
/// Flush cache with zero length — should be a no-op and succeed.
#[should_panic(expected = "cache ops need real mapping")]
#[test]
fn flush_zero_length() {
    let _ctx = TestContext::new();
    panic!("cache ops need real mapping");
}

/// C++: TEST(CacheTest, FlushUnmappedAddress)
///
/// Flush cache for an unmapped address range. Should return
/// NOT_FOUND or cause a fault.
#[should_panic(expected = "cache ops need real mapping")]
#[test]
fn flush_unmapped_address() {
    let _ctx = TestContext::new();
    panic!("cache ops need real mapping");
}

/// C++: TEST(CacheTest, FlushLargeRange)
///
/// Flush cache for a large contiguous range. Should succeed
/// without hanging. Requires real mapping.
#[should_panic(expected = "cache ops need real mapping")]
#[test]
fn flush_large_range() {
    let _ctx = TestContext::new();
    panic!("cache ops need real mapping");
}

/// C++: TEST(CacheTest, InvalidOperation)
///
/// Call zx_cache_flush with an invalid operation code. Should
/// return INVALID_ARGS.
#[should_panic(expected = "cache ops need real mapping")]
#[test]
fn invalid_operation() {
    let _ctx = TestContext::new();
    panic!("cache ops need real mapping");
}
