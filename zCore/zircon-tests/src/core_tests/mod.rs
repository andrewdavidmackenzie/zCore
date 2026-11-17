//! 1:1 ports of the Fuchsia C++ core-tests-standalone binary.
//!
//! Syscall-layer tests (using UserInPtr/UserOutPtr) have been ported
//! to petal user-mode binaries. The remaining modules here contain
//! kernel-API tests that run in kernel mode, plus helpers used by
//! the extra test modules.
#![allow(dead_code)]
//!
//! Each sub-module corresponds to a C++ test suite directory under
//! `zircon/system/utest/core/`.  Test functions have `/// C++: TEST(...)`
//! doc comments identifying the original C++ test.
//!
//! Coverage: 1778 C++ core-tests.

mod address_tagging;
mod bad_access;
mod bad_segsel;
pub(crate) mod bti;
mod c11_mutex;
mod c11_thread;
mod channel;
mod channel_call;
mod channel_call_mutex;
mod channel_internal;
pub(crate) mod clock;
mod completion_wrapper;
mod conditional_variable;
mod counter;
mod cpp_thread;
mod cpu_mask_profile;
mod debuglog;
// mod event; — empty stub, tests not yet ported
mod event_pair;
pub(crate) mod exception;
mod fifo;
mod futex;
mod handle;
mod handle_wait;
mod hotplug;
mod interrupt;
mod interrupt_crash;
mod iovec;
mod job;
mod membarrier;
mod memory_mapping;
mod memory_priority_profile;
mod msi;
mod mutex_wrapper;
mod next_vdso;
mod object_child;
pub(crate) mod object_info;
mod object_wait;
mod page_size;
pub(crate) mod pager;
mod port;
mod process;
mod process_debug;
pub(crate) mod profile;
pub(crate) mod property;
mod protect;
mod pthread;
mod pthread_barrier;
mod pthread_tls;
mod register_context_switch;
mod register_state;
mod resource;
mod socket;
mod standalone_elf_load;
mod stream;
mod sync_completion;
mod sync_condition;
mod sync_mutex;
pub(crate) mod syscall_channel;
pub(crate) mod syscall_fifo;
mod syscall_generation;
mod syscall_handle;
mod syscall_port;
pub(crate) mod syscall_signal;
pub(crate) mod syscall_socket;
// mod syscall_task; — empty stub, tests not yet ported
pub(crate) mod syscall_transfer_data;
pub(crate) mod syscall_vmo;
mod system_cpu;
mod system_event;
mod system_suspend;
pub(crate) mod thread;
mod thread_initial_state;
mod thread_sampler;
mod ticks;
mod timer;
mod user_exceptions;
mod version_test;
mod vmar;
mod vmo;
mod vmo_clone;
mod vmo_clone2;
mod vmo_reference;
mod vmo_slice;
mod vmo_transfer_data;
mod vmo_zero;
mod wake_reports;

/// Run a test module, logging its name.
macro_rules! run_test {
    ($name:ident) => {{
        log::warn!("  core_test: {}", stringify!($name));
        $name::run_all();
    }};
    (async $name:ident) => {{
        log::warn!("  core_test: {}", stringify!($name));
        $name::run_all().await;
    }};
}

/// Run all core tests.
///
/// Tests are split into two groups:
/// - **Kernel API tests**: use kernel object APIs directly, work everywhere
/// - **Syscall tests**: use `UserInPtr`/`UserOutPtr` through the syscall
///   layer. These pass kernel addresses as "user" buffers, which only works
///   in libos mode (host `cargo test`). On bare-metal, the kernel correctly
///   rejects kernel-space addresses. These tests need to migrate to the
///   petal user-mode binary to run on bare-metal.
pub async fn run_all() {
    // ── Kernel API tests (no UserInPtr/UserOutPtr) ──────────────────
    run_test!(address_tagging);
    run_test!(bad_segsel);
    run_test!(bti);
    run_test!(async c11_mutex);
    run_test!(async c11_thread);
    run_test!(async channel_call_mutex);
    run_test!(async completion_wrapper);
    run_test!(async conditional_variable);
    run_test!(cpp_thread);
    run_test!(exception);
    run_test!(handle);
    run_test!(async interrupt);
    run_test!(interrupt_crash);
    run_test!(membarrier);
    run_test!(memory_mapping);
    run_test!(msi);
    run_test!(mutex_wrapper);
    run_test!(next_vdso);
    run_test!(object_child);
    run_test!(page_size);
    run_test!(async pager);
    run_test!(process_debug);
    run_test!(property);
    run_test!(protect);
    run_test!(async pthread);
    run_test!(pthread_barrier);
    run_test!(pthread_tls);
    run_test!(register_context_switch);
    run_test!(register_state);
    run_test!(resource);
    run_test!(standalone_elf_load);
    run_test!(stream);
    run_test!(async sync_condition);
    run_test!(async sync_mutex);
    run_test!(system_event);
    run_test!(system_suspend);
    run_test!(thread_initial_state);
    run_test!(ticks);
    run_test!(version_test);
    run_test!(vmar);
    run_test!(vmo_clone);
    run_test!(async vmo_clone2);
    run_test!(vmo_reference);
    run_test!(vmo_slice);
    run_test!(vmo_transfer_data);
    run_test!(vmo_zero);

    // ── Syscall tests (use UserInPtr/UserOutPtr) ────────────────────
    // Syscall-layer tests have been ported to petal user-mode binaries.
    // They run on bare-metal QEMU via the petal test infrastructure.
}
