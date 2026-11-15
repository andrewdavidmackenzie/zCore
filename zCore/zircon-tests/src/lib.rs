//! Rust-native Zircon syscall test suite.
//!
//! This crate contains tests organized into two categories:
//!
//! ## `core_tests` -- 1:1 C++ core-test ports (~1800 tests)
//!
//! Faithful ports of the Fuchsia `core-tests-standalone` binary.
//! Each sub-module maps to a C++ test suite directory under
//! `zircon/system/utest/core/`.  Test functions have
//! `/// C++: TEST(Suite, Name)` doc comments.
//!
//! Coverage: 99% of the 1778 C++ core-tests are ported.
//!
//! ## Top-level modules -- zCore-original tests
//!
//! Additional tests that exercise kernel-object-level APIs, syscall
//! dispatch, and zCore-specific behavior not covered by the C++ suite.
//!
//! # Running
//!
//! ```sh
//! cargo test -p zircon-tests --no-fail-fast -- --test-threads=1
//! ```

#![cfg_attr(not(feature = "libos"), no_std)]

#[macro_use]
extern crate alloc;
#[cfg(any(test, feature = "baremetal-test"))]
mod helpers;
mod prelude;
#[cfg(any(test, feature = "baremetal-test"))]
mod test_runtime;

// ── 1:1 C++ core-test ports ────────────────────────────────────────
#[cfg(any(test, feature = "baremetal-test"))]
mod core_tests;

// ── Tests from other Fuchsia binaries (not core-tests-standalone) ──
#[cfg(any(test, feature = "baremetal-test"))]
mod extra_tests;

// ── zCore-original tests ───────────────────────────────────────────
#[cfg(any(test, feature = "baremetal-test"))]
mod bad_syscall;
#[cfg(any(test, feature = "baremetal-test"))]
mod bti_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod cache;
#[cfg(any(test, feature = "baremetal-test"))]
mod channel_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod clock_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod clock_ops;
#[cfg(any(test, feature = "baremetal-test"))]
mod cprng;
#[cfg(any(test, feature = "baremetal-test"))]
mod debug;
#[cfg(any(test, feature = "baremetal-test"))]
mod debuglog_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod event_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod event_pair_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod exception_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod exception_ops;
#[cfg(any(test, feature = "baremetal-test"))]
mod fifo_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod fifo_ops;
#[cfg(any(test, feature = "baremetal-test"))]
mod futex_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod handle_dup;
#[cfg(any(test, feature = "baremetal-test"))]
mod handle_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod interrupt_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod ipc_limits;
#[cfg(any(test, feature = "baremetal-test"))]
mod job_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod kcounter;
#[cfg(any(test, feature = "baremetal-test"))]
mod memory_mapping_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod mini_process;
#[cfg(any(test, feature = "baremetal-test"))]
mod namespace;
#[cfg(any(test, feature = "baremetal-test"))]
mod object_info_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod object_lifecycle;
#[cfg(any(test, feature = "baremetal-test"))]
mod object_properties;
#[cfg(any(test, feature = "baremetal-test"))]
mod object_signal;
#[cfg(any(test, feature = "baremetal-test"))]
mod object_wait_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod object_wait_ops;
#[cfg(any(test, feature = "baremetal-test"))]
mod page_size_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod pager_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod policy;
#[cfg(any(test, feature = "baremetal-test"))]
mod port_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod port_stress_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod process_debug_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod process_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod profile_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod property_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod pthread_tls;
#[cfg(any(test, feature = "baremetal-test"))]
mod resource_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod rights;
#[cfg(any(test, feature = "baremetal-test"))]
mod socket_disposition;
#[cfg(any(test, feature = "baremetal-test"))]
mod socket_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod stream_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod stream_ops;
#[cfg(any(test, feature = "baremetal-test"))]
mod suspend_resume;
#[cfg(any(test, feature = "baremetal-test"))]
mod syscall_channel_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod syscall_fifo_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod syscall_handle_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod syscall_object;
#[cfg(any(test, feature = "baremetal-test"))]
mod syscall_port_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod syscall_signal_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod syscall_socket_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod syscall_transfer_data_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod syscall_vmo_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod task_lifecycle;
#[cfg(any(test, feature = "baremetal-test"))]
mod thread_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod timer_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod timer_ops;
#[cfg(any(test, feature = "baremetal-test"))]
mod vmar_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod vmar_ops;
#[cfg(any(test, feature = "baremetal-test"))]
mod vmo_clone_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod vmo_contiguous;
#[cfg(any(test, feature = "baremetal-test"))]
mod vmo_discardable;
#[cfg(any(test, feature = "baremetal-test"))]
mod vmo_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod vmo_ops;
#[cfg(any(test, feature = "baremetal-test"))]
mod vmo_slice_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod vmo_zero_extra;
#[cfg(any(test, feature = "baremetal-test"))]
mod zx_assert_extra;

/// Run all tests (for bare-metal kernel test runner).
#[cfg(feature = "baremetal-test")]
pub async fn run_all() {
    core_tests::run_all().await;
    extra_tests::run_all().await;
    bad_syscall::run_all();
    bti_extra::run_all();
    cache::run_all();
    channel_extra::run_all();
    clock_extra::run_all();
    clock_ops::run_all();
    cprng::run_all();
    debug::run_all();
    debuglog_extra::run_all();
    event_extra::run_all();
    event_pair_extra::run_all();
    exception_extra::run_all();
    exception_ops::run_all();
    fifo_extra::run_all();
    fifo_ops::run_all();
    futex_extra::run_all();
    handle_dup::run_all();
    handle_extra::run_all();
    interrupt_extra::run_all();
    ipc_limits::run_all();
    job_extra::run_all();
    kcounter::run_all();
    memory_mapping_extra::run_all();
    mini_process::run_all();
    namespace::run_all();
    object_info_extra::run_all();
    object_lifecycle::run_all();
    object_properties::run_all();
    object_signal::run_all();
    object_wait_extra::run_all().await;
    object_wait_ops::run_all().await;
    page_size_extra::run_all();
    pager_extra::run_all().await;
    policy::run_all();
    port_extra::run_all().await;
    port_stress_extra::run_all().await;
    process_debug_extra::run_all();
    process_extra::run_all();
    profile_extra::run_all();
    property_extra::run_all();
    pthread_tls::run_all();
    resource_extra::run_all();
    rights::run_all();
    socket_disposition::run_all();
    socket_extra::run_all();
    stream_extra::run_all();
    stream_ops::run_all();
    suspend_resume::run_all().await;
    syscall_channel_extra::run_all();
    syscall_fifo_extra::run_all();
    syscall_handle_extra::run_all();
    syscall_object::run_all();
    syscall_port_extra::run_all().await;
    syscall_signal_extra::run_all();
    syscall_socket_extra::run_all();
    syscall_transfer_data_extra::run_all().await;
    syscall_vmo_extra::run_all().await;
    task_lifecycle::run_all();
    thread_extra::run_all().await;
    timer_extra::run_all();
    timer_ops::run_all();
    vmar_extra::run_all();
    vmar_ops::run_all();
    vmo_clone_extra::run_all();
    vmo_contiguous::run_all();
    vmo_discardable::run_all().await;
    vmo_extra::run_all();
    vmo_ops::run_all();
    vmo_slice_extra::run_all();
    vmo_zero_extra::run_all();
    zx_assert_extra::run_all();
}
