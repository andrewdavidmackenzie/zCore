//! 1:1 ports of the Fuchsia C++ core-tests-standalone binary.
//!
//! Each sub-module corresponds to a C++ test suite directory under
//! `zircon/system/utest/core/`.  Test functions have `/// C++: TEST(...)`
//! doc comments identifying the original C++ test.
//!
//! Coverage: 1778 C++ core-tests, one `#[test]` per binary entry.

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
mod event;
mod event_pair;
pub(crate) mod exception;
// executable_tls uses thread_local! and std::thread which require std.
#[cfg(feature = "libos")]
mod executable_tls;
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
// port_stress uses std::thread, std::sync::Mutex, std::sync::Barrier
// for multi-threaded stress testing — requires std.
#[cfg(feature = "libos")]
mod port_stress;
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
// stack uses thread_local! which requires std.
#[cfg(feature = "libos")]
mod stack;
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
mod syscall_task;
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
// zx_assert_c uses std::panic::catch_unwind which requires std.
#[cfg(feature = "libos")]
mod zx_assert_c;

/// Run all core tests.
#[cfg(feature = "baremetal-test")]
pub async fn run_all() {
    address_tagging::run_all();
    bad_access::run_all();
    bad_segsel::run_all();
    bti::run_all();
    c11_mutex::run_all().await;
    c11_thread::run_all().await;
    channel::run_all().await;
    channel_call::run_all().await;
    channel_call_mutex::run_all().await;
    channel_internal::run_all().await;
    clock::run_all();
    completion_wrapper::run_all().await;
    conditional_variable::run_all().await;
    counter::run_all();
    cpp_thread::run_all();
    cpu_mask_profile::run_all();
    debuglog::run_all();
    event_pair::run_all().await;
    exception::run_all();
    fifo::run_all();
    futex::run_all().await;
    handle::run_all();
    handle_wait::run_all().await;
    hotplug::run_all();
    interrupt::run_all().await;
    interrupt_crash::run_all();
    iovec::run_all().await;
    job::run_all();
    membarrier::run_all();
    memory_mapping::run_all();
    memory_priority_profile::run_all();
    msi::run_all();
    mutex_wrapper::run_all();
    next_vdso::run_all();
    object_child::run_all();
    object_info::run_all();
    object_wait::run_all().await;
    page_size::run_all();
    pager::run_all().await;
    port::run_all().await;
    process::run_all().await;
    process_debug::run_all();
    profile::run_all();
    property::run_all();
    protect::run_all();
    pthread::run_all().await;
    pthread_barrier::run_all();
    pthread_tls::run_all();
    register_context_switch::run_all();
    register_state::run_all();
    resource::run_all();
    socket::run_all();
    standalone_elf_load::run_all();
    stream::run_all();
    sync_completion::run_all().await;
    sync_condition::run_all().await;
    sync_mutex::run_all().await;
    syscall_channel::run_all().await;
    syscall_fifo::run_all();
    syscall_generation::run_all();
    syscall_handle::run_all();
    syscall_port::run_all();
    syscall_socket::run_all();
    syscall_transfer_data::run_all();
    syscall_vmo::run_all();
    system_cpu::run_all();
    system_event::run_all();
    system_suspend::run_all();
    thread::run_all().await;
    thread_initial_state::run_all();
    thread_sampler::run_all();
    ticks::run_all();
    timer::run_all().await;
    user_exceptions::run_all();
    version_test::run_all();
    vmar::run_all();
    vmo::run_all().await;
    vmo_clone::run_all();
    vmo_clone2::run_all().await;
    vmo_reference::run_all();
    vmo_slice::run_all();
    vmo_transfer_data::run_all();
    vmo_zero::run_all();
    wake_reports::run_all().await;
}
