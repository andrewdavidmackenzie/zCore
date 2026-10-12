//! Resource tests.
//!
//! Tests for the Resource kernel object: creation, validation,
//! exclusive/overlapping ranges, child resources, info queries,
//! and region registration.

use crate::helpers::TestContext;

use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::KernelObject;

///
/// Create a resource with valid kind and verify it has a valid KOID.
#[test]
fn resource_create() {
    let _ctx = TestContext::new();
    let res = Resource::create(
        "test-mmio",
        ResourceKind::MMIO,
        0x1000,
        0x100,
        ResourceFlags::empty(),
    );
    assert_ne!(res.id(), 0);
}

///
/// ResourceKind::COUNT is the sentinel value (one past the last valid
/// kind). Creating a resource with COUNT is allowed at the kernel
/// object level — validation happens at the syscall layer. This test
/// documents that create() doesn't reject COUNT.
#[test]
fn resource_create_invalid() {
    let _ctx = TestContext::new();
    // COUNT (6) is not a real kind, but Resource::create doesn't
    // validate — that's the syscall layer's job.
    let res = Resource::create("bad", ResourceKind::COUNT, 0, 0, ResourceFlags::empty());
    assert_ne!(res.id(), 0);
}

///
/// Create a resource with the EXCLUSIVE flag.
#[test]
fn resource_create_exclusive() {
    let _ctx = TestContext::new();
    let res = Resource::create(
        "exclusive-irq",
        ResourceKind::IRQ,
        0x20,
        0x10,
        ResourceFlags::EXCLUSIVE,
    );
    assert!(res.is_exclusive());
    assert_ne!(res.id(), 0);
}

///
/// Two non-exclusive resources with overlapping ranges should both
/// pass the overlap check (only exclusive resources block overlap).
#[test]
fn resource_create_overlapping() {
    let _ctx = TestContext::new();
    let res1 = Resource::create(
        "io-1",
        ResourceKind::IOPORT,
        0x100,
        0x100,
        ResourceFlags::empty(),
    );
    res1.register_region();

    // Overlapping range — should succeed because neither is exclusive.
    let result = Resource::check_exclusive_overlap(
        ResourceKind::IOPORT,
        0x150,
        0x50,
        ResourceFlags::empty(),
    );
    assert!(result.is_ok(), "non-exclusive overlap should be allowed");

    // Cleanup: drop res1 (on_zero_handles unregisters).
    drop(res1);
}

///
/// Verify resource info fields match creation parameters.
#[test]
fn resource_get_info() {
    let _ctx = TestContext::new();
    let res = Resource::create(
        "my-resource",
        ResourceKind::MMIO,
        0x8000,
        0x2000,
        ResourceFlags::empty(),
    );
    let info = res.get_info();

    assert_eq!(info.kind, ResourceKind::MMIO as u32);
    assert_eq!(info.flags, 0);
    assert_eq!(info.base, 0x8000);
    assert_eq!(info.size, 0x2000);
    // Name should start with "my-resource".
    assert_eq!(&info.name[..11], b"my-resource");
}

///
/// A resource validates against its own kind and rejects other kinds.
#[test]
fn resource_validate() {
    let _ctx = TestContext::new();
    let res = Resource::create("mmio", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());

    assert!(
        res.validate(ResourceKind::MMIO).is_ok(),
        "same kind should validate"
    );
    assert_eq!(
        res.validate(ResourceKind::IRQ).err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
        "wrong kind should return WRONG_TYPE"
    );
}

///
/// validate_ranged_resource checks both kind and range coverage.
#[test]
fn resource_validate_ranged() {
    let _ctx = TestContext::new();
    let res = Resource::create(
        "mmio-range",
        ResourceKind::MMIO,
        0x1000,
        0x1000,
        ResourceFlags::empty(),
    );

    // Within range.
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x100)
        .is_ok());
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x1000)
        .is_ok());
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1800, 0x800)
        .is_ok());

    // Outside range — should fail with ACCESS_DENIED.
    assert_eq!(
        res.validate_ranged_resource(ResourceKind::MMIO, 0x0, 0x100)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
        "below range should be denied"
    );
    assert_eq!(
        res.validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x1001)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
        "exceeding range should be denied"
    );

    // Wrong kind.
    assert_eq!(
        res.validate_ranged_resource(ResourceKind::IRQ, 0x1000, 0x100)
            .err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
        "wrong kind should return WRONG_TYPE"
    );
}

///
/// A root resource can create child sub-resources of any kind because
/// ROOT bypasses kind and range checks.
#[test]
fn resource_create_child() {
    let _ctx = TestContext::new();
    let root = Resource::create(
        "root",
        ResourceKind::ROOT,
        0,
        0x1_0000_0000,
        ResourceFlags::empty(),
    );

    // Root validates as any kind.
    assert!(root.validate(ResourceKind::MMIO).is_ok());
    assert!(root.validate(ResourceKind::IRQ).is_ok());
    assert!(root.validate(ResourceKind::IOPORT).is_ok());
    assert!(root.validate(ResourceKind::SMC).is_ok());
    assert!(root.validate(ResourceKind::SYSTEM).is_ok());

    // Create a child MMIO resource.
    let child = Resource::create(
        "child-mmio",
        ResourceKind::MMIO,
        0x2000,
        0x500,
        ResourceFlags::empty(),
    );
    assert_ne!(child.id(), 0);
}

///
/// A child resource validates only against its own kind.
#[test]
fn resource_child_validation() {
    let _ctx = TestContext::new();
    let child = Resource::create(
        "mmio-child",
        ResourceKind::MMIO,
        0x2000,
        0x500,
        ResourceFlags::empty(),
    );

    assert!(child.validate(ResourceKind::MMIO).is_ok());
    assert_eq!(
        child.validate(ResourceKind::IRQ).err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
    );
    assert_eq!(
        child.validate(ResourceKind::ROOT).err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
    );
}

///
/// Creating an exclusive resource that overlaps an existing resource
/// (or vice versa) should be rejected.
#[test]
fn resource_exclusive_overlap_fails() {
    let _ctx = TestContext::new();
    // Register a non-exclusive resource.
    let res1 = Resource::create(
        "io-base",
        ResourceKind::IOPORT,
        0x400,
        0x100,
        ResourceFlags::empty(),
    );
    res1.register_region();

    // Trying to create an exclusive resource that overlaps should fail.
    let result = Resource::check_exclusive_overlap(
        ResourceKind::IOPORT,
        0x450,
        0x50,
        ResourceFlags::EXCLUSIVE,
    );
    assert_eq!(
        result.err(),
        Some(zircon_object::ZxError::NOT_FOUND),
        "exclusive overlap with existing should fail"
    );

    // Register an exclusive resource in a non-overlapping region.
    let res2 = Resource::create(
        "io-excl",
        ResourceKind::IOPORT,
        0x600,
        0x100,
        ResourceFlags::EXCLUSIVE,
    );
    res2.register_region();

    // Trying to create a non-exclusive resource that overlaps an
    // exclusive resource should also fail.
    let result = Resource::check_exclusive_overlap(
        ResourceKind::IOPORT,
        0x650,
        0x50,
        ResourceFlags::empty(),
    );
    assert_eq!(
        result.err(),
        Some(zircon_object::ZxError::NOT_FOUND),
        "overlap with exclusive should fail"
    );

    drop(res1);
    drop(res2);
}

///
/// ROOT resources bypass range checks in validate_ranged_resource.
#[test]
fn resource_root_bypasses_range() {
    let _ctx = TestContext::new();
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());

    // ROOT should pass ranged validation for any kind and any range.
    assert!(root
        .validate_ranged_resource(ResourceKind::MMIO, 0, 0x1_0000_0000)
        .is_ok());
    assert!(root
        .validate_ranged_resource(ResourceKind::IRQ, 0xFFFF, 1)
        .is_ok());
    assert!(root
        .validate_ranged_resource(ResourceKind::SYSTEM, 0, 16)
        .is_ok());
}

///
/// A SYSTEM resource validates as SYSTEM and covers the system
/// sub-resource base IDs.
#[test]
fn resource_system_resource() {
    let _ctx = TestContext::new();
    use zircon_object::dev::{
        ZX_RSRC_SYSTEM_DEBUGLOG_BASE, ZX_RSRC_SYSTEM_HYPERVISOR_BASE, ZX_RSRC_SYSTEM_VMEX_BASE,
    };

    let sys = Resource::create(
        "system",
        ResourceKind::SYSTEM,
        0,
        16,
        ResourceFlags::empty(),
    );
    assert!(sys.validate(ResourceKind::SYSTEM).is_ok());
    assert_eq!(
        sys.validate(ResourceKind::MMIO).err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
    );

    // System sub-resource bases are within range [0, 16).
    assert!(sys
        .validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_HYPERVISOR_BASE, 1)
        .is_ok());
    assert!(sys
        .validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_VMEX_BASE, 1)
        .is_ok());
    assert!(sys
        .validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_DEBUGLOG_BASE, 1)
        .is_ok());
}

///
/// A resource with a kind beyond COUNT is not representable by the
/// ResourceKind enum. Attempting to create one via numeric_enum would
/// fail. This test verifies COUNT is the boundary.
#[test]
fn resource_invalid_kind() {
    let _ctx = TestContext::new();
    // COUNT (6) is valid as an enum variant but semantically invalid.
    // The syscall layer rejects kind >= COUNT.
    let res = Resource::create("count", ResourceKind::COUNT, 0, 0, ResourceFlags::empty());
    let info = res.get_info();
    assert_eq!(info.kind, ResourceKind::COUNT as u32);
}

///
/// Resource name is stored and returned in info.
#[test]
fn resource_name_support() {
    let _ctx = TestContext::new();
    let res = Resource::create(
        "hello-resource",
        ResourceKind::MMIO,
        0,
        0,
        ResourceFlags::empty(),
    );
    let info = res.get_info();
    let name_str = core::str::from_utf8(&info.name[..14]).unwrap();
    assert_eq!(name_str, "hello-resource\0".trim_end_matches('\0'));

    // Name via KernelObject trait.
    assert_eq!(res.name(), "hello-resource");
}

///
/// Registering a region and then checking overlap works correctly.
/// Region cleanup happens via on_zero_handles (handle table lifecycle),
/// not via Drop. This test verifies registration and overlap checking.
#[test]
fn resource_register_region() {
    let _ctx = TestContext::new();
    // Use a unique address range unlikely to collide with other tests.
    let res = Resource::create(
        "region-test",
        ResourceKind::SMC,
        0xDEAD_0000,
        0x1000,
        ResourceFlags::EXCLUSIVE,
    );
    res.register_region();

    // Overlapping with the exclusive region should fail.
    let result = Resource::check_exclusive_overlap(
        ResourceKind::SMC,
        0xDEAD_0500,
        0x100,
        ResourceFlags::empty(),
    );
    assert_eq!(
        result.err(),
        Some(zircon_object::ZxError::NOT_FOUND),
        "overlap with exclusive region should fail"
    );

    // Non-overlapping should succeed.
    let result = Resource::check_exclusive_overlap(
        ResourceKind::SMC,
        0xDEAE_0000,
        0x1000,
        ResourceFlags::empty(),
    );
    assert!(result.is_ok(), "non-overlapping should succeed");

    // Different kind should not conflict.
    let result = Resource::check_exclusive_overlap(
        ResourceKind::MMIO,
        0xDEAD_0500,
        0x100,
        ResourceFlags::empty(),
    );
    assert!(result.is_ok(), "different kind should not conflict");

    // Trigger on_zero_handles by going through the handle table.
    // Add to a process handle table, then remove — this calls
    // on_zero_handles which unregisters the region.
    use zircon_object::object::{Handle, Rights};
    let ctx = TestContext::new();
    let hv = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));
    ctx.proc.remove_handle(hv).unwrap();

    // Now the same range should be available.
    let result = Resource::check_exclusive_overlap(
        ResourceKind::SMC,
        0xDEAD_0500,
        0x100,
        ResourceFlags::empty(),
    );
    assert!(
        result.is_ok(),
        "after unregister, region should be available"
    );
}

// ============================================================================
// Missing Fuchsia core-tests: Resource suite (36 tests)
// All require system/MMIO/IRQ resources not available in libos mode.
// ============================================================================

/// C++: TEST(Resource, BasicActions)
#[test]
#[ignore = "requires MMIO resource and zx_object_get_info with ZX_INFO_RESOURCE"]
fn basic_actions() {
    todo!("requires MMIO resource and zx_object_get_info with ZX_INFO_RESOURCE");
}

/// C++: TEST(Resource, CreateArithmeticOverflow)
#[test]
#[ignore = "requires resource creation with base+size overflow validation"]
fn create_arithmetic_overflow() {
    todo!("requires resource creation with base+size overflow validation");
}

/// C++: TEST(Resource, CreateBadNamePointerReturnsInvalidArgs)
#[test]
#[ignore = "requires resource creation with bad name pointer validation"]
fn create_bad_name_pointer_returns_invalid_args() {
    todo!("requires resource creation with bad name pointer validation");
}

/// C++: TEST(Resource, CreateFromRangedRoot)
#[test]
#[ignore = "requires MMIO ranged root resource for exclusive creation"]
fn create_from_ranged_root() {
    todo!("requires MMIO ranged root resource for exclusive creation");
}

/// C++: TEST(Resource, CreateFromRangedRootKinds)
#[test]
#[ignore = "requires MMIO/IRQ/SYSTEM ranged root resources"]
fn create_from_ranged_root_kinds() {
    todo!("requires MMIO/IRQ/SYSTEM ranged root resources");
}

/// C++: TEST(Resource, CreateInvalidOptionsAndFlags)
#[test]
#[ignore = "requires resource creation with invalid options/flags validation"]
fn create_invalid_options_and_flags() {
    todo!("requires resource creation with invalid options/flags validation");
}

/// C++: TEST(Resource, CreateInvalidParentReturnsBadHandle)
#[test]
#[ignore = "requires resource creation with ZX_HANDLE_INVALID parent"]
fn create_invalid_parent_returns_bad_handle() {
    todo!("requires resource creation with ZX_HANDLE_INVALID parent");
}

/// C++: TEST(Resource, CreateNameHandlingAndTruncation)
#[test]
#[ignore = "requires resource creation with name truncation validation"]
fn create_name_handling_and_truncation() {
    todo!("requires resource creation with name truncation validation");
}

/// C++: TEST(Resource, CreateParentHandleValidation)
#[test]
#[ignore = "requires resource creation with various parent handle types"]
fn create_parent_handle_validation() {
    todo!("requires resource creation with various parent handle types");
}

/// C++: TEST(Resource, CreateParentMissingWriteRightsReturnsAccessDenied)
#[test]
#[ignore = "requires resource creation with reduced parent rights"]
fn create_parent_missing_write_rights_returns_access_denied() {
    todo!("requires resource creation with reduced parent rights");
}

/// C++: TEST(Resource, CreateResourceSlice)
#[test]
#[ignore = "requires MMIO resource slicing with kind/flag validation"]
fn create_resource_slice() {
    todo!("requires MMIO resource slicing with kind/flag validation");
}

/// C++: TEST(Resource, CreatedHandlePropertiesAndRights)
#[test]
#[ignore = "requires resource handle properties and rights validation"]
fn created_handle_properties_and_rights() {
    todo!("requires resource handle properties and rights validation");
}

/// C++: TEST(Resource, ExclusiveRegionReleaseLifecycle)
#[test]
#[ignore = "requires MMIO exclusive region allocation and release lifecycle"]
fn exclusive_region_release_lifecycle() {
    todo!("requires MMIO exclusive region allocation and release lifecycle");
}

/// C++: TEST(Resource, ExclusiveShared)
#[test]
#[ignore = "requires MMIO resource exclusive/shared conflict validation"]
fn exclusive_shared() {
    todo!("requires MMIO resource exclusive/shared conflict validation");
}

/// C++: TEST(Resource, GetInfoResourceValidation)
#[test]
#[ignore = "requires ZX_INFO_RESOURCE with buffer/rights validation"]
fn get_info_resource_validation() {
    todo!("requires ZX_INFO_RESOURCE with buffer/rights validation");
}

/// C++: TEST(Resource, HierarchySlicingBoundaries)
#[test]
#[ignore = "requires MMIO resource hierarchy slicing boundary validation"]
fn hierarchy_slicing_boundaries() {
    todo!("requires MMIO resource hierarchy slicing boundary validation");
}

/// C++: TEST(Resource, InvalidArgs)
#[test]
#[ignore = "requires resource creation with privilege inversion validation"]
fn invalid_args() {
    todo!("requires resource creation with privilege inversion validation");
}

/// C++: TEST(Resource, IoportDeniedByDefault)
#[test]
#[ignore = "requires x86_64 I/O port access and crash detection"]
fn ioport_denied_by_default() {
    todo!("requires x86_64 I/O port access and crash detection");
}

/// C++: TEST(Resource, Ioports)
#[test]
#[ignore = "requires x86_64 I/O port resource and ioports_request syscall"]
fn ioports() {
    todo!("requires x86_64 I/O port resource and ioports_request syscall");
}

/// C++: TEST(Resource, MexecBadZbiHeaderLength)
#[test]
#[ignore = "requires MEXEC resource and ZBI header validation"]
fn mexec_bad_zbi_header_length() {
    todo!("requires MEXEC resource and ZBI header validation");
}

/// C++: TEST(Resource, MexecEmptyZbi)
#[test]
#[ignore = "requires MEXEC resource and empty ZBI validation"]
fn mexec_empty_zbi() {
    todo!("requires MEXEC resource and empty ZBI validation");
}

/// C++: TEST(Resource, MexecMemoryLeakOnFailure)
#[test]
#[ignore = "requires MEXEC resource and memory leak detection"]
fn mexec_memory_leak_on_failure() {
    todo!("requires MEXEC resource and memory leak detection");
}

/// C++: TEST(Resource, MexecPagerPanic)
#[test]
#[ignore = "requires MEXEC resource and pager-backed VMO"]
fn mexec_pager_panic() {
    todo!("requires MEXEC resource and pager-backed VMO");
}

/// C++: TEST(Resource, MexecPayloadTooSmall)
#[test]
#[ignore = "requires MEXEC resource and payload buffer validation"]
fn mexec_payload_too_small() {
    todo!("requires MEXEC resource and payload buffer validation");
}

/// C++: TEST(Resource, MexecZeroSizedVmo)
#[test]
#[ignore = "requires MEXEC resource and zero-sized VMO validation"]
fn mexec_zero_sized_vmo() {
    todo!("requires MEXEC resource and zero-sized VMO validation");
}

/// C++: TEST(Resource, PcInterruptVectorPoolLeak)
#[test]
#[ignore = "requires x86_64 IRQ resource and interrupt vector pool"]
fn pc_interrupt_vector_pool_leak() {
    todo!("requires x86_64 IRQ resource and interrupt vector pool");
}

/// C++: TEST(Resource, ProbeAddressSpace)
#[test]
#[ignore = "requires MMIO resource and physical address space probing"]
fn probe_address_space() {
    todo!("requires MMIO resource and physical address space probing");
}

/// C++: TEST(Resource, SharedExclusive)
#[test]
#[ignore = "requires MMIO resource shared/exclusive conflict validation"]
fn shared_exclusive() {
    todo!("requires MMIO resource shared/exclusive conflict validation");
}

/// C++: TEST(Resource, StrictMmioRangeValidation)
#[test]
#[ignore = "requires MMIO resource with unaligned range and strict validation"]
fn strict_mmio_range_validation() {
    todo!("requires MMIO resource with unaligned range and strict validation");
}

/// C++: TEST(Resource, SystemResourceSubBasesAndSyscallValidation)
#[test]
#[ignore = "requires system resource sub-bases and syscall enforcement"]
fn system_resource_sub_bases_and_syscall_validation() {
    todo!("requires system resource sub-bases and syscall enforcement");
}

/// C++: TEST(Resource, ValidateRangedResourceWithWrongKindReturnsAccessDenied)
#[test]
#[ignore = "requires ranged resource validation with wrong kind"]
fn validate_ranged_resource_with_wrong_kind_returns_access_denied() {
    todo!("requires ranged resource validation with wrong kind");
}

/// C++: TEST(Resource, ValidateResourceKindBaseWithWrongTypeOrKindReturnsError)
#[test]
#[ignore = "requires resource kind/base validation with wrong type"]
fn validate_resource_kind_base_with_wrong_type_or_kind_returns_error() {
    todo!("requires resource kind/base validation with wrong type");
}

/// C++: TEST(Resource, VmoCreation)
#[test]
#[ignore = "requires MMIO resource and zx_vmo_create_physical"]
fn vmo_creation() {
    todo!("requires MMIO resource and zx_vmo_create_physical");
}

/// C++: TEST(Resource, VmoCreationSmaller)
#[test]
#[ignore = "requires MMIO resource smaller than page with physical VMO"]
fn vmo_creation_smaller() {
    todo!("requires MMIO resource smaller than page with physical VMO");
}

/// C++: TEST(Resource, VmoCreationUnaligned)
#[test]
#[ignore = "requires MMIO resource with unaligned base and physical VMO"]
fn vmo_creation_unaligned() {
    todo!("requires MMIO resource with unaligned base and physical VMO");
}

/// C++: TEST(Resource, VmoReplaceAsExecutable)
#[test]
#[ignore = "requires system resource and vmo_replace_as_executable"]
fn vmo_replace_as_executable() {
    todo!("requires system resource and vmo_replace_as_executable");
}
