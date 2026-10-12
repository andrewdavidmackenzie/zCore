//! Rust-native Zircon syscall test suite.
//!
//! This crate tests Zircon kernel syscalls through the full dispatch
//! path (zircon-syscall → zircon-object) in libos mode. Tests run on
//! the host with `cargo test -p zircon-tests`, without QEMU.
//!
//! # Architecture
//!
//! Each test creates a minimal Zircon process context using
//! [`helpers::TestContext::new()`], which exercises the same
//! kernel code path as real Zircon userspace programs.

#[cfg(test)]
mod helpers;

#[cfg(test)]
mod channel;

#[cfg(test)]
mod vmo;

#[cfg(test)]
mod socket;

#[cfg(test)]
mod event;

#[cfg(test)]
mod fifo;

#[cfg(test)]
mod port;

#[cfg(test)]
mod timer;

#[cfg(test)]
mod handle;

#[cfg(test)]
mod job;

#[cfg(test)]
mod process;

#[cfg(test)]
mod thread;

#[cfg(test)]
mod futex;

#[cfg(test)]
mod stream;

#[cfg(test)]
mod vmo_clone;

#[cfg(test)]
mod vmo_clone2;

#[cfg(test)]
mod vmo_slice;

#[cfg(test)]
mod vmo_reference;

#[cfg(test)]
mod vmo_zero;

#[cfg(test)]
mod vmar;

#[cfg(test)]
mod clock;

#[cfg(test)]
mod counter;

#[cfg(test)]
mod resource;

#[cfg(test)]
mod interrupt;

#[cfg(test)]
mod bti;

#[cfg(test)]
mod exception;

#[cfg(test)]
mod pager;

#[cfg(test)]
mod debuglog;

#[cfg(test)]
mod skipped;

#[cfg(test)]
mod syscall_channel;

#[cfg(test)]
mod syscall_fifo;

#[cfg(test)]
mod syscall_handle;

#[cfg(test)]
mod syscall_object;

#[cfg(test)]
mod syscall_signal;

#[cfg(test)]
mod syscall_port;

#[cfg(test)]
mod syscall_socket;

#[cfg(test)]
mod syscall_task;

#[cfg(test)]
mod syscall_vmo;

#[cfg(test)]
mod object_info;

#[cfg(test)]
mod property;

#[cfg(test)]
mod vmo_transfer_data;

#[cfg(test)]
mod syscall_transfer_data;

#[cfg(test)]
mod bad_syscall;

#[cfg(test)]
mod sync_mutex;

#[cfg(test)]
mod object_wait;

#[cfg(test)]
mod profile;

#[cfg(test)]
mod address_tagging;

#[cfg(test)]
mod c11_thread;

#[cfg(test)]
mod c11_mutex;

#[cfg(test)]
mod system_event;

#[cfg(test)]
mod memory_mapping;

#[cfg(test)]
mod cprng;

#[cfg(test)]
mod mini_process;

#[cfg(test)]
mod policy;

#[cfg(test)]
mod rights;

#[cfg(test)]
mod debug;

#[cfg(test)]
mod namespace;

#[cfg(test)]
mod pthread;

#[cfg(test)]
mod cache;

#[cfg(test)]
mod object_signal;

#[cfg(test)]
mod object_properties;

#[cfg(test)]
mod event_pair;

#[cfg(test)]
mod handle_dup;

#[cfg(test)]
mod vmo_contiguous;

#[cfg(test)]
mod vmo_discardable;

#[cfg(test)]
mod task_lifecycle;

#[cfg(test)]
mod vmar_ops;

#[cfg(test)]
mod port_stress;

#[cfg(test)]
mod channel_call;

#[cfg(test)]
mod vmo_ops;

#[cfg(test)]
mod fifo_ops;

#[cfg(test)]
mod timer_ops;

#[cfg(test)]
mod stream_ops;

#[cfg(test)]
mod socket_disposition;

#[cfg(test)]
mod exception_ops;

#[cfg(test)]
mod object_lifecycle;

#[cfg(test)]
mod ipc_limits;

#[cfg(test)]
mod suspend_resume;

#[cfg(test)]
mod kcounter;

#[cfg(test)]
mod object_info_extra;

#[cfg(test)]
mod object_wait_ops;

#[cfg(test)]
mod clock_ops;

#[cfg(test)]
mod iovec;

#[cfg(test)]
mod iob;

// ── New test modules ported from Fuchsia C++ core-tests ────────────

#[cfg(test)]
mod bad_access;

#[cfg(test)]
mod bad_segsel;

#[cfg(test)]
mod channel_call_mutex;

#[cfg(test)]
mod channel_internal;

#[cfg(test)]
mod completion_wrapper;

#[cfg(test)]
mod conditional_variable;

#[cfg(test)]
mod cpp_thread;

#[cfg(test)]
mod cpu_features;

#[cfg(test)]
mod cpu_mask_profile;

#[cfg(test)]
mod datagram_socket_bad_buffer;

#[cfg(test)]
mod exceptions_arch;

#[cfg(test)]
mod executable_tls;

#[cfg(test)]
mod hotplug;

#[cfg(test)]
mod interrupt_crash;

#[cfg(test)]
mod iob_shared_region;

#[cfg(test)]
mod membarrier;

#[cfg(test)]
mod memory_priority_profile;

#[cfg(test)]
mod msi;

#[cfg(test)]
mod mutex_wrapper;

#[cfg(test)]
mod next_vdso;

#[cfg(test)]
mod object_child;

#[cfg(test)]
mod page_size;

#[cfg(test)]
mod process_debug;

#[cfg(test)]
mod process_shared;

#[cfg(test)]
mod protect;

#[cfg(test)]
mod pthread_barrier;

#[cfg(test)]
mod register_context_switch;

#[cfg(test)]
mod register_state;

#[cfg(test)]
mod restricted_mode;

#[cfg(test)]
mod rseq;

#[cfg(test)]
mod stack;

#[cfg(test)]
mod standalone_elf_load;

#[cfg(test)]
mod sync_completion;

#[cfg(test)]
mod sync_condition;

#[cfg(test)]
mod syscall_generation;

#[cfg(test)]
mod system_cpu;

#[cfg(test)]
mod system_suspend;

#[cfg(test)]
mod thread_initial_state;

#[cfg(test)]
mod thread_sampler;

#[cfg(test)]
mod ticks;

#[cfg(test)]
mod user_exceptions;

#[cfg(test)]
mod version_test;

#[cfg(test)]
mod wake_reports;

#[cfg(test)]
mod zx_assert;

#[cfg(test)]
mod zx_assert_c;

#[cfg(test)]
mod handle_wait;

#[cfg(test)]
mod pthread_tls;
