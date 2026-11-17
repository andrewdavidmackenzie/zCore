//! Tests from other Fuchsia test binaries (not core-tests-standalone).
//!
//! These test suites exist in the Fuchsia tree but are compiled into
//! separate binaries (IOB tests, RSEQ tests, restricted-mode tests, etc.).
//! They are kept here for reference and future use.

mod address_tagging_tbi;
mod cpu_features;
mod exceptions_arch;
mod iob;
mod iob_shared_region;
mod process_shared;
mod restricted_mode;
mod rseq;
mod thread_extra;

/// Run all extra tests.
pub async fn run_all() {
    // Kernel API tests
    address_tagging_tbi::run_all();
    cpu_features::run_all();
    iob::run_all();
    iob_shared_region::run_all();
    restricted_mode::run_all();
    rseq::run_all();

    // Syscall tests (need user-mode buffers)
    // Syscall-layer tests ported to petal user-mode binaries.
}
