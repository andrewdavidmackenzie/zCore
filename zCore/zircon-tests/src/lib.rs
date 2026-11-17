//! Rust-native Zircon syscall test suite.
//!
//! Syscall-layer tests (using UserInPtr/UserOutPtr) have been ported
//! to petal user-mode binaries. The remaining modules here contain
//! kernel-API tests plus helpers used by cross-module references.
//! Functions in ported modules are kept for the host `cargo test`
//! path but are not called from `run_all()` on bare-metal.
#![allow(dead_code)]
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

/// Log and run a test module.
macro_rules! run_module {
    ($name:ident) => {{
        log::warn!("TEST MODULE: {}", stringify!($name));
        $name::run_all();
    }};
    (async $name:ident) => {{
        log::warn!("TEST MODULE: {}", stringify!($name));
        $name::run_all().await;
    }};
}

/// Run all tests (for bare-metal kernel test runner).
///
/// Tests are split into kernel API tests (run everywhere) and syscall
/// tests (need user-mode buffers, only run in libos/host mode).
/// Syscall tests will migrate to the petal user-mode binary.
pub async fn run_all() {
    // core_tests and extra_tests handle the kernel/syscall split internally
    run_module!(async core_tests);
    run_module!(async extra_tests);

    // ── Kernel API modules (no UserInPtr/UserOutPtr) ────────────────
    run_module!(bti_extra);
    run_module!(cache);
    run_module!(channel_extra);
    run_module!(debuglog_extra);
    run_module!(event_extra);
    run_module!(event_pair_extra);
    run_module!(exception_ops);
    run_module!(fifo_extra);
    run_module!(fifo_ops);
    run_module!(futex_extra);
    run_module!(handle_dup);
    run_module!(handle_extra);
    run_module!(interrupt_extra);
    run_module!(ipc_limits);
    run_module!(job_extra);
    run_module!(kcounter);
    run_module!(memory_mapping_extra);
    run_module!(namespace);
    run_module!(object_lifecycle);
    run_module!(object_properties);
    run_module!(object_signal);
    run_module!(async object_wait_ops);
    run_module!(page_size_extra);
    run_module!(async pager_extra);
    run_module!(policy);
    run_module!(async port_extra);
    run_module!(async port_stress_extra);
    run_module!(process_debug_extra);
    run_module!(process_extra);
    run_module!(profile_extra);
    run_module!(property_extra);
    run_module!(pthread_tls);
    run_module!(resource_extra);
    run_module!(rights);
    run_module!(socket_disposition);
    run_module!(socket_extra);
    run_module!(stream_extra);
    run_module!(stream_ops);
    run_module!(async suspend_resume);
    run_module!(task_lifecycle);
    run_module!(timer_ops);
    run_module!(vmar_extra);
    run_module!(vmar_ops);
    run_module!(vmo_clone_extra);
    run_module!(vmo_contiguous);
    run_module!(vmo_ops);
    run_module!(vmo_slice_extra);
    run_module!(vmo_zero_extra);
    run_module!(zx_assert_extra);

    // ── Syscall modules (use UserInPtr/UserOutPtr) ──────────────────
    // These pass kernel addresses through syscall user-pointer validation.
    // On bare-metal, kernel addresses are correctly rejected by VMAR
    // validation. These tests only run in libos mode (host `cargo test`).
    // They need to migrate to the petal user-mode binary.
    // Syscall-layer tests have been ported to petal user-mode binaries.
    // They run on bare-metal QEMU via the petal test infrastructure.
}
