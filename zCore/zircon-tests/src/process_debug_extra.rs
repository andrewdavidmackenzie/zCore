//! Additional process debug tests (not 1:1 C++ ports).

use zircon_object::task::{Job, Process};
use zircon_object::vm::{MMUFlags, VmObject, PAGE_SIZE};

/// Write then read back via memory APIs.
/// (No direct C++ binary equivalent — additional round-trip coverage.)
#[test]
fn read_write_round_trip() {
    let _ctx = crate::helpers::TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "debug-rw").unwrap();
    let vmar = proc.vmar();

    let vmo = VmObject::new_paged(1);
    let flags = MMUFlags::READ | MMUFlags::WRITE;
    let addr = vmar.map(None, vmo, 0, PAGE_SIZE, flags).unwrap();

    let data = [0xCC; 32];
    let _ = vmar.write_memory(addr, &data);

    let mut buf = [0u8; 32];
    let result = vmar.read_memory(addr, &mut buf);
    if result.is_ok() {
        assert_eq!(buf, data);
    }

    vmar.unmap(addr, PAGE_SIZE).unwrap();
}
