//! VMAR syscall tests.
//!
//! Note: VMAR allocate/map operations require a real page table
//! setup which may not work fully in libos mode. Some tests are
//! expected to fail until libos page table emulation improves.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process};

/// Process root VMAR exists and has valid properties.
#[test]
fn vmar_root_exists() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "vmar-test").unwrap();
    let vmar = proc.vmar();
    assert_ne!(vmar.id(), 0);
    assert_ne!(vmar.addr(), 0);
}
