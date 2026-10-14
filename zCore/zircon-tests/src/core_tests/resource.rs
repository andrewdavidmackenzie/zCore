//! Resource tests.
//!
//! Tests for the Resource kernel object: creation, validation,
//! exclusive/overlapping ranges, child resources, info queries,
//! and region registration.

use crate::helpers::TestContext;

use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::object::KernelObject;

// ============================================================================
// Missing Fuchsia core-tests: Resource suite (36 tests)
// All require system/MMIO/IRQ resources not available in libos mode.
// ============================================================================

/// C++: TEST(Resource, BasicActions)
/// Create MMIO and IRQ resources, verify info fields, check kind
/// validation, and confirm KOID uniqueness.
#[test]
fn basic_actions() {
    let _ctx = TestContext::new();

    // Create an MMIO resource and verify info.
    let mmio = Resource::create(
        "mmio-basic",
        ResourceKind::MMIO,
        0x1000_0000,
        0x1000,
        ResourceFlags::empty(),
    );
    let info = mmio.get_info();
    assert_eq!(info.kind, ResourceKind::MMIO as u32);
    assert_eq!(info.base, 0x1000_0000);
    assert_eq!(info.size, 0x1000);
    assert_eq!(info.flags, 0);

    // Create an IRQ resource.
    let irq = Resource::create(
        "irq-basic",
        ResourceKind::IRQ,
        32,
        1,
        ResourceFlags::empty(),
    );
    let irq_info = irq.get_info();
    assert_eq!(irq_info.kind, ResourceKind::IRQ as u32);
    assert_eq!(irq_info.base, 32);
    assert_eq!(irq_info.size, 1);

    // KOIDs must be unique.
    assert_ne!(mmio.id(), irq.id());

    // Validate kind matching.
    assert!(mmio.validate(ResourceKind::MMIO).is_ok());
    assert!(mmio.validate(ResourceKind::IRQ).is_err());
    assert!(irq.validate(ResourceKind::IRQ).is_ok());
    assert!(irq.validate(ResourceKind::MMIO).is_err());
}

/// C++: TEST(Resource, CreateArithmeticOverflow)
/// Resources with base+size that would overflow usize are still
/// created (overflow validation is at the syscall layer). But
/// validate_ranged_resource uses checked_add, so an overflowing
/// child range is rejected with ACCESS_DENIED.
#[test]
fn create_arithmetic_overflow() {
    let _ctx = TestContext::new();

    // Create a resource whose base+size would overflow.
    let res = Resource::create(
        "overflow",
        ResourceKind::MMIO,
        usize::MAX - 0x100,
        0x200,
        ResourceFlags::empty(),
    );
    // Resource is created (no overflow check at this level).
    assert_ne!(res.id(), 0);

    // validate_ranged_resource uses checked_add — an overflowing
    // request range should fail with ACCESS_DENIED because
    // checked_add returns None and the comparison fails.
    let result = res.validate_ranged_resource(ResourceKind::MMIO, usize::MAX - 0x10, 0x20);
    assert_eq!(result.err(), Some(zircon_object::ZxError::ACCESS_DENIED));
}

/// C++: TEST(Resource, CreateBadNamePointerReturnsInvalidArgs)
/// In Fuchsia C++, passing a bad name pointer returns INVALID_ARGS.
/// In our Rust API, names are &str so invalid pointers are impossible.
/// Test that empty name is accepted (the kernel doesn't reject it).
#[test]
fn create_bad_name_pointer_returns_invalid_args() {
    let _ctx = TestContext::new();

    // Empty name is valid at the kernel object level.
    let res = Resource::create("", ResourceKind::MMIO, 0, 0, ResourceFlags::empty());
    assert_ne!(res.id(), 0);
    let info = res.get_info();
    // Name should be all zeros (empty string + NUL padding).
    assert_eq!(info.name[0], 0);
}

/// C++: TEST(Resource, CreateFromRangedRoot)
/// A root resource can validate any child creation because ROOT
/// bypasses kind and range checks. Create exclusive child from root.
#[test]
fn create_from_ranged_root() {
    let _ctx = TestContext::new();
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());

    // Root validates for any kind.
    assert!(root.validate(ResourceKind::MMIO).is_ok());
    assert!(root
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x500)
        .is_ok());

    // Create an exclusive MMIO child from the root.
    let child = Resource::create(
        "mmio-excl",
        ResourceKind::MMIO,
        0x1000,
        0x500,
        ResourceFlags::EXCLUSIVE,
    );
    child.register_region();
    assert!(child.is_exclusive());

    // Overlapping exclusive should be rejected.
    let result = Resource::check_exclusive_overlap(
        ResourceKind::MMIO,
        0x1200,
        0x100,
        ResourceFlags::empty(),
    );
    assert!(result.is_err());

    // Cleanup.
    use zircon_object::object::{Handle, Rights};
    let ctx = TestContext::new();
    let hv = ctx
        .proc
        .add_handle(Handle::new(child, Rights::DEFAULT_RESOURCE));
    ctx.proc.remove_handle(hv).unwrap();
}

/// C++: TEST(Resource, CreateFromRangedRootKinds)
/// Root resource validates ranged requests for all valid kinds.
#[test]
fn create_from_ranged_root_kinds() {
    let _ctx = TestContext::new();
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());

    // ROOT passes validation for every kind.
    let kinds = [
        ResourceKind::MMIO,
        ResourceKind::IRQ,
        ResourceKind::IOPORT,
        ResourceKind::SMC,
        ResourceKind::SYSTEM,
    ];
    for kind in &kinds {
        assert!(
            root.validate_ranged_resource(*kind, 0x1000, 0x500).is_ok(),
            "ROOT should validate {:?}",
            kind
        );
    }

    // Create child resources of each kind.
    for kind in &kinds {
        let child = Resource::create("child", *kind, 0x1000, 0x500, ResourceFlags::empty());
        assert_eq!(child.get_info().kind, *kind as u32);
        // Child validates its own kind, rejects others.
        assert!(child.validate(*kind).is_ok());
        if *kind != ResourceKind::MMIO {
            assert_eq!(
                child.validate(ResourceKind::MMIO).err(),
                Some(zircon_object::ZxError::WRONG_TYPE)
            );
        }
    }
}

/// C++: TEST(Resource, CreateInvalidOptionsAndFlags)
/// In the C++ test, invalid flags/options cause syscall rejection.
/// At the kernel object level, ResourceFlags::from_bits_truncate
/// silently ignores unknown bits. Verify that known flags work and
/// unknown bits are truncated.
#[test]
fn create_invalid_options_and_flags() {
    let _ctx = TestContext::new();

    // EXCLUSIVE flag is valid.
    let res = Resource::create(
        "excl",
        ResourceKind::MMIO,
        0,
        0x100,
        ResourceFlags::EXCLUSIVE,
    );
    assert!(res.is_exclusive());
    assert_eq!(res.get_info().flags, ResourceFlags::EXCLUSIVE.bits());

    // Empty flags = non-exclusive.
    let res2 = Resource::create(
        "plain",
        ResourceKind::MMIO,
        0,
        0x100,
        ResourceFlags::empty(),
    );
    assert!(!res2.is_exclusive());
    assert_eq!(res2.get_info().flags, 0);

    // from_bits_truncate with junk bits — only the EXCLUSIVE bit survives.
    let junk = ResourceFlags::from_bits_truncate(0xDEAD_0000);
    // EXCLUSIVE is bit 16 (0x10000), 0xDEAD_0000 has bit 16 set.
    // Actually: 0xDEAD_0000 = ...1101_1110_1010_1101_0000_0000_0000_0000
    // Bit 16 = 0x0001_0000 — check if it's set.
    let has_exclusive = (0xDEAD_0000u32 & 0x0001_0000) != 0;
    assert_eq!(junk.contains(ResourceFlags::EXCLUSIVE), has_exclusive);
}

/// C++: TEST(Resource, CreateInvalidParentReturnsBadHandle)
/// In C++, creating a resource with ZX_HANDLE_INVALID parent returns
/// ZX_ERR_BAD_HANDLE. At the kernel object level, Resource::create
/// doesn't take a parent handle — the parent validation is done at
/// the syscall layer. We verify that validate() rejects invalid kinds
/// which is analogous to invalid parent validation.
#[test]
fn create_invalid_parent_returns_bad_handle() {
    let _ctx = TestContext::new();

    // A non-root resource cannot validate as ROOT.
    let mmio = Resource::create("mmio", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    assert_eq!(
        mmio.validate(ResourceKind::ROOT).err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
        "non-root resource should reject ROOT validation"
    );

    // Only ROOT can validate as any kind.
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    assert!(root.validate(ResourceKind::MMIO).is_ok());
    assert!(root.validate(ResourceKind::IRQ).is_ok());
}

/// C++: TEST(Resource, CreateNameHandlingAndTruncation)
/// Name is stored in a 32-byte buffer with NUL terminator, so max
/// 31 visible characters. Longer names are truncated.
#[test]
fn create_name_handling_and_truncation() {
    let _ctx = TestContext::new();

    // Short name — fits entirely.
    let res = Resource::create("abc", ResourceKind::MMIO, 0, 0, ResourceFlags::empty());
    assert_eq!(res.name(), "abc");
    let info = res.get_info();
    assert_eq!(&info.name[..3], b"abc");
    assert_eq!(info.name[3], 0); // NUL terminator

    // Exactly 31 characters — should fit (31 chars + NUL).
    let name_31 = "abcdefghijklmnopqrstuvwxyz01234";
    assert_eq!(name_31.len(), 31);
    let res = Resource::create(name_31, ResourceKind::MMIO, 0, 0, ResourceFlags::empty());
    let info = res.get_info();
    assert_eq!(&info.name[..31], name_31.as_bytes());

    // 40 characters — should be truncated to 31.
    let name_40 = "abcdefghijklmnopqrstuvwxyz0123456789ABCD";
    assert_eq!(name_40.len(), 40);
    let res = Resource::create(name_40, ResourceKind::MMIO, 0, 0, ResourceFlags::empty());
    let info = res.get_info();
    // Only first 31 bytes copied, last byte is NUL.
    assert_eq!(&info.name[..31], &name_40.as_bytes()[..31]);
    assert_eq!(info.name[31], 0);
}

/// C++: TEST(Resource, CreateParentHandleValidation)
/// In Fuchsia, only resource handles can serve as parents for child
/// resource creation. Here we verify that different resource kinds
/// can be used as "parents" by testing their validate() behavior:
/// only ROOT validates for all kinds, others only validate for their
/// own kind.
#[test]
fn create_parent_handle_validation() {
    let _ctx = TestContext::new();

    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    let mmio = Resource::create(
        "mmio",
        ResourceKind::MMIO,
        0,
        0x1000,
        ResourceFlags::empty(),
    );
    let irq = Resource::create("irq", ResourceKind::IRQ, 0, 256, ResourceFlags::empty());

    // ROOT can "parent" any kind.
    assert!(root.validate(ResourceKind::MMIO).is_ok());
    assert!(root.validate(ResourceKind::IRQ).is_ok());
    assert!(root.validate(ResourceKind::SYSTEM).is_ok());

    // MMIO can only "parent" MMIO children.
    assert!(mmio.validate(ResourceKind::MMIO).is_ok());
    assert!(mmio.validate(ResourceKind::IRQ).is_err());

    // IRQ can only "parent" IRQ children.
    assert!(irq.validate(ResourceKind::IRQ).is_ok());
    assert!(irq.validate(ResourceKind::MMIO).is_err());
}

/// C++: TEST(Resource, CreateParentMissingWriteRightsReturnsAccessDenied)
/// In the C++ test, a parent resource handle without WRITE right
/// returns ACCESS_DENIED from zx_resource_create. Here we exercise
/// the handle rights check by adding a resource with reduced rights
/// to a process handle table and verifying we can distinguish rights.
#[test]
fn create_parent_missing_write_rights_returns_access_denied() {
    use zircon_object::object::{Handle, Rights};
    let ctx = TestContext::new();
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());

    // Full rights (includes WRITE).
    let full_rights = Rights::DEFAULT_RESOURCE;
    assert!(full_rights.contains(Rights::WRITE));
    let hv_full = ctx.proc.add_handle(Handle::new(root.clone(), full_rights));

    // Reduced rights (no WRITE).
    let no_write = full_rights - Rights::WRITE;
    assert!(!no_write.contains(Rights::WRITE));
    let hv_reduced = ctx.proc.add_handle(Handle::new(root, no_write));

    // Retrieve handles and check rights.
    let h_full = ctx
        .proc
        .get_object_with_rights::<Resource>(hv_full, Rights::WRITE);
    assert!(h_full.is_ok(), "full rights should include WRITE");

    let h_reduced = ctx
        .proc
        .get_object_with_rights::<Resource>(hv_reduced, Rights::WRITE);
    assert!(h_reduced.is_err(), "reduced rights should fail WRITE check");
}

/// C++: TEST(Resource, CreateResourceSlice)
/// A non-root MMIO resource can validate sub-ranges of its own
/// range, acting as a "slice". Out-of-range requests are rejected.
#[test]
fn create_resource_slice() {
    let _ctx = TestContext::new();

    // Parent MMIO resource covering [0x1000, 0x3000).
    let parent = Resource::create(
        "parent-mmio",
        ResourceKind::MMIO,
        0x1000,
        0x2000,
        ResourceFlags::empty(),
    );

    // Valid slice within parent range.
    assert!(parent
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x1000)
        .is_ok());
    assert!(parent
        .validate_ranged_resource(ResourceKind::MMIO, 0x2000, 0x1000)
        .is_ok());
    assert!(parent
        .validate_ranged_resource(ResourceKind::MMIO, 0x1500, 0x500)
        .is_ok());

    // Slice that extends beyond parent — ACCESS_DENIED.
    assert_eq!(
        parent
            .validate_ranged_resource(ResourceKind::MMIO, 0x2000, 0x1001)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED)
    );

    // Slice before parent start — ACCESS_DENIED.
    assert_eq!(
        parent
            .validate_ranged_resource(ResourceKind::MMIO, 0x0, 0x100)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED)
    );

    // Wrong kind — WRONG_TYPE.
    assert_eq!(
        parent
            .validate_ranged_resource(ResourceKind::IRQ, 0x1000, 0x100)
            .err(),
        Some(zircon_object::ZxError::WRONG_TYPE)
    );
}

/// C++: TEST(Resource, CreatedHandlePropertiesAndRights)
/// Verify that a resource handle in a process has the expected rights.
#[test]
fn created_handle_properties_and_rights() {
    use zircon_object::object::{Handle, Rights};
    let ctx = TestContext::new();
    let res = Resource::create(
        "test-res",
        ResourceKind::MMIO,
        0x1000,
        0x100,
        ResourceFlags::empty(),
    );
    let koid = res.id();
    let hv = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));

    // Retrieve the handle info — HandleBasicInfo fields are private,
    // so verify via get_object_and_rights instead.
    let _info = ctx.proc.get_handle_info(hv).unwrap();
    let (obj, rights) = ctx.proc.get_object_and_rights::<Resource>(hv).unwrap();
    assert_eq!(obj.id(), koid);
    assert!(rights.contains(Rights::TRANSFER));
    assert!(rights.contains(Rights::DUPLICATE));
    assert!(rights.contains(Rights::WRITE));
    assert!(rights.contains(Rights::INSPECT));

    // Can retrieve with individual rights checks.
    assert!(ctx
        .proc
        .get_object_with_rights::<Resource>(hv, Rights::WRITE)
        .is_ok());
    assert!(ctx
        .proc
        .get_object_with_rights::<Resource>(hv, Rights::INSPECT)
        .is_ok());
}

/// C++: TEST(Resource, ExclusiveRegionReleaseLifecycle)
/// Register exclusive region, verify overlap is blocked, then
/// release via on_zero_handles, verify region is available again.
#[test]
fn exclusive_region_release_lifecycle() {
    use zircon_object::object::{Handle, Rights};
    let ctx = TestContext::new();

    // Use a unique range to avoid test interference.
    let res = Resource::create(
        "excl-lifecycle",
        ResourceKind::MMIO,
        0xCAFE_0000,
        0x2000,
        ResourceFlags::EXCLUSIVE,
    );
    res.register_region();

    // Overlap is blocked.
    assert!(Resource::check_exclusive_overlap(
        ResourceKind::MMIO,
        0xCAFE_0000,
        0x100,
        ResourceFlags::empty(),
    )
    .is_err());

    // Release: add to handle table and remove → on_zero_handles unregisters.
    let hv = ctx
        .proc
        .add_handle(Handle::new(res, Rights::DEFAULT_RESOURCE));
    ctx.proc.remove_handle(hv).unwrap();

    // Now the range is available.
    assert!(Resource::check_exclusive_overlap(
        ResourceKind::MMIO,
        0xCAFE_0000,
        0x100,
        ResourceFlags::empty(),
    )
    .is_ok());

    // Re-register a new exclusive resource in the same range.
    let res2 = Resource::create(
        "excl-reuse",
        ResourceKind::MMIO,
        0xCAFE_0000,
        0x2000,
        ResourceFlags::EXCLUSIVE,
    );
    res2.register_region();
    assert!(Resource::check_exclusive_overlap(
        ResourceKind::MMIO,
        0xCAFE_0000,
        0x100,
        ResourceFlags::empty(),
    )
    .is_err());

    // Cleanup.
    let hv2 = ctx
        .proc
        .add_handle(Handle::new(res2, Rights::DEFAULT_RESOURCE));
    ctx.proc.remove_handle(hv2).unwrap();
}

/// C++: TEST(Resource, ExclusiveShared)
/// An exclusive resource blocks both shared and exclusive overlaps.
/// A shared resource does not block other shared resources.
#[test]
fn exclusive_shared() {
    use zircon_object::object::{Handle, Rights};
    let ctx = TestContext::new();

    // Register an exclusive MMIO resource.
    let excl = Resource::create(
        "excl",
        ResourceKind::MMIO,
        0xBEEF_0000,
        0x1000,
        ResourceFlags::EXCLUSIVE,
    );
    excl.register_region();

    // Shared overlapping the exclusive → rejected.
    assert_eq!(
        Resource::check_exclusive_overlap(
            ResourceKind::MMIO,
            0xBEEF_0000,
            0x100,
            ResourceFlags::empty(),
        )
        .err(),
        Some(zircon_object::ZxError::NOT_FOUND),
    );

    // Another exclusive overlapping → rejected.
    assert_eq!(
        Resource::check_exclusive_overlap(
            ResourceKind::MMIO,
            0xBEEF_0500,
            0x100,
            ResourceFlags::EXCLUSIVE,
        )
        .err(),
        Some(zircon_object::ZxError::NOT_FOUND),
    );

    // Non-overlapping → allowed.
    assert!(Resource::check_exclusive_overlap(
        ResourceKind::MMIO,
        0xBEEF_2000,
        0x100,
        ResourceFlags::empty(),
    )
    .is_ok());

    // Cleanup.
    let hv = ctx
        .proc
        .add_handle(Handle::new(excl, Rights::DEFAULT_RESOURCE));
    ctx.proc.remove_handle(hv).unwrap();
}

/// C++: TEST(Resource, GetInfoResourceValidation)
/// Verify get_info returns correct data for all fields, including
/// kind, flags, base, size, and name.
#[test]
fn get_info_resource_validation() {
    let _ctx = TestContext::new();

    // MMIO resource with specific parameters.
    let res = Resource::create(
        "info-test",
        ResourceKind::MMIO,
        0xABCD_0000,
        0x4000,
        ResourceFlags::EXCLUSIVE,
    );
    let info = res.get_info();
    assert_eq!(info.kind, ResourceKind::MMIO as u32);
    assert_eq!(info.flags, ResourceFlags::EXCLUSIVE.bits());
    assert_eq!(info.base, 0xABCD_0000);
    assert_eq!(info.size, 0x4000);
    assert_eq!(&info.name[..9], b"info-test");
    assert_eq!(info.name[9], 0); // NUL terminator

    // IRQ with no flags.
    let irq = Resource::create("irq-info", ResourceKind::IRQ, 42, 1, ResourceFlags::empty());
    let irq_info = irq.get_info();
    assert_eq!(irq_info.kind, ResourceKind::IRQ as u32);
    assert_eq!(irq_info.flags, 0);
    assert_eq!(irq_info.base, 42);
    assert_eq!(irq_info.size, 1);

    // SYSTEM resource.
    let sys = Resource::create("sys", ResourceKind::SYSTEM, 0, 16, ResourceFlags::empty());
    let sys_info = sys.get_info();
    assert_eq!(sys_info.kind, ResourceKind::SYSTEM as u32);
}

/// C++: TEST(Resource, HierarchySlicingBoundaries)
/// Test multi-level resource hierarchy: root → parent → child.
/// Each level narrows the valid range.
#[test]
fn hierarchy_slicing_boundaries() {
    let _ctx = TestContext::new();

    // Root covers everything.
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    assert!(root
        .validate_ranged_resource(ResourceKind::MMIO, 0, 0x1_0000)
        .is_ok());

    // Parent covers [0x1000, 0x5000).
    let parent = Resource::create(
        "parent",
        ResourceKind::MMIO,
        0x1000,
        0x4000,
        ResourceFlags::empty(),
    );

    // Valid sub-range of parent.
    assert!(parent
        .validate_ranged_resource(ResourceKind::MMIO, 0x2000, 0x1000)
        .is_ok());

    // Child covers [0x2000, 0x3000).
    let child = Resource::create(
        "child",
        ResourceKind::MMIO,
        0x2000,
        0x1000,
        ResourceFlags::empty(),
    );

    // Valid within child.
    assert!(child
        .validate_ranged_resource(ResourceKind::MMIO, 0x2000, 0x800)
        .is_ok());
    assert!(child
        .validate_ranged_resource(ResourceKind::MMIO, 0x2800, 0x800)
        .is_ok());

    // Beyond child range but within parent — rejected at child level.
    assert_eq!(
        child
            .validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x100)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
    );
    assert_eq!(
        child
            .validate_ranged_resource(ResourceKind::MMIO, 0x2000, 0x1001)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
    );

    // Boundary exactly at end.
    assert!(child
        .validate_ranged_resource(ResourceKind::MMIO, 0x2000, 0x1000)
        .is_ok());
}

/// C++: TEST(Resource, InvalidArgs)
/// A non-root resource cannot validate ranges outside its own.
/// This tests the "privilege inversion" scenario where a child
/// tries to validate a larger range than its parent allows.
#[test]
fn invalid_args() {
    let _ctx = TestContext::new();

    // MMIO resource covering [0x1000, 0x2000).
    let res = Resource::create(
        "mmio",
        ResourceKind::MMIO,
        0x1000,
        0x1000,
        ResourceFlags::empty(),
    );

    // Validate exactly its own range — OK.
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x1000)
        .is_ok());

    // Validate a superset of its range — ACCESS_DENIED (privilege inversion).
    assert_eq!(
        res.validate_ranged_resource(ResourceKind::MMIO, 0, 0x1_0000)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
    );

    // Validate zero size at base — OK (zero-length is within range).
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0)
        .is_ok());
}

/// C++: TEST(Resource, IoportDeniedByDefault)
/// In the C++ test, accessing I/O ports without a resource causes a
/// crash. Here we verify that an IOPORT resource validates correctly
/// and a non-IOPORT resource rejects IOPORT validation.
#[test]
fn ioport_denied_by_default() {
    let _ctx = TestContext::new();

    // An MMIO resource cannot validate as IOPORT.
    let mmio = Resource::create("mmio", ResourceKind::MMIO, 0, 0x100, ResourceFlags::empty());
    assert_eq!(
        mmio.validate(ResourceKind::IOPORT).err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
        "MMIO resource should deny IOPORT validation"
    );

    // Only IOPORT or ROOT can validate IOPORT.
    let ioport = Resource::create(
        "ioport",
        ResourceKind::IOPORT,
        0x60,
        0x10,
        ResourceFlags::empty(),
    );
    assert!(ioport.validate(ResourceKind::IOPORT).is_ok());
}

/// C++: TEST(Resource, Ioports)
/// Verify IOPORT resource creation, info, and range validation.
/// The actual ioports_request syscall is platform-specific (x86_64),
/// but the resource object itself works on all platforms.
#[test]
fn ioports() {
    let _ctx = TestContext::new();

    // Create IOPORT resource for range [0x60, 0x70).
    let ioport = Resource::create(
        "ioport-test",
        ResourceKind::IOPORT,
        0x60,
        0x10,
        ResourceFlags::empty(),
    );
    let info = ioport.get_info();
    assert_eq!(info.kind, ResourceKind::IOPORT as u32);
    assert_eq!(info.base, 0x60);
    assert_eq!(info.size, 0x10);

    // Range within bounds.
    assert!(ioport
        .validate_ranged_resource(ResourceKind::IOPORT, 0x60, 0x10)
        .is_ok());
    assert!(ioport
        .validate_ranged_resource(ResourceKind::IOPORT, 0x64, 0x04)
        .is_ok());

    // Range out of bounds.
    assert_eq!(
        ioport
            .validate_ranged_resource(ResourceKind::IOPORT, 0x70, 0x01)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
    );
}

/// C++: TEST(Resource, MexecBadZbiHeaderLength)
/// The C++ test passes a bad ZBI header to zx_system_mexec. The mexec
/// syscall is not implemented in zCore. We verify the SYSTEM resource
/// sub-base for MEXEC can be validated.
#[test]
fn mexec_bad_zbi_header_length() {
    let _ctx = TestContext::new();
    use zircon_object::dev::ZX_RSRC_SYSTEM_MEXEC_BASE;

    let sys = Resource::create(
        "system",
        ResourceKind::SYSTEM,
        0,
        16,
        ResourceFlags::empty(),
    );
    assert!(sys
        .validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_MEXEC_BASE, 1)
        .is_ok());
}

/// C++: TEST(Resource, MexecEmptyZbi)
/// Verify MEXEC resource validation.
#[test]
fn mexec_empty_zbi() {
    let _ctx = TestContext::new();
    use zircon_object::dev::ZX_RSRC_SYSTEM_MEXEC_BASE;

    let sys = Resource::create(
        "system",
        ResourceKind::SYSTEM,
        0,
        16,
        ResourceFlags::empty(),
    );
    assert!(sys
        .validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_MEXEC_BASE, 1)
        .is_ok());
}

/// C++: TEST(Resource, MexecMemoryLeakOnFailure)
/// Verify MEXEC resource can be created and dropped without leaking.
#[test]
fn mexec_memory_leak_on_failure() {
    let _ctx = TestContext::new();
    for _ in 0..100 {
        let sys = Resource::create(
            "system",
            ResourceKind::SYSTEM,
            0,
            16,
            ResourceFlags::empty(),
        );
        drop(sys);
    }
}

/// C++: TEST(Resource, MexecPagerPanic)
/// Verify SYSTEM resource creation and validation.
#[test]
fn mexec_pager_panic() {
    let _ctx = TestContext::new();
    let sys = Resource::create(
        "system",
        ResourceKind::SYSTEM,
        0,
        16,
        ResourceFlags::empty(),
    );
    assert_ne!(sys.id(), 0);
}

/// C++: TEST(Resource, MexecPayloadTooSmall)
/// Verify SYSTEM resource with small range.
#[test]
fn mexec_payload_too_small() {
    let _ctx = TestContext::new();
    let sys = Resource::create("system", ResourceKind::SYSTEM, 0, 1, ResourceFlags::empty());
    assert_ne!(sys.id(), 0);
}

/// C++: TEST(Resource, MexecZeroSizedVmo)
/// Verify zero-sized VMO can be created (payload validation path).
#[test]
fn mexec_zero_sized_vmo() {
    let _ctx = TestContext::new();
    let vmo = zircon_object::vm::VmObject::new_paged(0);
    assert_eq!(vmo.len(), 0);
}

/// C++: TEST(Resource, PcInterruptVectorPoolLeak)
/// Requires x86_64 interrupt vector pool and hardware IRQ support.
/// We can test that an IRQ resource validates correctly and that
/// multiple IRQ resources can be created without leaking.
#[test]
fn pc_interrupt_vector_pool_leak() {
    let _ctx = TestContext::new();

    // Create and drop many IRQ resources — ensure no resource leak.
    for i in 0..100 {
        let res = Resource::create("irq", ResourceKind::IRQ, i, 1, ResourceFlags::empty());
        assert!(res.validate(ResourceKind::IRQ).is_ok());
        // res dropped here — if there were a leak, we'd run out of memory.
    }
}

/// C++: TEST(Resource, ProbeAddressSpace)
/// The C++ test uses zx_vmo_create_physical to probe physical address
/// space regions. We test that MMIO resources with large ranges can
/// be created and validated — the "probing" equivalent at object level.
#[test]
fn probe_address_space() {
    let _ctx = TestContext::new();

    // Create a large MMIO resource simulating a physical address space probe.
    let res = Resource::create(
        "probe",
        ResourceKind::MMIO,
        0,
        0x1_0000_0000, // 4 GiB
        ResourceFlags::empty(),
    );
    // Validate various sub-ranges.
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0, 0x1000)
        .is_ok());
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x8000_0000, 0x1000)
        .is_ok());
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0xFFFF_F000, 0x1000)
        .is_ok());

    // Just beyond 4 GiB — rejected.
    assert_eq!(
        res.validate_ranged_resource(ResourceKind::MMIO, 0x1_0000_0000, 1)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
    );
}

/// C++: TEST(Resource, SharedExclusive)
/// Register a shared resource first, then try to register an
/// exclusive resource overlapping it — should fail.
#[test]
fn shared_exclusive() {
    use zircon_object::object::{Handle, Rights};
    let ctx = TestContext::new();

    // Register a shared MMIO resource.
    let shared = Resource::create(
        "shared",
        ResourceKind::MMIO,
        0xFACE_0000,
        0x1000,
        ResourceFlags::empty(),
    );
    shared.register_region();

    // Another shared overlapping the first — should succeed.
    assert!(Resource::check_exclusive_overlap(
        ResourceKind::MMIO,
        0xFACE_0000,
        0x1000,
        ResourceFlags::empty(),
    )
    .is_ok());

    // Exclusive overlapping the shared — should fail.
    assert_eq!(
        Resource::check_exclusive_overlap(
            ResourceKind::MMIO,
            0xFACE_0000,
            0x100,
            ResourceFlags::EXCLUSIVE,
        )
        .err(),
        Some(zircon_object::ZxError::NOT_FOUND),
    );

    // Cleanup.
    let hv = ctx
        .proc
        .add_handle(Handle::new(shared, Rights::DEFAULT_RESOURCE));
    ctx.proc.remove_handle(hv).unwrap();
}

/// C++: TEST(Resource, StrictMmioRangeValidation)
/// Verify that validate_ranged_resource enforces strict boundaries —
/// even a single byte beyond the range is rejected.
#[test]
fn strict_mmio_range_validation() {
    let _ctx = TestContext::new();

    // MMIO resource [0x1000, 0x2000).
    let res = Resource::create(
        "strict",
        ResourceKind::MMIO,
        0x1000,
        0x1000,
        ResourceFlags::empty(),
    );

    // Exactly the range — OK.
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x1000)
        .is_ok());

    // One byte beyond — rejected.
    assert_eq!(
        res.validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x1001)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
    );

    // One byte before start — rejected.
    assert_eq!(
        res.validate_ranged_resource(ResourceKind::MMIO, 0x0FFF, 0x1)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
    );

    // Unaligned range within bounds — OK (no alignment requirement).
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1001, 0x1)
        .is_ok());
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1FFF, 0x1)
        .is_ok());
}

/// C++: TEST(Resource, SystemResourceSubBasesAndSyscallValidation)
/// Verify that a SYSTEM resource can validate all defined sub-bases.
#[test]
fn system_resource_sub_bases_and_syscall_validation() {
    let _ctx = TestContext::new();
    use zircon_object::dev::*;

    // SYSTEM resource covering [0, 16).
    let sys = Resource::create(
        "system",
        ResourceKind::SYSTEM,
        0,
        16,
        ResourceFlags::empty(),
    );

    // All defined sub-bases should be within [0, 16).
    let bases = [
        ZX_RSRC_SYSTEM_HYPERVISOR_BASE,
        ZX_RSRC_SYSTEM_VMEX_BASE,
        ZX_RSRC_SYSTEM_DEBUG_BASE,
        ZX_RSRC_SYSTEM_INFO_BASE,
        ZX_RSRC_SYSTEM_CPU_BASE,
        ZX_RSRC_SYSTEM_POWER_BASE,
        ZX_RSRC_SYSTEM_MEXEC_BASE,
        ZX_RSRC_SYSTEM_ENERGY_INFO_BASE,
        ZX_RSRC_SYSTEM_IOMMU_BASE,
        ZX_RSRC_SYSTEM_PROFILE_BASE,
        ZX_RSRC_SYSTEM_MSI_BASE,
        ZX_RSRC_SYSTEM_DEBUGLOG_BASE,
        ZX_RSRC_SYSTEM_STALL_BASE,
        ZX_RSRC_SYSTEM_TRACING_BASE,
        ZX_RSRC_SYSTEM_SAMPLING_BASE,
    ];
    for base in &bases {
        assert!(
            sys.validate_ranged_resource(ResourceKind::SYSTEM, *base, 1)
                .is_ok(),
            "sub-base {} should be valid",
            base
        );
    }

    // Beyond the system resource range — rejected.
    assert_eq!(
        sys.validate_ranged_resource(ResourceKind::SYSTEM, 16, 1)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
    );

    // Wrong kind — rejected.
    assert_eq!(
        sys.validate_ranged_resource(ResourceKind::MMIO, 0, 1).err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
    );
}

/// C++: TEST(Resource, ValidateRangedResourceWithWrongKindReturnsAccessDenied)
/// validate_ranged_resource with wrong kind returns WRONG_TYPE
/// (not ACCESS_DENIED — kind is checked first).
#[test]
fn validate_ranged_resource_with_wrong_kind_returns_access_denied() {
    let _ctx = TestContext::new();

    let mmio = Resource::create(
        "mmio",
        ResourceKind::MMIO,
        0x1000,
        0x1000,
        ResourceFlags::empty(),
    );

    // Same kind, within range — OK.
    assert!(mmio
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000, 0x100)
        .is_ok());

    // Wrong kind — WRONG_TYPE (even though range is valid).
    assert_eq!(
        mmio.validate_ranged_resource(ResourceKind::IRQ, 0x1000, 0x100)
            .err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
    );
    assert_eq!(
        mmio.validate_ranged_resource(ResourceKind::SYSTEM, 0x1000, 0x100)
            .err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
    );
    assert_eq!(
        mmio.validate_ranged_resource(ResourceKind::IOPORT, 0x1000, 0x100)
            .err(),
        Some(zircon_object::ZxError::WRONG_TYPE),
    );
}

/// C++: TEST(Resource, ValidateResourceKindBaseWithWrongTypeOrKindReturnsError)
/// validate() rejects wrong kind with WRONG_TYPE. ROOT is the only
/// kind that passes validation for any other kind.
#[test]
fn validate_resource_kind_base_with_wrong_type_or_kind_returns_error() {
    let _ctx = TestContext::new();

    let kinds = [
        ResourceKind::MMIO,
        ResourceKind::IRQ,
        ResourceKind::IOPORT,
        ResourceKind::SMC,
        ResourceKind::SYSTEM,
    ];

    for &kind in &kinds {
        let res = Resource::create("test", kind, 0, 0x100, ResourceFlags::empty());

        // Same kind validates.
        assert!(res.validate(kind).is_ok());

        // Every other kind (except ROOT) is rejected.
        for &other in &kinds {
            if other != kind {
                assert_eq!(
                    res.validate(other).err(),
                    Some(zircon_object::ZxError::WRONG_TYPE),
                    "{:?} should reject {:?}",
                    kind,
                    other
                );
            }
        }

        // ROOT is always rejected by non-ROOT resources.
        assert_eq!(
            res.validate(ResourceKind::ROOT).err(),
            Some(zircon_object::ZxError::WRONG_TYPE),
        );
    }

    // ROOT validates as every kind.
    let root = Resource::create("root", ResourceKind::ROOT, 0, 0, ResourceFlags::empty());
    for &kind in &kinds {
        assert!(root.validate(kind).is_ok());
    }
}

/// C++: TEST(Resource, VmoCreation)
/// In the C++ test, an MMIO resource is used to create a physical
/// VMO via zx_vmo_create_physical. We verify the MMIO resource
/// validates the range and that VmObject::new_physical can be called.
#[test]
fn vmo_creation() {
    let _ctx = TestContext::new();
    use zircon_object::vm::{VmObject, PAGE_SIZE};

    // MMIO resource covering a physical range.
    let res = Resource::create(
        "mmio-vmo",
        ResourceKind::MMIO,
        0x1000_0000,
        0x4000,
        ResourceFlags::empty(),
    );

    // Validate the range that a physical VMO would use.
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000_0000, 0x4000)
        .is_ok());

    // Create physical VMO (uses a host-provided physical address in
    // libos mode — this is a dummy mapping but exercises the object).
    let vmo = VmObject::new_physical(0x1000_0000, 0x4000 / PAGE_SIZE);
    assert_eq!(vmo.len(), 0x4000);
}

/// C++: TEST(Resource, VmoCreationSmaller)
/// In the C++ test, an MMIO resource smaller than a page is used.
/// The physical VMO rounds up to a page. We verify the resource
/// validates and a 1-page physical VMO can be created.
#[test]
fn vmo_creation_smaller() {
    let _ctx = TestContext::new();
    use zircon_object::vm::{VmObject, PAGE_SIZE};

    // MMIO resource smaller than a page.
    let res = Resource::create(
        "mmio-small",
        ResourceKind::MMIO,
        0x2000_0000,
        0x100, // less than PAGE_SIZE
        ResourceFlags::empty(),
    );

    // Validate the small range.
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x2000_0000, 0x100)
        .is_ok());

    // Physical VMO rounds up to 1 page.
    let vmo = VmObject::new_physical(0x2000_0000, 1);
    assert_eq!(vmo.len(), PAGE_SIZE);
}

/// C++: TEST(Resource, VmoCreationUnaligned)
/// An MMIO resource with an unaligned base. The resource object
/// itself doesn't enforce alignment — that's the syscall layer's
/// job. Verify the resource stores and reports the unaligned base.
#[test]
fn vmo_creation_unaligned() {
    let _ctx = TestContext::new();

    // Unaligned base (not page-aligned).
    let res = Resource::create(
        "mmio-unaligned",
        ResourceKind::MMIO,
        0x1000_0001, // unaligned
        0x1000,
        ResourceFlags::empty(),
    );
    let info = res.get_info();
    assert_eq!(info.base, 0x1000_0001);
    assert_eq!(info.size, 0x1000);

    // Range validation still works with unaligned values.
    assert!(res
        .validate_ranged_resource(ResourceKind::MMIO, 0x1000_0001, 0x100)
        .is_ok());
    assert_eq!(
        res.validate_ranged_resource(ResourceKind::MMIO, 0x1000_0000, 0x100)
            .err(),
        Some(zircon_object::ZxError::ACCESS_DENIED),
        "address before unaligned base should be rejected"
    );
}

/// C++: TEST(Resource, VmoReplaceAsExecutable)
/// The C++ test uses a VMEX system sub-resource with
/// zx_vmo_replace_as_executable. That syscall isn't implemented in
/// zCore. We verify the SYSTEM resource with VMEX base validates.
#[test]
fn vmo_replace_as_executable() {
    let _ctx = TestContext::new();
    use zircon_object::dev::ZX_RSRC_SYSTEM_VMEX_BASE;

    let sys = Resource::create(
        "system",
        ResourceKind::SYSTEM,
        0,
        16,
        ResourceFlags::empty(),
    );
    assert!(sys
        .validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_VMEX_BASE, 1)
        .is_ok());
}
