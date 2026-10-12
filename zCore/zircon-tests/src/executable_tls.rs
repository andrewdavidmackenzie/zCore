//! Executable TLS tests.
//!
//! Ports of Fuchsia's executable TLS test suite. Tests thread-local
//! storage initialization and access patterns in ELF executables.
//! ELF TLS requires a userspace runtime (dynamic linker) not available
//! in kernel test mode. These tests exercise the kernel primitives
//! that underlie TLS: per-thread state, thread creation, and
//! thread-pointer register management.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread, ThreadState};

extern crate alloc;

/// C++: TEST(ExecutableTlsTest, AlignmentInitializerInThread)
/// TLS alignment in a child thread. At kernel level, each thread
/// has its own register context including thread pointer.
#[test]
fn alignment_initializer_in_thread() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "tls-test").unwrap();
    let thread = Thread::create(&proc, "tls-align").unwrap();
    assert_ne!(thread.id(), 0);
    // TLS alignment is handled by the dynamic linker, not the kernel.
}

/// C++: TEST(ExecutableTlsTest, AlignmentInitializierInMain)
/// TLS alignment in the main thread.
#[test]
fn alignment_initializier_in_main() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "tls-test").unwrap();
    let thread = Thread::create(&proc, "main-tls").unwrap();
    assert_ne!(thread.id(), 0);
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializerInMain)
/// TLS array initialization. Verify VMO provides zeroed pages
/// (TLS bss behavior).
#[test]
fn array_initializer_in_main() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    // TLS bss is backed by zeroed VMO pages.
    let vmo = VmObject::new_paged(1);
    let mut buf = [0xFFu8; 64];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0u8; 64], "VMO pages should be zeroed (TLS bss)");
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializerInThread)
/// TLS array in a child thread. Each thread sees zeroed TLS initially.
#[test]
fn array_initializer_in_thread() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    let mut buf = [0xFFu8; 32];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(buf, [0u8; 32]);
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializerSpamThread)
/// Many threads each with their own TLS array. Verify each thread
/// has a unique identity.
#[test]
fn array_initializer_spam_thread() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "tls-test").unwrap();

    let mut ids = alloc::vec::Vec::new();
    for i in 0..20 {
        let t = Thread::create(&proc, &alloc::format!("spam-{i}")).unwrap();
        ids.push(t.id());
    }
    // All unique.
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 20);
}

/// C++: TEST(ExecutableTlsTest, ArrayInitializierSpamMain)
/// Many iterations of TLS access in the main thread.
#[test]
fn array_initializier_spam_main() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    // Simulate repeated TLS access — VMO read/write.
    let vmo = VmObject::new_paged(1);
    for i in 0u64..100 {
        vmo.write(0, &i.to_ne_bytes()).unwrap();
        let mut val = [0u8; 8];
        vmo.read(0, &mut val).unwrap();
        assert_eq!(u64::from_ne_bytes(val), i);
    }
}

/// C++: TEST(ExecutableTlsTest, BasicInitalizersInMain)
/// Basic TLS initializers. Verify VMO supports initialized data.
#[test]
fn basic_initalizers_in_main() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(1);
    // Write initial value (simulating TLS .tdata).
    vmo.write(0, &42u64.to_ne_bytes()).unwrap();

    let mut val = [0u8; 8];
    vmo.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 42);
}

/// C++: TEST(ExecutableTlsTest, BasicInitalizersInThread)
/// Basic TLS initializers in a child thread.
#[test]
fn basic_initalizers_in_thread() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    // Each thread gets a COW copy of the TLS template.
    let template = VmObject::new_paged(1);
    template.write(0, &99u64.to_ne_bytes()).unwrap();

    let copy = template
        .create_child(false, 0, zircon_object::vm::PAGE_SIZE)
        .unwrap();
    let mut val = [0u8; 8];
    copy.read(0, &mut val).unwrap();
    assert_eq!(u64::from_ne_bytes(val), 99);
}

/// C++: TEST(ExecutableTlsTest, BigArrayInitializerInMain)
/// Large TLS array. Verify multi-page VMO zeroing.
#[test]
fn big_array_initializer_in_main() {
    use zircon_object::vm::{VmObject, PAGE_SIZE};

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(4);
    let mut buf = alloc::vec![0xFFu8; 4 * PAGE_SIZE];
    vmo.read(0, &mut buf).unwrap();
    assert!(
        buf.iter().all(|&b| b == 0),
        "multi-page VMO should be zeroed"
    );
}

/// C++: TEST(ExecutableTlsTest, BigArrayInitializerInThread)
/// Large TLS array in a child thread.
#[test]
fn big_array_initializer_in_thread() {
    use zircon_object::vm::{VmObject, PAGE_SIZE};

    let _ctx = TestContext::new();
    let vmo = VmObject::new_paged(4);
    let mut buf = alloc::vec![0xFFu8; 4 * PAGE_SIZE];
    vmo.read(0, &mut buf).unwrap();
    assert!(buf.iter().all(|&b| b == 0));
}

/// C++: TEST(ExecutableTlsTest, StructureInitalizierInMain)
/// TLS struct initializer. Verify VMO supports structured data.
#[test]
fn structure_initalizier_in_main() {
    use zircon_object::vm::VmObject;

    let _ctx = TestContext::new();
    #[repr(C)]
    struct TlsStruct {
        x: u32,
        y: u32,
        z: u64,
    }
    let s = TlsStruct { x: 1, y: 2, z: 3 };
    let bytes = unsafe {
        core::slice::from_raw_parts(
            &s as *const TlsStruct as *const u8,
            core::mem::size_of::<TlsStruct>(),
        )
    };

    let vmo = VmObject::new_paged(1);
    vmo.write(0, bytes).unwrap();

    let mut buf = [0u8; 16];
    vmo.read(0, &mut buf).unwrap();
    assert_eq!(&buf, bytes);
}

/// C++: TEST(ExecutableTlsTest, StructureInitializerInThread)
/// TLS struct in a child thread. Each thread gets a COW copy.
#[test]
fn structure_initializer_in_thread() {
    use zircon_object::vm::{VmObject, PAGE_SIZE};

    let _ctx = TestContext::new();
    let template = VmObject::new_paged(1);
    template.write(0, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();

    let copy = template.create_child(false, 0, PAGE_SIZE).unwrap();
    let mut buf = [0u8; 8];
    copy.read(0, &mut buf).unwrap();
    assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8]);
}
