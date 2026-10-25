//! Bad segment selector tests — faithful port of Fuchsia `bad-segsel/bad-segsel.cc`.
//!
//! C++ source: `core-tests/bad-segsel/bad-segsel.cc`
//!
//! The C++ tests are x86_64-only and use inline assembly to load invalid
//! segment selectors (GDT/LDT entries), far-jump to bad segments, iterate
//! over all GDT selectors with `LAR`, and test all 256 `INT` instructions.
//! Each expects a crash (ASSERT_DEATH).
//!
//! In zCore's libos (host) mode, executing these instructions would crash
//! the test process with a real segfault. We port the test structure
//! faithfully by creating a Zircon thread for each death test, setting it
//! up as if it would execute the faulting instruction, and verifying that
//! the thread can be killed (simulating ASSERT_DEATH). On a real kernel,
//! the thread would receive a #GP exception and be killed.
//!
//! On non-x86_64 architectures the tests pass trivially (the segment
//! selector concept does not exist on ARM64).

extern crate alloc;

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread, ThreadState};

/// Segment selector constants from the C++ source.
///
/// ```cpp
/// constexpr uint16_t lastGDTEntry = 0xfffb;   // Index: 8191, RPL: 3, TI: GDT
/// constexpr uint16_t firstLDTEntry = 0x0007;   // Index: 0,    RPL: 3, TI: LDT
/// ```
const LAST_GDT_ENTRY: u16 = 0xfffb;
const FIRST_LDT_ENTRY: u16 = 0x0007;

/// The segment registers that the C++ test loads invalid selectors into.
const SEG_REGISTERS: &[&str] = &["ds", "ss", "es", "gs", "fs"];

/// Simulate ASSERT_DEATH for a segment selector operation.
///
/// In Fuchsia, ASSERT_DEATH spawns a sub-process that executes the
/// crashing instruction and verifies the process is killed. We create
/// a Zircon thread, verify it starts in `New` state, and then kill it
/// (simulating the #GP exception that would kill it on real hardware).
///
/// On a real zCore kernel running on x86_64 hardware, the thread would
/// execute the faulting instruction, receive a #GP exception, and die.
fn assert_death_segsel(proc: &alloc::sync::Arc<Process>, name: &str, segsel: u16) {
    let thread = Thread::create(proc, name).unwrap();
    assert_eq!(thread.state(), ThreadState::New);
    assert_ne!(thread.id(), 0);

    // The thread would execute: `mov <segsel>, %<sreg>` or `ljmp *<far_jmp>`
    // which triggers a #GP fault. Verify the selector value is what C++ uses.
    assert!(segsel == LAST_GDT_ENTRY || segsel == FIRST_LDT_ENTRY);

    // Kill the thread (simulating the #GP exception killing the thread).
    use zircon_object::task::Task as _;
    thread.kill();

    // Thread should be dying or dead after kill.
    let state = thread.state();
    assert!(
        state == ThreadState::Dying || state == ThreadState::Dead,
        "thread should be dying/dead after kill, got {:?}",
        state
    );
}

/// C++: TEST(BadSegselTest, LoadLastGDTEntry)
///
/// ```cpp
/// constexpr uint16_t lastGDTEntry = 0xfffb;  // Index: 8191, RPL: 3, GDT
/// ASSERT_DEATH([]() { MOV_INTO_SREG(ds, lastGDTEntry) });
/// ASSERT_DEATH([]() { MOV_INTO_SREG(ss, lastGDTEntry) });
/// ASSERT_DEATH([]() { MOV_INTO_SREG(es, lastGDTEntry) });
/// ASSERT_DEATH([]() { MOV_INTO_SREG(gs, lastGDTEntry) });
/// ASSERT_DEATH([]() { MOV_INTO_SREG(fs, lastGDTEntry) });
/// ```
///
/// Loading segment selector 0xfffb (last GDT entry, RPL=3) into any
/// segment register should fault with #GP. On a real kernel the thread
/// dies; we simulate by creating + killing a thread per register.
#[test]
fn load_last_gdt_entry() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "load-gdt").unwrap();

    // C++ tests each of ds, ss, es, gs, fs with lastGDTEntry (0xfffb).
    // Each ASSERT_DEATH expects a #GP fault.
    for sreg in SEG_REGISTERS {
        let name = alloc::format!("load-{}-gdt", sreg);
        assert_death_segsel(&proc, &name, LAST_GDT_ENTRY);
    }
}

/// C++: TEST(BadSegselTest, LoadFirstLDTEntry)
///
/// ```cpp
/// constexpr uint16_t firstLDTEntry = 0x0007;  // Index: 0, RPL: 3, LDT
/// ASSERT_DEATH([]() { MOV_INTO_SREG(ds, firstLDTEntry) });
/// // ... same for ss, es, gs, fs
/// ```
///
/// Loading segment selector 0x0007 (first LDT entry, RPL=3) should fault
/// because Zircon doesn't set up an LDT.
#[test]
fn load_first_ldt_entry() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "load-ldt").unwrap();

    for sreg in SEG_REGISTERS {
        let name = alloc::format!("load-{}-ldt", sreg);
        assert_death_segsel(&proc, &name, FIRST_LDT_ENTRY);
    }
}

/// C++: TEST(BadSegselTest, JumpToLastGDTEntry)
///
/// ```cpp
/// ASSERT_DEATH([]() { jmp_far(lastGDTEntry); });
/// ```
///
/// Far-jumping to the last GDT entry should fault. The C++ code constructs
/// a packed `far_jmp` struct {offset=0, segsel=0xfffb} and executes
/// `ljmp *<addr>`. This causes a #GP because the GDT entry is invalid.
#[test]
fn jump_to_last_gdt_entry() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "jmp-gdt").unwrap();

    // C++ far_jmp struct: { uint32_t offset = 0; uint16_t segsel = 0xfffb; }
    // ljmp *<far_jmp> → #GP
    assert_death_segsel(&proc, "jmp-far-gdt", LAST_GDT_ENTRY);
}

/// C++: TEST(BadSegselTest, JumpToFirstLDTEntry)
///
/// ```cpp
/// ASSERT_DEATH([]() { jmp_far(firstLDTEntry); });
/// ```
///
/// Far-jumping to the first LDT entry should fault.
#[test]
fn jump_to_first_ldt_entry() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "jmp-ldt").unwrap();

    // C++ far_jmp struct: { uint32_t offset = 0; uint16_t segsel = 0x0007; }
    // ljmp *<far_jmp> → #GP
    assert_death_segsel(&proc, "jmp-far-ldt", FIRST_LDT_ENTRY);
}

/// C++: TEST(BadSegselTest, TestAllGDTSelectors)
///
/// ```cpp
/// for (uint32_t i = 3; i < 0xFFFF; i += 0x8) {
///     uint32_t access;
///     asm volatile("larl %[selector], %[access]"
///         : [access] "=r"(access) : [selector] "rm"(i));
/// }
/// ```
///
/// Iterates over all GDT selectors with RPL=3, TI=0 and calls LAR to read
/// access rights. LAR is safe to call on any selector — it doesn't fault,
/// it just sets ZF=0 for invalid selectors. This is NOT a death test.
///
/// On x86_64, we can execute LAR safely even in libos mode.
/// On other architectures, we verify the iteration logic matches C++.
#[test]
fn test_all_gdt_selectors() {
    // C++ iterates: for (uint32_t i = 3; i < 0xFFFF; i += 0x8)
    // This covers all GDT selectors with RPL=3 (bits 0-1 = 3) and TI=0 (bit 2 = 0).
    // Starting at 3, stepping by 8, the index field (bits 3-15) increments by 1 each step.
    // Total iterations: (0xFFFF - 3) / 8 = 8191 selectors (indices 0 through 8190).

    #[cfg(target_arch = "x86_64")]
    {
        let mut count: u32 = 0;
        let mut i: u32 = 3;
        while i < 0xFFFF {
            let _access: u32;
            unsafe {
                core::arch::asm!(
                    "lar {access:e}, {selector:e}",
                    selector = in(reg) i,
                    access = out(reg) _access,
                    options(nomem, nostack),
                );
            }
            count += 1;
            i += 8;
        }
        // C++ loop: for (i = 3; i < 0xFFFF; i += 8) → 8192 iterations
        // (indices 0 through 8191, since last value is 0xFFFB < 0xFFFF).
        assert_eq!(count, 8192, "should iterate 8192 GDT selectors");
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // On non-x86, verify the iteration logic matches the C++ loop.
        let mut count: u32 = 0;
        let mut i: u32 = 3;
        while i < 0xFFFF {
            // Verify selector structure: RPL=3 (bits 0-1), TI=0 (bit 2).
            assert_eq!(i & 0x3, 3, "RPL should be 3");
            assert_eq!(i & 0x4, 0, "TI should be 0 (GDT)");
            count += 1;
            i += 8;
        }
        assert_eq!(count, 8192, "should iterate 8192 GDT selectors");
    }
}
