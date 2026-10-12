//! Kernel counter tests.
//!
//! Ports of Fuchsia's kernel counter (kcounter) tests. Kernel
//! counters provide statistics about kernel operations (page faults,
//! syscall counts, etc.). These are kernel-internal and not exposed
//! through the standard object API.

#[allow(unused_imports)]
use crate::helpers::TestContext;

///
/// Read kernel counter values. Requires root resource and the
/// kcounter VMO.
#[test]
#[ignore = "kcounters require root resource and kcounter VMO (not implemented)"]
fn read_kcounters() {
    // Kernel counters are not implemented in zCore.
}

///
/// Read kernel counter descriptors (names, types). Requires root
/// resource.
#[test]
#[ignore = "kcounters require root resource and kcounter VMO (not implemented)"]
fn kcounter_descriptors() {
    // Kernel counters are not implemented in zCore.
}

///
/// Get the kcounter VMO handle. Requires root resource.
#[test]
#[ignore = "kcounters require root resource and kcounter VMO (not implemented)"]
fn kcounter_vmo() {
    // Kernel counters are not implemented in zCore.
}

///
/// Get the kcounter descriptor VMO handle. Requires root resource.
#[test]
#[ignore = "kcounters require root resource and kcounter VMO (not implemented)"]
fn kcounter_desc_vmo() {
    // Kernel counters are not implemented in zCore.
}

///
/// Try to get kcounter VMO without root resource. Should return
/// ACCESS_DENIED.
#[test]
#[ignore = "kcounters require root resource and kcounter VMO (not implemented)"]
fn kcounter_no_resource() {
    // Kernel counters are not implemented in zCore.
}
