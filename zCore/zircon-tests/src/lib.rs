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
