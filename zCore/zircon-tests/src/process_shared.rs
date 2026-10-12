//! Process shared tests.
//!
//! Ports of Fuchsia's process-shared test suite. Shared processes
//! allow multiple processes to share a single address space.

#[allow(unused_imports)]
use crate::helpers::TestContext;

/// C++: TEST(ProcessShared, InfoProcessMaps)
#[test]
#[ignore = "requires shared process infrastructure"]
fn info_process_maps() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, InfoProcessVmos)
#[test]
#[ignore = "requires shared process infrastructure"]
fn info_process_vmos() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, InfoTaskStats)
#[test]
#[ignore = "requires shared process infrastructure"]
fn info_task_stats() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, InvalidPrototype)
#[test]
#[ignore = "requires shared process infrastructure"]
fn invalid_prototype() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, MapInPrototype)
#[test]
#[ignore = "requires shared process infrastructure"]
fn map_in_prototype() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, PropertiesCopied)
#[test]
#[ignore = "requires shared process infrastructure"]
fn properties_copied() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, PropertiesShared)
#[test]
#[ignore = "requires shared process infrastructure"]
fn properties_shared() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, RestrictedVmarNotShared)
#[test]
#[ignore = "requires shared process infrastructure"]
fn restricted_vmar_not_shared() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, SharedVmarBoundary)
#[test]
#[ignore = "requires shared process infrastructure"]
fn shared_vmar_boundary() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, SharedVmarBoundaryDestruction)
#[test]
#[ignore = "requires shared process infrastructure"]
fn shared_vmar_boundary_destruction() {
    todo!("requires shared process infrastructure");
}

/// C++: TEST(ProcessShared, UnifiedVmarBoundary)
#[test]
#[ignore = "requires shared process infrastructure"]
fn unified_vmar_boundary() {
    todo!("requires shared process infrastructure");
}
