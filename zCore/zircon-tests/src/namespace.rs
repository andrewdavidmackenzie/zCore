//! Namespace tests.
//!
//! Ports of Fuchsia's namespace-related tests. Namespaces provide
//! a hierarchical directory structure for capability routing in
//! Fuchsia's component model. Not directly applicable to the Zircon
//! kernel layer -- there is no Namespace kernel object in zCore.
//!
//! All tests remain `#[ignore]` because namespaces are a component
//! model abstraction layered above the Zircon kernel.

///
/// Create an empty namespace. Should succeed with zero entries.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn create_empty() {
    todo!("namespaces are component model");
}

///
/// Add a directory entry to a namespace. Requires fidl/component.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn add_entry() {
    todo!("namespaces are component model");
}

///
/// Look up an entry in a namespace by path. Requires fidl/component.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn lookup_entry() {
    todo!("namespaces are component model");
}

///
/// Remove an entry from a namespace. Requires fidl/component.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn remove_entry() {
    todo!("namespaces are component model");
}

///
/// Add an entry with an invalid path (empty, no leading slash).
/// Should return INVALID_ARGS.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn invalid_path() {
    todo!("namespaces are component model");
}

///
/// Add two entries with the same path. Should return ALREADY_EXISTS.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn duplicate_path() {
    todo!("namespaces are component model");
}

///
/// Add entries with nested paths (/svc, /svc/fuchsia.logger.Log).
/// Both should be independently resolvable.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn nested_paths() {
    todo!("namespaces are component model");
}

///
/// Enumerate all entries in a namespace. Should return them in
/// insertion order. Requires fidl/component.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn enumerate_entries() {
    todo!("namespaces are component model");
}

///
/// Create, populate, use, and destroy a namespace. All entries
/// should be properly cleaned up on destruction.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn namespace_lifecycle() {
    todo!("namespaces are component model");
}

///
/// Transfer a namespace to a child process via process_create.
/// The child should see the same entries. Requires process execution.
#[test]
#[ignore = "namespaces are component model, not Zircon kernel"]
fn transfer_namespace() {
    todo!("namespaces are component model");
}
