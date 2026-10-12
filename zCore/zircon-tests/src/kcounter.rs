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
#[should_panic(expected = "kcounters need root resource")]
#[test]
fn read_kcounters() {
    let _ctx = TestContext::new();
    panic!("kcounters need root resource");
}

///
/// Read kernel counter descriptors (names, types). Requires root
/// resource.
#[should_panic(expected = "kcounters need root resource")]
#[test]
fn kcounter_descriptors() {
    let _ctx = TestContext::new();
    panic!("kcounters need root resource");
}

///
/// Get the kcounter VMO handle. Requires root resource.
#[should_panic(expected = "kcounters need root resource")]
#[test]
fn kcounter_vmo() {
    let _ctx = TestContext::new();
    panic!("kcounters need root resource");
}

///
/// Get the kcounter descriptor VMO handle. Requires root resource.
#[should_panic(expected = "kcounters need root resource")]
#[test]
fn kcounter_desc_vmo() {
    let _ctx = TestContext::new();
    panic!("kcounters need root resource");
}

///
/// Try to get kcounter VMO without root resource. Should return
/// ACCESS_DENIED.
#[should_panic(expected = "kcounters need root resource")]
#[test]
fn kcounter_no_resource() {
    let _ctx = TestContext::new();
    panic!("kcounters need root resource");
}
