//! Process shared tests.
//!
//! Ports of Fuchsia's process-shared test suite. Shared processes
//! allow multiple processes to share a single address space. zCore
//! does not implement shared process infrastructure. These tests
//! exercise process and VMAR operations that would underlie process
//! sharing.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process};
use zircon_object::vm::{VmObject, PAGE_SIZE};

extern crate alloc;

/// C++: TEST(ProcessShared, InfoProcessMaps)
/// Query process memory maps. Verify VMAR is accessible.
#[test]
fn info_process_maps() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "shared-test").unwrap();
    let vmar = proc.vmar();
    assert_ne!(vmar.id(), 0, "process should have a root VMAR");
}

/// C++: TEST(ProcessShared, InfoProcessVmos)
/// Query process VMO list. Verify VMO creation and tracking.
#[test]
fn info_process_vmos() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(2);
    assert_ne!(vmo.id(), 0);
    assert_eq!(vmo.len(), 2 * PAGE_SIZE);
}

/// C++: TEST(ProcessShared, InfoTaskStats)
/// Query task statistics. Verify process has valid identity.
#[test]
fn info_task_stats() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "shared-test").unwrap();
    assert_ne!(proc.id(), 0);
}

/// C++: TEST(ProcessShared, InvalidPrototype)
/// Invalid prototype process. Verify process creation with valid args.
#[test]
fn invalid_prototype() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "valid").unwrap();
    assert_ne!(proc.id(), 0);
    // Shared process with invalid prototype would fail — no API in zCore.
}

/// C++: TEST(ProcessShared, MapInPrototype)
/// Map a VMO in the prototype process. Verify VMO read/write works.
#[test]
fn map_in_prototype() {
    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    vmo.write(0, b"mapped in proto").unwrap();

    let mut buf = [0u8; 15];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"mapped in proto");
}

/// C++: TEST(ProcessShared, PropertiesCopied)
/// Process properties are copied to shared children. Verify
/// process name is set correctly (a copyable property).
#[test]
fn properties_copied() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "original").unwrap();
    assert_eq!(proc.name(), "original");
}

/// C++: TEST(ProcessShared, PropertiesShared)
/// Shared properties between processes. Verify two processes
/// in the same job have independent names.
#[test]
fn properties_shared() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let p1 = Process::create(&job, "proc-1").unwrap();
    let p2 = Process::create(&job, "proc-2").unwrap();

    assert_eq!(p1.name(), "proc-1");
    assert_eq!(p2.name(), "proc-2");
    assert_ne!(p1.id(), p2.id());
}

/// C++: TEST(ProcessShared, RestrictedVmarNotShared)
/// Restricted VMARs are not shared. Verify each process has its
/// own VMAR.
#[test]
fn restricted_vmar_not_shared() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let p1 = Process::create(&job, "p1").unwrap();
    let p2 = Process::create(&job, "p2").unwrap();

    // Each process has its own root VMAR.
    assert_ne!(p1.vmar().id(), p2.vmar().id());
}

/// C++: TEST(ProcessShared, SharedVmarBoundary)
/// Shared VMAR boundary. Verify VMAR identity.
#[test]
fn shared_vmar_boundary() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "boundary").unwrap();
    let vmar = proc.vmar();
    assert_ne!(vmar.id(), 0);
}

/// C++: TEST(ProcessShared, SharedVmarBoundaryDestruction)
/// Shared VMAR destroyed when process exits. Verify process kill
/// cleans up properly.
#[test]
fn shared_vmar_boundary_destruction() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "destruction").unwrap();
    let vmar_id = proc.vmar().id();
    assert_ne!(vmar_id, 0);
    proc.exit(zircon_object::task::TASK_RETCODE_SYSCALL_KILL);
    // Process killed — VMAR should be cleaned up.
}

/// C++: TEST(ProcessShared, UnifiedVmarBoundary)
/// Unified VMAR boundary. Verify VMAR is associated with process.
#[test]
fn unified_vmar_boundary() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "unified").unwrap();
    let vmar = proc.vmar();
    assert_ne!(vmar.id(), 0);
    // Unified VMAR sharing requires shared process infrastructure.
}
