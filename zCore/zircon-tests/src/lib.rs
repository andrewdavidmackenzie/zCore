//! Rust-native Zircon syscall test suite.
//!
//! This crate contains 3209 tests organized into two categories:
//!
//! ## `core_tests` — 1:1 C++ core-test ports (~1800 tests)
//!
//! Faithful ports of the Fuchsia `core-tests-standalone` binary.
//! Each sub-module maps to a C++ test suite directory under
//! `zircon/system/utest/core/`.  Test functions have
//! `/// C++: TEST(Suite, Name)` doc comments.
//!
//! Coverage: 99% of the 1778 C++ core-tests are ported.
//!
//! ## Top-level modules — zCore-original tests (~1400 tests)
//!
//! Additional tests that exercise kernel-object-level APIs, syscall
//! dispatch, and zCore-specific behavior not covered by the C++ suite.
//!
//! # Running
//!
//! ```sh
//! cargo test -p zircon-tests --no-fail-fast -- --test-threads=1
//! ```

#[cfg(test)]
mod helpers;

// ── 1:1 C++ core-test ports ────────────────────────────────────────
#[cfg(test)]
mod core_tests;

// ── zCore-original tests ───────────────────────────────────────────
#[cfg(test)]
mod bad_syscall;
#[cfg(test)]
mod cache;
#[cfg(test)]
mod clock_ops;
#[cfg(test)]
mod cprng;
#[cfg(test)]
mod debug;
#[cfg(test)]
mod exception_ops;
#[cfg(test)]
mod fifo_ops;
#[cfg(test)]
mod handle_dup;
#[cfg(test)]
mod ipc_limits;
#[cfg(test)]
mod kcounter;
#[cfg(test)]
mod mini_process;
#[cfg(test)]
mod namespace;
#[cfg(test)]
mod object_info_extra;
#[cfg(test)]
mod object_lifecycle;
#[cfg(test)]
mod object_properties;
#[cfg(test)]
mod object_signal;
#[cfg(test)]
mod object_wait_ops;
#[cfg(test)]
mod policy;
#[cfg(test)]
mod pthread_tls;
#[cfg(test)]
mod rights;
#[cfg(test)]
mod socket_disposition;
#[cfg(test)]
mod stream_ops;
#[cfg(test)]
mod suspend_resume;
#[cfg(test)]
mod syscall_object;
#[cfg(test)]
#[cfg(test)]
mod task_lifecycle;
#[cfg(test)]
mod timer_ops;
#[cfg(test)]
mod vmar_ops;
#[cfg(test)]
mod vmo_contiguous;
#[cfg(test)]
mod vmo_discardable;
#[cfg(test)]
mod vmo_ops;
