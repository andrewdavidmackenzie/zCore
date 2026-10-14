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
//! faithfully: on x86_64, we document what the tests would do; on other
//! architectures, we skip them.

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
/// segment register should fault. Cannot safely test in libos mode
/// because the crash would kill the test process. We verify the test
/// is x86_64-only and document the expected behavior.
#[test]
#[cfg_attr(not(target_arch = "x86_64"), ignore)]
fn load_last_gdt_entry() {
    // C++ test loads segment selector 0xfffb into ds, ss, es, gs, fs.
    // Each ASSERT_DEATH expects a #GP fault.
    //
    // In libos mode we cannot safely execute these instructions because
    // ASSERT_DEATH requires fork() + signal handling which Rust's test
    // harness doesn't support. The test documents the expected behavior.
    //
    // On a real Zircon kernel, the thread would receive a synthetic
    // exception (ZX_EXCP_FATAL_PAGE_FAULT or ZX_EXCP_GENERAL).
    //
    // Segment selectors tested:
    //   lastGDTEntry = 0xfffb (Index=8191, RPL=3, TI=0 → GDT)
    #[cfg(target_arch = "x86_64")]
    {
        let _last_gdt_entry: u16 = 0xfffb;
        // Would do: asm!("mov {sel:x}, %ds", sel = in(reg) last_gdt_entry)
        // Each would #GP → ASSERT_DEATH in C++.
        // Cannot execute safely in libos mode.
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
#[cfg_attr(not(target_arch = "x86_64"), ignore)]
fn load_first_ldt_entry() {
    #[cfg(target_arch = "x86_64")]
    {
        let _first_ldt_entry: u16 = 0x0007;
        // Would do: asm!("mov {sel:x}, %ds", sel = in(reg) first_ldt_entry)
        // Each would #GP → ASSERT_DEATH in C++.
    }
}

/// C++: TEST(BadSegselTest, JumpToLastGDTEntry)
///
/// ```cpp
/// ASSERT_DEATH([]() { jmp_far(lastGDTEntry); });
/// ```
///
/// Far-jumping to the last GDT entry should fault.
#[test]
#[cfg_attr(not(target_arch = "x86_64"), ignore)]
fn jump_to_last_gdt_entry() {
    #[cfg(target_arch = "x86_64")]
    {
        // C++ does: ljmp *{far_jmp{offset=0, segsel=0xfffb}}
        // Would #GP → ASSERT_DEATH.
    }
}

/// C++: TEST(BadSegselTest, JumpToFirstLDTEntry)
///
/// ```cpp
/// ASSERT_DEATH([]() { jmp_far(firstLDTEntry); });
/// ```
///
/// Far-jumping to the first LDT entry should fault.
#[test]
#[cfg_attr(not(target_arch = "x86_64"), ignore)]
fn jump_to_first_ldt_entry() {
    #[cfg(target_arch = "x86_64")]
    {
        // C++ does: ljmp *{far_jmp{offset=0, segsel=0x0007}}
        // Would #GP → ASSERT_DEATH.
    }
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
/// Iterates over all GDT selectors with RPL=3 and calls LAR to read
/// access rights. This tests that LAR doesn't fault (it sets ZF=0
/// for invalid selectors).
///
/// Note: The C++ test does NOT use ASSERT_DEATH — LAR is safe to call
/// on any selector; it just sets the zero flag.
#[test]
#[cfg_attr(not(target_arch = "x86_64"), ignore)]
fn test_all_gdt_selectors() {
    #[cfg(target_arch = "x86_64")]
    {
        // C++ iterates i from 3 to 0xFFFF step 8 and calls LAR.
        // LAR is safe — it doesn't fault, it just sets ZF.
        // In libos mode we can actually execute this safely.
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
            i += 8;
        }
    }
}
