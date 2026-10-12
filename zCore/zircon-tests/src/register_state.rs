//! Register state tests.
//!
//! 1:1 port of Fuchsia's zircon/system/utest/core/register-context-switch/x86-segments.cc
//! (RegisterStateTest suite).
//!
//! Tests GS base register preservation across context switches using
//! pthread barriers and FSGSBASE instructions. This is x86-specific
//! hardware functionality not available in libos mode.

/// C++: TEST(RegisterStateTest, ContextSwitchOfGsBase)
/// Launches thread_count = num_cpus * 2 threads, each setting gs_base
/// to a unique value (i * 0x10004). All threads sync at a pthread
/// barrier, then verify gs_base and fs_base are preserved.
#[test]
#[ignore = "x86 FSGSBASE instructions and pthread barriers not available in libos mode"]
fn context_switch_of_gs_base() {
    // C++ test:
    //   const uint32_t thread_count = zx_system_get_num_cpus() * 2;
    //   pthread_barrier_init(&barrier, nullptr, thread_count);
    //   for (i in 0..thread_count) {
    //     // Each thread:
    //     //   1. If FSGSBASE available: _writegsbase_u64(gs_base = i * 0x10004)
    //     //   2. fs_base = _readfsbase_u64()
    //     //   3. pthread_barrier_wait (sync all threads)
    //     //   4. EXPECT_EQ(_readgsbase_u64(), gs_base)  // preserved
    //     //   5. EXPECT_EQ(_readfsbase_u64(), fs_base)  // preserved
    //   }
    //   pthread_barrier_destroy(&barrier);
}
