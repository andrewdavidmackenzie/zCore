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
#[macro_use]
extern crate alloc;
mod helpers;

mod prelude;

// ── 1:1 C++ core-test ports ────────────────────────────────────────

mod core_tests;

// ── Tests from other Fuchsia binaries (not core-tests-standalone) ──

mod extra_tests;

// ── zCore-original tests ───────────────────────────────────────────

mod bad_syscall;

mod bti_extra;

mod cache;

mod channel_extra;

mod clock_extra;

mod clock_ops;

mod cprng;

mod debug;

mod debuglog_extra;

mod event_extra;

mod event_pair_extra;

mod exception_extra;

mod exception_ops;

mod fifo_extra;

mod fifo_ops;

mod futex_extra;

mod handle_dup;

mod handle_extra;

mod interrupt_extra;

mod ipc_limits;

mod job_extra;

mod kcounter;

mod memory_mapping_extra;

mod mini_process;

mod namespace;

mod object_info_extra;

mod object_lifecycle;

mod object_properties;

mod object_signal;

mod object_wait_extra;

mod object_wait_ops;

mod page_size_extra;

mod pager_extra;

mod policy;

mod port_extra;

mod port_stress_extra;

mod process_debug_extra;

mod process_extra;

mod profile_extra;

mod property_extra;

mod pthread_tls;

mod resource_extra;

mod rights;

mod socket_disposition;

mod socket_extra;

mod stream_extra;

mod stream_ops;

mod suspend_resume;

mod syscall_channel_extra;

mod syscall_fifo_extra;

mod syscall_handle_extra;

mod syscall_object;

mod syscall_port_extra;

mod syscall_signal_extra;

mod syscall_socket_extra;

mod syscall_transfer_data_extra;

mod syscall_vmo_extra;

mod task_lifecycle;

mod thread_extra;

mod timer_extra;

mod timer_ops;

mod vmar_extra;

mod vmar_ops;

mod vmo_clone_extra;

mod vmo_contiguous;

mod vmo_discardable;

mod vmo_extra;

mod vmo_ops;

mod vmo_slice_extra;

mod vmo_zero_extra;

mod zx_assert_extra;

/// Run all tests (for bare-metal kernel test runner).
#[cfg(not(test))]
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
    vmar_extra::run_all();
    log::warn!("TEST MODULE: vmar_ops");
    // Requires user-mode VMAR access (#817)
    vmar_ops::run_all();
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
