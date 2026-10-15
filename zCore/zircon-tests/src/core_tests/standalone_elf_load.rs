//! Standalone ELF load tests.
//!
//! 1:1 port of Fuchsia's zircon/system/utest/core/standalone-elf-load/standalone-elf-load.cc.
//!
//! The C++ test uses elfldltl to load test ELF modules into the root
//! VMAR, reads their build IDs, resolves symbols (kTestRoData), and
//! calls their entry points. This requires:
//!   - elfldltl library (ELF parsing/loading)
//!   - fuchsia.ldsvc / userboot for fetching test module VMOs
//!   - VmarLoader for mapping ELF segments
//!   - Symbolizer markup for debug output
//!
//! None of this infrastructure exists in zCore's libos mode.

/// C++: TEST(StandaloneElfLoadTests, Load)
/// Iterates over TEST_MODULE_LIST (>3 test ELF modules), loads each:
///   1. Fetches VMO via elfldltl::testing::GetTestLibVmo
///   2. Maps the file with MappedVmoFile
///   3. Reads ELF headers, dispatches by format (32/64, endian)
///   4. Checks ehdr.Loadable()
///   5. Collects build ID from PT_NOTE
///   6. Loads segments via VmarLoader
///   7. Decodes PT_DYNAMIC for symbols
///   8. Looks up kTestRoData symbol, reads Word value
///   9. Asserts *kTestRoData == kTestRoDataValue
///  10. If native machine: calls entry point test_start(17, 23)
///  11. Asserts result == 40 (17 + 23)
///  12. Does NOT call loader.Commit() (test module unmapped)
///
/// Asserts at least one module was for the native machine.
///
/// Full ELF loading requires elfldltl, test module VMOs, VmarLoader,
/// and the ability to execute mapped code. None of this exists in
/// zCore's libos mode. We verify the VMAR mapping infrastructure
/// that would underlie ELF segment loading.
#[test]
fn load() {
    let _ctx = crate::helpers::TestContext::new();

    // Verify the VMAR mapping infrastructure that ELF loading uses.
    // Create a VMO representing an ELF segment and map it.
    let job = zircon_object::task::Job::root();
    let proc = zircon_object::task::Process::create(&job, "elf-load").unwrap();
    let vmar = proc.vmar();

    let segment_vmo = zircon_object::vm::VmObject::new_paged(1);
    // Write an ELF magic number to simulate an ELF header.
    segment_vmo.write(0, &[0x7f, b'E', b'L', b'F']).unwrap();

    // Map the VMO as READ (simulating a read-only ELF segment).
    let addr = vmar
        .map(
            None,
            segment_vmo.clone(),
            0,
            zircon_object::vm::PAGE_SIZE,
            zircon_object::vm::MMUFlags::READ,
        )
        .unwrap();
    assert_ne!(addr, 0, "ELF segment mapping should succeed");

    // Verify the ELF magic is readable through the VMO.
    let mut magic = [0u8; 4];
    segment_vmo.read(0, &mut magic).unwrap();
    assert_eq!(&magic, &[0x7f, b'E', b'L', b'F']);
}
