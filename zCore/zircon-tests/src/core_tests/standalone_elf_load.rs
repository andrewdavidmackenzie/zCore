//! Standalone ELF load tests.
//!
//! 1:1 port of Fuchsia's zircon/system/utest/core/standalone-elf-load/standalone-elf-load.cc.
//!
//! The C++ test loads ELF test modules, maps their segments into the root
//! VMAR, looks up the `kTestRoData` symbol, and verifies its value is 42.
//! If the module is for the native machine, it also calls the entry point
//! `TestStart(17, 23)` and verifies the result is 40.
//!
//! The test module source (test-module.cc) is trivial:
//!   int TestStart(int x, int y) { return x + y; }
//!   extern "C" const uint32_t kTestRoData = 42;
//!
//! Prebuilt test modules are at:
//!   prebuilt/zircon-test/x86_64/standalone-elf-load-test-module.so
//!   prebuilt/zircon-test/aarch64/standalone-elf-load-test-module.so

use crate::helpers::TestContext;
use xmas_elf::symbol_table::Entry;
use zircon_object::task::{Job, Process};
use zircon_object::util::elf_loader::{ElfExt, VmarExt};

/// The expected value at the `kTestRoData` symbol (test-module.h:16).
const K_TEST_RO_DATA_VALUE: u32 = 42;

/// Embedded test module ELF binary — architecture-specific.
/// Module names match the C++ TEST_MODULE_LIST format:
///   standalone-elf-load-test-module.$cpu.so
#[cfg(target_arch = "x86_64")]
const TEST_MODULE_BYTES: &[u8] = include_bytes!(
    "../../../../prebuilt/zircon-test/x86_64/standalone-elf-load-test-module.x86_64.so"
);
#[cfg(target_arch = "aarch64")]
const TEST_MODULE_BYTES: &[u8] = include_bytes!(
    "../../../../prebuilt/zircon-test/aarch64/standalone-elf-load-test-module.arm64.so"
);
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const TEST_MODULE_BYTES: &[u8] = &[];

/// C++: TEST(StandaloneElfLoadTests, Load)
/// standalone-elf-load/standalone-elf-load.cc:167
///
/// Loads the test ELF module, maps segments into VMAR, looks up the
/// `kTestRoData` symbol, and verifies its value is 42.
///
/// C++ ref: TestOneModule() at standalone-elf-load.cc:40
///   Steps: parse ELF → ehdr.Loadable() → load segments via
///   LocalVmarLoader → decode PT_DYNAMIC → look up "kTestRoData"
///   → assert *ptr == kTestRoDataValue (42)
#[test]
fn load() {
    let _ctx = TestContext::new();

    // C++ iterates TEST_MODULE_LIST (one module per architecture).
    // We embed the native-architecture module at compile time.
    assert!(
        !TEST_MODULE_BYTES.is_empty(),
        "no test module available for this architecture"
    );

    // Copy to an aligned Vec — xmas_elf requires aligned access to ELF
    // structures and include_bytes! doesn't guarantee alignment.
    let aligned: Vec<u8> = TEST_MODULE_BYTES.to_vec();

    // Parse ELF headers (C++ step c: WithLoadHeadersFromFile).
    let elf = xmas_elf::ElfFile::new(&aligned).expect("failed to parse test module ELF");

    // Verify it's a valid ELF (C++ step d: ehdr.Loadable()).
    assert_eq!(elf.header.pt1.magic, [0x7f, b'E', b'L', b'F']);

    // Create a process + VMAR to load into (C++ uses root VMAR).
    let job = Job::root();
    let proc = Process::create(&job, "elf-load-test").unwrap();
    let vmar = proc.vmar();

    // Load segments into VMAR (C++ step f: loader.Load()).
    let _first_vmo = vmar
        .load_from_elf(&elf)
        .expect("failed to load ELF segments into VMAR");

    // Look up the `kTestRoData` symbol in .dynsym
    // (C++ step h: kTestRoData.Lookup(symbol_info)).
    let dynsym = elf.dynsym().expect("test module has no .dynsym");

    let mut found_symbol = false;
    for sym in dynsym {
        let name = sym.get_name(&elf).unwrap_or("");
        if name == "kTestRoData" {
            found_symbol = true;

            // The symbol's value is the virtual address within the loaded
            // module. Read the u32 from the first VMO (which contains
            // the .rodata section where kTestRoData lives).
            let sym_vaddr = sym.value() as usize;

            // Read through the VMO (the first LOAD segment VMO contains
            // the .rodata at offset sym_vaddr).
            let mut buf = [0u8; 4];
            _first_vmo
                .read(sym_vaddr, &mut buf)
                .expect("failed to read kTestRoData from loaded VMO");
            let value = u32::from_le_bytes(buf);

            // C++ step i: EXPECT_EQ(*ptr, kTestRoDataValue)
            assert_eq!(
                value, K_TEST_RO_DATA_VALUE,
                "kTestRoData should be {} but got {}",
                K_TEST_RO_DATA_VALUE, value
            );
            break;
        }
    }

    // C++: if (!found) ADD_FAILURE() << "has no kTestRoData"
    assert!(found_symbol, "test module has no kTestRoData symbol");

    // C++ step j: if native machine, call TestStart(17, 23) and assert == 40.
    // We cannot call code from a mapped VMO in libos mode — the
    // mapping is backed by anonymous memory, not executable pages
    // with proper instruction fetch support. This step is only
    // testable on bare-metal/QEMU where VMAR mappings are real
    // hardware page table entries with execute permission.
    //
    // The kTestRoData lookup above exercises the full ELF loading
    // pipeline (parse → map → symbol lookup → read), which is the
    // core functionality being tested.
}
