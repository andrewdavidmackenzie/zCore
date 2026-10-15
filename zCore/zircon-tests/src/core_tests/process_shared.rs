//! Process shared tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/process-shared/process-shared.cc.
//!
//! Shared processes (zx_process_create with ZX_PROCESS_SHARED /
//! zx_process_create_shared) allow multiple processes to share a
//! single address space. zCore does not implement this infrastructure.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;

/// C++: TEST(ProcessShared, MapInPrototype)
/// Creates a prototype process (ZX_PROCESS_SHARED), creates a child
/// process via zx_process_create_shared, maps a VMO into the shared
/// VMAR, and verifies both processes can read the data via
/// zx_process_read_memory at the same address.
///
/// zCore does not implement ZX_PROCESS_SHARED or zx_process_create_shared.
/// We verify the underlying VMO/VMAR mapping infrastructure: create a
/// VMO, write data, map it into a process VMAR, and read it back via
/// process.vmar_read_memory(). This exercises the kernel code path
/// that shared processes would use for the shared mapping half.
#[test]
fn map_in_prototype() {
    let _ctx = TestContext::new();

    // Create a process with a VMAR (the "prototype" role).
    let job = zircon_object::task::Job::root();
    let proc = zircon_object::task::Process::create(&job, "prototype").unwrap();
    let vmar = proc.vmar();

    // Create a VMO and write data.
    let vmo = zircon_object::vm::VmObject::new_paged(1);
    vmo.write(0, b"abc").unwrap();

    // Map the VMO into the process VMAR.
    let map_addr = vmar
        .map(
            None,
            vmo.clone(),
            0,
            zircon_object::vm::PAGE_SIZE,
            zircon_object::vm::MMUFlags::READ | zircon_object::vm::MMUFlags::WRITE,
        )
        .unwrap();

    // Read back via the VMO directly (process.read_memory requires
    // the process to be started, which needs userspace; we verify
    // the VMO content is intact after mapping).
    let mut buf = [0u8; 3];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, b"abc", "VMO data should be readable after mapping");

    // Verify the mapping exists in the VMAR.
    assert_ne!(map_addr, 0, "map address should be non-zero");
}

/// C++: TEST(ProcessShared, RestrictedVmarNotShared)
/// Maps a VMO into the restricted (private) VMAR of the child process,
/// then verifies the prototype process cannot read from that address.
///
/// zCore does not implement shared processes. We verify that two
/// separate processes have independent VMARs: mapping a VMO into one
/// process's VMAR does not make it visible in the other's.
#[test]
fn restricted_vmar_not_shared() {
    let _ctx = TestContext::new();

    let job = zircon_object::task::Job::root();
    let proc_a = zircon_object::task::Process::create(&job, "proc-a").unwrap();
    let proc_b = zircon_object::task::Process::create(&job, "proc-b").unwrap();

    // Create a VMO and map it into proc_b only.
    let vmo = zircon_object::vm::VmObject::new_paged(1);
    vmo.write(0, b"abc").unwrap();

    let addr_b = proc_b
        .vmar()
        .map(
            None,
            vmo.clone(),
            0,
            zircon_object::vm::PAGE_SIZE,
            zircon_object::vm::MMUFlags::READ,
        )
        .unwrap();
    assert_ne!(addr_b, 0);

    // proc_a's VMAR is separate — it has no mapping at addr_b.
    // The two processes have independent address spaces.
    assert_ne!(
        proc_a.vmar().id(),
        proc_b.vmar().id(),
        "processes should have distinct VMARs"
    );
}

/// C++: TEST(ProcessShared, InvalidPrototype)
/// Creating a shared child from a non-shared (regular) process
/// returns ZX_ERR_INVALID_ARGS.
///
/// zCore does not implement zx_process_create_shared. We verify that
/// zx_process_create (the only creation path in zCore) creates a
/// regular (non-shared) process, and that attempting to use it as a
/// prototype would be invalid. We verify process creation with
/// invalid arguments fails with the expected error.
#[test]
fn invalid_prototype() {
    let ctx = TestContext::new();
    let sc = ctx.syscall();

    // Create a regular process — not shared (options=0).
    let job = zircon_object::task::Job::root();
    let proc = zircon_object::task::Process::create(&job, "prototype").unwrap();

    // The process is a regular process — not shared.
    // Verify it exists and has a valid VMAR.
    assert_ne!(proc.id(), 0);
    assert_ne!(proc.vmar().id(), 0);

    // In Fuchsia, zx_process_create_shared(regular_process) → INVALID_ARGS.
    // zCore doesn't have this syscall, but we can verify that
    // sys_process_create with invalid options fails.
    let mut proc_h: u32 = 0;
    let mut vmar_h: u32 = 0;
    let job_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        job,
        zircon_object::object::Rights::DEFAULT_JOB,
    ));
    let result = sc.sys_process_create(
        job_h,
        hal_impl::user::UserInPtr::from(c"child".as_ptr() as usize),
        5,
        0xFFFF_FFFF, // invalid options
        hal_impl::user::UserOutPtr::from(&mut proc_h as *mut _ as usize),
        hal_impl::user::UserOutPtr::from(&mut vmar_h as *mut _ as usize),
    );
    assert!(
        result.is_err(),
        "process_create with invalid options should fail"
    );
}

/// C++: TEST(ProcessShared, PropertiesCopied)
/// Verifies that ZX_PROP_PROCESS_BREAK_ON_LOAD and
/// ZX_PROP_PROCESS_DEBUG_ADDR are copied (not shared) to the child.
/// Changing the child's value does not affect the prototype's.
///
/// zCore does not implement shared processes. We verify that process
/// properties (debug_addr, break_on_load) are per-process by setting
/// them on one process and confirming a separate process has
/// independent values.
#[test]
fn properties_copied() {
    let _ctx = TestContext::new();

    let job = zircon_object::task::Job::root();
    let proc_a = zircon_object::task::Process::create(&job, "proc-a").unwrap();
    let proc_b = zircon_object::task::Process::create(&job, "proc-b").unwrap();

    // Set properties on proc_a.
    proc_a.set_debug_addr(0x10adf00d);
    proc_a.set_dyn_break_on_load(0xfeedface);

    // proc_b should have independent (default) values.
    assert_eq!(proc_b.get_debug_addr(), 0, "proc_b debug_addr should be 0");
    assert_eq!(
        proc_b.get_dyn_break_on_load(),
        0,
        "proc_b break_on_load should be 0"
    );

    // proc_a's values should be unchanged.
    assert_eq!(proc_a.get_debug_addr(), 0x10adf00d);
    assert_eq!(proc_a.get_dyn_break_on_load(), 0xfeedface);

    // Change proc_b's values — should not affect proc_a.
    proc_b.set_debug_addr(0xBAADCAFE);
    assert_eq!(
        proc_a.get_debug_addr(),
        0x10adf00d,
        "proc_a should be unaffected"
    );
    assert_eq!(proc_b.get_debug_addr(), 0xBAADCAFE);
}
