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
#[test]
#[ignore = "requires elfldltl, VMAR loader, test module VMOs — not available in zCore libos"]
fn load() {
    // Full C++ test requires the elfldltl ELF loading infrastructure,
    // test module binaries, and the ability to map and execute ELF
    // segments in the process address space. None of this exists in
    // zCore's libos mode.
}
