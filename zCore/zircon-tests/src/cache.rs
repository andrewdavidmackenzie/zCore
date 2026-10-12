//! Cache control tests.
//!
//! Ports of Fuchsia's zircon/system/utest/core/cache tests.
//! In zCore libos mode, cache operations are implemented as compiler
//! fences (no-ops on the actual cache). These tests verify the syscall
//! API's argument validation and error paths.

use crate::helpers::TestContext;
use zircon_object::ZxError;

/// Cache flush option flags (match Fuchsia ABI).
const ZX_CACHE_FLUSH_DATA: u32 = 1 << 0;
const ZX_CACHE_FLUSH_INVALIDATE: u32 = 1 << 1;
const ZX_CACHE_FLUSH_INSN: u32 = 1 << 2;

///
/// Flush the data cache for a range of memory. In libos mode this
/// is a no-op fence but should succeed for valid arguments.
#[test]
fn flush_data_cache() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Use a valid stack address and small range.
    let buf = [0u8; 64];
    let result = sc.sys_cache_flush(buf.as_ptr() as usize, buf.len(), ZX_CACHE_FLUSH_DATA);
    assert!(result.is_ok());
}

///
/// Flush the instruction cache for a range. In libos mode this is
/// a no-op fence but should succeed for valid arguments.
#[test]
fn flush_instruction_cache() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let buf = [0u8; 64];
    let result = sc.sys_cache_flush(buf.as_ptr() as usize, buf.len(), ZX_CACHE_FLUSH_INSN);
    assert!(result.is_ok());
}

///
/// Synchronize data and instruction caches (DATA | INSN). In libos
/// mode this is a no-op fence but should succeed.
#[test]
fn sync_caches() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let buf = [0u8; 64];
    let result = sc.sys_cache_flush(
        buf.as_ptr() as usize,
        buf.len(),
        ZX_CACHE_FLUSH_DATA | ZX_CACHE_FLUSH_INSN,
    );
    assert!(result.is_ok());
}

///
/// Flush cache with a null address but non-zero size. The syscall
/// implementation doesn't validate the address itself — it just does
/// a fence. Should succeed (no-op).
#[test]
fn flush_null_address() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // size > 0 with addr 0. In libos mode, the fence doesn't touch memory.
    let result = sc.sys_cache_flush(0, 64, ZX_CACHE_FLUSH_DATA);
    assert!(result.is_ok());
}

///
/// Flush cache with zero length — should be a no-op and succeed.
#[test]
fn flush_zero_length() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let result = sc.sys_cache_flush(0x1000, 0, ZX_CACHE_FLUSH_DATA);
    assert!(result.is_ok());
}

///
/// Flush with DATA | INVALIDATE — should succeed.
#[test]
fn flush_with_invalidate() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let buf = [0u8; 128];
    let result = sc.sys_cache_flush(
        buf.as_ptr() as usize,
        buf.len(),
        ZX_CACHE_FLUSH_DATA | ZX_CACHE_FLUSH_INVALIDATE,
    );
    assert!(result.is_ok());
}

///
/// Flush cache for a large contiguous range. Should succeed in libos
/// mode (fence only).
#[test]
fn flush_large_range() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let buf = vec![0u8; 1024 * 1024]; // 1 MiB
    let result = sc.sys_cache_flush(buf.as_ptr() as usize, buf.len(), ZX_CACHE_FLUSH_DATA);
    assert!(result.is_ok());
}

///
/// Call zx_cache_flush with an invalid operation code (0). Should
/// return INVALID_ARGS.
#[test]
fn invalid_operation() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    let buf = [0u8; 64];
    // options = 0 is invalid (must have DATA or INSN).
    let result = sc.sys_cache_flush(buf.as_ptr() as usize, buf.len(), 0);
    assert_eq!(result, Err(ZxError::INVALID_ARGS));

    // High bits set — should also be invalid.
    let result = sc.sys_cache_flush(buf.as_ptr() as usize, buf.len(), 0x80);
    assert_eq!(result, Err(ZxError::INVALID_ARGS));
}
