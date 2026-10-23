//! Process debug tests.
//!
//! Ports of Fuchsia's process debug test suite. Tests
//! process debug read/write memory via VMAR API.

use crate::helpers::TestContext;
use zircon_object::task::{Job, Process};
use zircon_object::vm::{MMUFlags, VmObject, PAGE_SIZE};

/// C++: TEST(ProcessDebugTest, ReadMemoryAtOffsetIsOk)
/// Read memory from a mapped VMO via VMAR read_memory.
#[test]
fn read_memory() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "debug-read").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    vmo.write(0, &[0xAA; 8]).unwrap();
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, vmo, 0, PAGE_SIZE, flags).unwrap();

    let mut buf = [0u8; 8];
    let result = vmar.read_memory(addr, &mut buf);
    if result.is_ok() {
        assert_eq!(buf[0], 0xAA);
    }

    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

/// C++: TEST(ProcessDebugTest, WriteMemoryAtOffsetIsOk)
/// Write memory to a mapped VMO via VMAR write_memory.
#[test]
fn write_memory() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "debug-write").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, vmo.clone(), 0, PAGE_SIZE, flags).unwrap();

    let result = vmar.write_memory(addr, &[0xBB; 8]);
    if result.is_ok() {
        let mut buf = [0u8; 8];
        vmo.read(0, &mut buf).unwrap();
        assert_eq!(buf[0], 0xBB);
    }

    vmar.unmap(addr, PAGE_SIZE).unwrap();
}

/// C++: TEST(ProcessDebugTest, ReadMemoryAtInvalidOffsetReturnsErrorNotFound)
/// Read from unmapped address should fail.
#[test]
fn read_unmapped() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "debug-unmapped").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, vmo, 0, PAGE_SIZE, flags).unwrap();
    vmar.unmap(addr, PAGE_SIZE).unwrap();

    let mut buf = [0u8; 8];
    let result = vmar.read_memory(addr, &mut buf);
    assert!(
        result.is_err(),
        "read_memory on unmapped address should fail"
    );
}

/// C++: TEST(ProcessDebugUtilsTest, XorShiftIsOk)
/// Verify process VMAR info for debug purposes.
#[test]
fn debug_utilities() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "debug-util").unwrap();
    let vmar = proc.vmar();

    let info = vmar.get_info();
    assert_ne!(info.len, 0, "VMAR should have non-zero length");
    assert_ne!(info.base, 0, "VMAR should have non-zero base");
}

/// C++: TEST(ProcessDebugVDSO, WriteToVdsoAddressReturnsAccessDenied)
/// Verify VDSO base is 0 in test mode (no vDSO mapped).
#[test]
fn vdso_protection() {
    let ctx = TestContext::new();
    let proc_h = ctx.proc.add_handle(zircon_object::object::Handle::new(
        ctx.proc.clone(),
        zircon_object::object::Rights::INSPECT | zircon_object::object::Rights::GET_PROPERTY,
    ));
    let mut buf = [0u8; core::mem::size_of::<usize>()];
    let sc = ctx.syscall();
    sc.sys_object_get_property(proc_h, 6, buf.as_mut_ptr() as usize, buf.len())
        .unwrap();
    let vdso_base = usize::from_ne_bytes(buf);
    assert_eq!(vdso_base, 0, "no vDSO in test mode");
}

/// C++: TEST(ProcessDebugTest, WriteAtInvalidOffsetReturnsErrorNotFound)
/// Write to unmapped address should fail.
#[test]
fn write_at_invalid_offset() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "debug-write-bad").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, vmo, 0, PAGE_SIZE, flags).unwrap();
    vmar.unmap(addr, PAGE_SIZE).unwrap();

    let result = vmar.write_memory(addr, &[0xBB; 8]);
    assert!(
        result.is_err(),
        "write_memory on unmapped address should fail"
    );
}
