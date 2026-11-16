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

#![no_std]

#[cfg_attr(not(any(test, feature = "baremetal-test")), allow(unused_imports))]
#[macro_use]
extern crate alloc;

#[cfg(any(test, feature = "baremetal-test"))]
mod helpers;
#[cfg(any(test, feature = "baremetal-test"))]
mod prelude;

// ── 1:1 C++ core-test ports ────────────────────────────────────────
#[cfg(any(test, feature = "baremetal-test"))]
mod core_tests;
#[cfg(any(test, feature = "baremetal-test"))]
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
    log::warn!("TEST MODULE: core_tests");
    core_tests::run_all().await;
    log::warn!("TEST MODULE: extra_tests");
    extra_tests::run_all().await;
    log::warn!("TEST MODULE: bad_syscall");
    bad_syscall::run_all();
    log::warn!("TEST MODULE: bti_extra");
    bti_extra::run_all();
    log::warn!("TEST MODULE: cache");
    cache::run_all();
    log::warn!("TEST MODULE: channel_extra");
    channel_extra::run_all();
    log::warn!("TEST MODULE: clock_extra");
    clock_extra::run_all();
    log::warn!("TEST MODULE: clock_ops");
    clock_ops::run_all();
    log::warn!("TEST MODULE: cprng");
    cprng::run_all();
    log::warn!("TEST MODULE: debug");
    debug::run_all();
    log::warn!("TEST MODULE: debuglog_extra");
    debuglog_extra::run_all();
    log::warn!("TEST MODULE: event_extra");
    event_extra::run_all();
    log::warn!("TEST MODULE: event_pair_extra");
    event_pair_extra::run_all();
    log::warn!("TEST MODULE: exception_extra");
    exception_extra::run_all();
    log::warn!("TEST MODULE: exception_ops");
    exception_ops::run_all();
    log::warn!("TEST MODULE: fifo_extra");
    fifo_extra::run_all();
    log::warn!("TEST MODULE: fifo_ops");
    fifo_ops::run_all();
    log::warn!("TEST MODULE: futex_extra");
    futex_extra::run_all();
    log::warn!("TEST MODULE: handle_dup");
    handle_dup::run_all();
    log::warn!("TEST MODULE: handle_extra");
    handle_extra::run_all();
    log::warn!("TEST MODULE: interrupt_extra");
    interrupt_extra::run_all();
    log::warn!("TEST MODULE: ipc_limits");
    ipc_limits::run_all();
    log::warn!("TEST MODULE: job_extra");
    job_extra::run_all();
    log::warn!("TEST MODULE: kcounter");
    kcounter::run_all();
    log::warn!("TEST MODULE: memory_mapping_extra");
    memory_mapping_extra::run_all();
    log::warn!("TEST MODULE: mini_process");
    mini_process::run_all();
    log::warn!("TEST MODULE: namespace");
    namespace::run_all();
    log::warn!("TEST MODULE: object_info_extra");
    object_info_extra::run_all();
    log::warn!("TEST MODULE: object_lifecycle");
    object_lifecycle::run_all();
    log::warn!("TEST MODULE: object_properties");
    object_properties::run_all();
    log::warn!("TEST MODULE: object_signal");
    object_signal::run_all();
    log::warn!("TEST MODULE: object_wait_extra");
    object_wait_extra::run_all().await;
    log::warn!("TEST MODULE: object_wait_ops");
    object_wait_ops::run_all().await;
    log::warn!("TEST MODULE: page_size_extra");
    page_size_extra::run_all();
    log::warn!("TEST MODULE: pager_extra");
    pager_extra::run_all().await;
    log::warn!("TEST MODULE: policy");
    policy::run_all();
    log::warn!("TEST MODULE: port_extra");
    port_extra::run_all().await;
    log::warn!("TEST MODULE: port_stress_extra");
    port_stress_extra::run_all().await;
    log::warn!("TEST MODULE: process_debug_extra");
    process_debug_extra::run_all();
    log::warn!("TEST MODULE: process_extra");
    process_extra::run_all();
    log::warn!("TEST MODULE: profile_extra");
    profile_extra::run_all();
    log::warn!("TEST MODULE: property_extra");
    property_extra::run_all();
    log::warn!("TEST MODULE: pthread_tls");
    pthread_tls::run_all();
    log::warn!("TEST MODULE: resource_extra");
    resource_extra::run_all();
    log::warn!("TEST MODULE: rights");
    rights::run_all();
    log::warn!("TEST MODULE: socket_disposition");
    socket_disposition::run_all();
    log::warn!("TEST MODULE: socket_extra");
    socket_extra::run_all();
    log::warn!("TEST MODULE: stream_extra");
    stream_extra::run_all();
    log::warn!("TEST MODULE: stream_ops");
    stream_ops::run_all();
    log::warn!("TEST MODULE: suspend_resume");
    suspend_resume::run_all().await;
    log::warn!("TEST MODULE: syscall_channel_extra");
    syscall_channel_extra::run_all();
    log::warn!("TEST MODULE: syscall_fifo_extra");
    syscall_fifo_extra::run_all();
    log::warn!("TEST MODULE: syscall_handle_extra");
    syscall_handle_extra::run_all();
    log::warn!("TEST MODULE: syscall_object");
    syscall_object::run_all();
    log::warn!("TEST MODULE: syscall_port_extra");
    syscall_port_extra::run_all().await;
    log::warn!("TEST MODULE: syscall_signal_extra");
    syscall_signal_extra::run_all();
    log::warn!("TEST MODULE: syscall_socket_extra");
    syscall_socket_extra::run_all();
    log::warn!("TEST MODULE: syscall_transfer_data_extra");
    syscall_transfer_data_extra::run_all().await;
    log::warn!("TEST MODULE: syscall_vmo_extra");
    syscall_vmo_extra::run_all().await;
    log::warn!("TEST MODULE: task_lifecycle");
    task_lifecycle::run_all();
    log::warn!("TEST MODULE: thread_extra");
    thread_extra::run_all().await;
    log::warn!("TEST MODULE: timer_extra");
    timer_extra::run_all();
    log::warn!("TEST MODULE: timer_ops");
    timer_ops::run_all();
    log::warn!("TEST MODULE: vmar_extra");
    // Requires user-mode VMAR access (#817)
    // vmar_extra::run_all();
    log::warn!("TEST MODULE: vmar_ops");
    // Requires user-mode VMAR access (#817)
    // vmar_ops::run_all();
    log::warn!("TEST MODULE: vmo_clone_extra");
    vmo_clone_extra::run_all();
    log::warn!("TEST MODULE: vmo_contiguous");
    vmo_contiguous::run_all();
    log::warn!("TEST MODULE: vmo_discardable");
    vmo_discardable::run_all().await;
    log::warn!("TEST MODULE: vmo_extra");
    vmo_extra::run_all();
    log::warn!("TEST MODULE: vmo_ops");
    vmo_ops::run_all();
    log::warn!("TEST MODULE: vmo_slice_extra");
    vmo_slice_extra::run_all();
    log::warn!("TEST MODULE: vmo_zero_extra");
    vmo_zero_extra::run_all();
    log::warn!("TEST MODULE: zx_assert_extra");
    zx_assert_extra::run_all();
}
