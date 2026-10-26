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
