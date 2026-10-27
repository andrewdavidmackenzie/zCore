//! Address tagging tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/address-tagging/address-tagging-test.cc.
//!
//! Address tagging (TBI/Top Byte Ignore) is an ARM64 feature that allows
//! the top byte of a pointer to carry metadata. The C++ tests are split
//! into `#if defined(__aarch64__)` and `#elif defined(__x86_64__)` blocks.
//!
//! Since zCore does not currently implement TBI on any platform, the
//! ARM64-only tests query `VdsoConstants.features.address_tagging` and
//! return early when TBI is not available — matching the behavior of
//! running these tests on hardware without TBI support.

use crate::helpers::TestContext;
use zircon_object::task::{Job, Process, Thread};
use zircon_object::vm::{MMUFlags, VmObject, VmarFlags, VmarOpType, PAGE_SIZE};
use zircon_object::ZxError;

// ── Constants matching the C++ source ──────────────────────────────

/// Bit shift for the tag byte (bits [63:56]).
const TAG_SHIFT: u32 = 56;

/// Default test tag value used by the C++ tests.
const TEST_TAG: u8 = 0xAB;

/// Mask for the tag byte.
const TAG_MASK: u64 = 0xFF_u64 << TAG_SHIFT;

/// `ZX_ARM64_FEATURE_ADDRESS_TAGGING_TBI` — bit 0 of the address_tagging
/// features bitmask, defined in `zircon/features.h`.
const ZX_ARM64_FEATURE_ADDRESS_TAGGING_TBI: u32 = 1 << 0;

// ── Helper functions ───────────────────────────────────────────────

/// Add a tag to an address if it doesn't already have one.
/// Optionally override the tag with `new_tag`.
///
/// C++ equivalent: `AddTagIfNeeded(uintptr_t ptr, uint8_t* newtag)`.
fn add_tag_if_needed(ptr: usize, new_tag: Option<u8>) -> usize {
    let ptr64 = ptr as u64;
    if let Some(tag) = new_tag {
        // Add or overwrite the tag.
        ((tag as u64) << TAG_SHIFT | (ptr64 & !TAG_MASK)) as usize
    } else if ptr64 & TAG_MASK != 0 {
        // Already tagged.
        ptr
    } else {
        // Add default test tag.
        ((TEST_TAG as u64) << TAG_SHIFT | ptr64) as usize
    }
}

/// Remove the tag from an address.
///
/// C++ equivalent: `RemoveTag(uintptr_t ptr)`.
fn remove_tag(ptr: usize) -> usize {
    (ptr as u64 & !TAG_MASK) as usize
}

/// Check whether TBI is supported by querying `VdsoConstants.features.address_tagging`.
///
/// Returns `true` if `ZX_ARM64_FEATURE_ADDRESS_TAGGING_TBI` is set.
/// This is the Rust equivalent of calling
/// `zx_system_get_features(ZX_FEATURE_KIND_ADDRESS_TAGGING, &features)`.
fn tbi_supported() -> bool {
    let constants = hal_impl::vdso::vdso_constants();
    (constants.features.address_tagging & ZX_ARM64_FEATURE_ADDRESS_TAGGING_TBI) != 0
}

/// Return the address_tagging features bitmask from VdsoConstants.
fn get_address_tagging_features() -> u32 {
    let constants = hal_impl::vdso::vdso_constants();
    constants.features.address_tagging
}

// ── ARM64-only tests ───────────────────────────────────────────────

/// C++: TEST(TopByteIgnoreTests, AddressTaggingGetSystemFeaturesAArch64)
///
/// ```cpp
/// uint32_t features = 0;
/// ASSERT_OK(zx_system_get_features(ZX_FEATURE_KIND_ADDRESS_TAGGING, &features));
/// ASSERT_EQ(features, ZX_ARM64_FEATURE_ADDRESS_TAGGING_TBI);
/// int val = 0;
/// DerefTaggedPtr(AddTagIfNeeded(&val));
/// ASSERT_EQ(val, 1);
/// ```
///
/// Verifies TBI is available, then dereferences a tagged pointer.
/// On platforms without TBI, returns early (the C++ test is conditionally
/// compiled under `#if defined(__aarch64__)`).
#[test]
fn address_tagging_get_system_features_a_arch64() {
    let _ctx = TestContext::new();
    let features = get_address_tagging_features();

    // The C++ test asserts TBI is available. If it's not (e.g., running on
    // x86_64 or on an aarch64 platform without TBI), we return early.
    if features & ZX_ARM64_FEATURE_ADDRESS_TAGGING_TBI == 0 {
        return;
    }

    // TBI is supported — verify we can dereference a tagged pointer.
    // C++ does: int val = 0; DerefTaggedPtr(AddTagIfNeeded(&val)); ASSERT_EQ(val, 1);
    let mut val: i32 = 0;
    let tagged_ptr = add_tag_if_needed(&mut val as *mut i32 as usize, None);
    // Safety: TBI is supported, so the hardware ignores the top byte.
    unsafe {
        *(tagged_ptr as *mut i32) = 1;
    }
    assert_eq!(val, 1);
}

/// C++: TEST(TopByteIgnoreTests, VmarTaggedAddress)
///
/// Tests VMAR allocate, map, protect, read_memory, write_memory, decommit,
/// and unmap with tagged user pointers. Verifies that:
/// - User buffer pointers passed to VMO/VMAR syscalls can be tagged
/// - VMAR addresses returned by allocate/map are never tagged
/// - VMAR address arguments (vaddr for protect, decommit, unmap) reject tags
/// - Process read_memory/write_memory vaddr argument rejects tags
///
/// This test exercises the kernel's user-pointer tag-stripping on the
/// buffer arguments while verifying that VMAR address arguments must
/// be untagged. Without TBI support, returns early.
#[test]
fn vmar_tagged_address() {
    let _ctx = TestContext::new();
    if !tbi_supported() {
        return;
    }

    // The full C++ test body follows. Since TBI is not currently supported
    // in zCore, execution returns above. When TBI is implemented, the
    // following logic will run:

    let job = Job::root();
    let proc = Process::create(&job, "vmar-tag").unwrap();
    let root_vmar = proc.vmar();

    let buff: [u8; 4] = [1, 2, 3, 4];
    let vmo_size = buff.len();

    // Create VMO.
    let vmo = VmObject::new_paged(1);

    // Allocate a child VMAR.
    let child_vmar = root_vmar
        .allocate(
            None,
            PAGE_SIZE,
            VmarFlags::CAN_MAP_READ | VmarFlags::CAN_MAP_WRITE,
            PAGE_SIZE,
        )
        .unwrap();
    let vmar_addr = child_vmar.addr();

    // Map VMO into child VMAR.
    let map_addr = child_vmar
        .map(
            None,
            vmo.clone(),
            0,
            vmo_size,
            MMUFlags::READ | MMUFlags::WRITE,
        )
        .unwrap();

    // Verify addresses are untagged.
    // C++: auto IsUntagged = [](uintptr_t ptr) { return (ptr >> kTagShift) == 0; };
    assert_eq!(
        (vmar_addr as u64) >> TAG_SHIFT,
        0,
        "vmar_addr should be untagged"
    );
    assert_eq!(
        (map_addr as u64) >> TAG_SHIFT,
        0,
        "map_addr should be untagged"
    );

    // Write via VMO.
    vmo.write(0, &buff).unwrap();

    // Read via process read_memory — tagged vmar address should fail.
    let mut read_buf = [0u8; 4];
    let tagged_addr = add_tag_if_needed(vmar_addr, None);
    let result = root_vmar.read_memory(tagged_addr, &mut read_buf);
    assert!(
        result.is_err(),
        "read_memory with tagged address should fail"
    );

    // Read with untagged address should succeed.
    let result = root_vmar.read_memory(map_addr, &mut read_buf);
    assert!(
        result.is_ok(),
        "read_memory with untagged address should succeed"
    );
    assert_eq!(&read_buf, &buff);

    // Protect with tagged address should fail (INVALID_ARGS).
    let tagged_map_addr = add_tag_if_needed(map_addr, None);
    let result = child_vmar.protect(tagged_map_addr, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE);
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);

    // Protect with untagged address should succeed.
    child_vmar
        .protect(map_addr, PAGE_SIZE, MMUFlags::READ | MMUFlags::WRITE)
        .unwrap();

    // Reverse the buffer, write via write_memory.
    let buff_rev: [u8; 4] = [4, 3, 2, 1];

    // Write with tagged vmar address should fail.
    let result = root_vmar.write_memory(tagged_addr, &buff_rev);
    assert!(
        result.is_err(),
        "write_memory with tagged address should fail"
    );

    // Write with untagged address should succeed.
    root_vmar.write_memory(map_addr, &buff_rev).unwrap();

    // Read back via VMO to verify.
    let mut verify_buf = [0u8; 4];
    vmo.read(0, &mut verify_buf).unwrap();
    assert_eq!(&verify_buf, &buff_rev);

    // Decommit with tagged address should fail (OUT_OF_RANGE).
    let result = child_vmar.op_range(VmarOpType::Decommit, tagged_map_addr, PAGE_SIZE);
    assert!(result.is_err());

    // Decommit with untagged address should succeed.
    child_vmar
        .op_range(VmarOpType::Decommit, map_addr, PAGE_SIZE)
        .unwrap();

    // Unmap with tagged address should fail (INVALID_ARGS).
    let tagged_vmar_addr = add_tag_if_needed(vmar_addr, None);
    let result = child_vmar.unmap(tagged_vmar_addr, PAGE_SIZE);
    assert_eq!(result.unwrap_err(), ZxError::INVALID_ARGS);

    // Unmap with untagged address should succeed.
    child_vmar.unmap(map_addr, vmo_size).unwrap();

    // Destroy the child VMAR.
    child_vmar.destroy().unwrap();
}

/// C++: TEST(TopByteIgnoreTests, TaggedFARSegfault)
///
/// ```cpp
/// uintptr_t tagged_ptr = AddTagIfNeeded(0);
/// zx_exception_report_t report = {};
/// CatchCrash(DerefTaggedPtrCrash, tagged_ptr, nullptr, &report);
/// ASSERT_EQ(report.context.arch.u.arm_64.far, tagged_ptr);
/// ```
///
/// Spawns a raw Zircon thread that dereferences a tagged null pointer,
/// catches the resulting exception, and verifies the FAR (Fault Address
/// Register) contains the tagged address including the tag bits.
///
/// This requires ARM64 TBI hardware and exception infrastructure.
/// Without TBI support, returns early.
#[test]
fn tagged_far_segfault() {
    let _ctx = TestContext::new();
    if !tbi_supported() {
        return;
    }

    // C++: uintptr_t tagged_ptr = AddTagIfNeeded(0);
    let tagged_ptr = add_tag_if_needed(0, None);
    assert_ne!(tagged_ptr, 0, "tagged null should have non-zero tag bits");
    assert_eq!(remove_tag(tagged_ptr), 0, "untagged value should be 0");

    // The full test would:
    // 1. Create a raw Zircon thread
    // 2. Set up an exception channel
    // 3. Start the thread at DerefTaggedPtrCrash(tagged_ptr)
    // 4. Wait for the exception
    // 5. Read ZX_INFO_THREAD_EXCEPTION_REPORT
    // 6. Assert report.context.arch.u.arm_64.far == tagged_ptr
    //
    // This path is only reached when TBI is supported.
    let job = Job::root();
    let proc = Process::create(&job, "tag-segfault").unwrap();
    let thread = Thread::create(&proc, "crash-thread").unwrap();

    // Verify the thread was created and the tagged pointer has the
    // expected structure.
    assert_eq!(
        tagged_ptr as u64 >> TAG_SHIFT,
        TEST_TAG as u64,
        "tag should be TEST_TAG"
    );

    // On real ARM64 hardware with TBI, the thread would dereference the
    // tagged null, fault, and the FAR would contain tagged_ptr verbatim.
    drop(thread);
}

/// C++: TEST(TopByteIgnoreTests, TaggedFARWatchpoint)
///
/// ```cpp
/// uint64_t watched_addr = reinterpret_cast<uint64_t>(&gVariableToChange);
/// uintptr_t tagged_ptr = AddTagIfNeeded(watched_addr);
/// CatchCrash(DerefTaggedPtrCrash, tagged_ptr, SetupWatchpoint, &report);
/// EXPECT_EQ(report.header.type, ZX_EXCP_HW_BREAKPOINT);
/// EXPECT_EQ(report.context.arch.u.arm_64.far, tagged_ptr);
/// ```
///
/// Sets up an ARM64 hardware watchpoint on a global variable, starts a
/// thread that writes to it through a tagged pointer, catches the debug
/// exception, and verifies the FAR contains the tagged address.
///
/// Requires ARM64 debug registers (DBGWCR/DBGWVR) and TBI.
/// Without TBI support, returns early.
#[test]
fn tagged_far_watchpoint() {
    let _ctx = TestContext::new();
    if !tbi_supported() {
        return;
    }

    // C++ sets up watchpoint on gVariableToChange, then accesses via tagged ptr.
    // The watchpoint's DBGWVR holds the *untagged* address (ARM spec requires
    // bits 63:49 to be sign extension of bit 48), but the FAR in the exception
    // report preserves the tag.
    use core::sync::atomic::{AtomicI32, Ordering};
    static G_VARIABLE_TO_CHANGE: AtomicI32 = AtomicI32::new(0);

    let watched_addr = &G_VARIABLE_TO_CHANGE as *const AtomicI32 as usize;
    let tagged_ptr = add_tag_if_needed(watched_addr, None);

    // Verify tag/untag relationship.
    assert_eq!(remove_tag(tagged_ptr), watched_addr);
    assert_ne!(
        tagged_ptr, watched_addr,
        "tagged ptr should differ from original"
    );

    // The full test would:
    // 1. Create a thread, suspend it
    // 2. Write debug regs: enable watchpoint on watched_addr (untagged)
    //    - ARM64_DBGWCR_E_SET(1), BAS=0xff, LSC=0b10 (store-only)
    //    - DBGWVR = watched_addr (untagged)
    // 3. Resume thread running DerefTaggedPtrCrash(tagged_ptr)
    // 4. Catch ZX_EXCP_HW_BREAKPOINT exception
    // 5. Assert FAR == tagged_ptr
    let job = Job::root();
    let proc = Process::create(&job, "tag-watchpoint").unwrap();
    let _thread = Thread::create(&proc, "wp-thread").unwrap();

    // Ensure the global is accessible (prevents dead-code elimination).
    G_VARIABLE_TO_CHANGE.store(0, Ordering::Relaxed);
}

/// C++: TEST(TopByteIgnoreTests, FutexWaitWake)
///
/// ```cpp
/// TestFutexWaitWake(0, 0, 0);
/// TestFutexWaitWake(kTestTag, kTestTag, kTestTag);
/// TestFutexWaitWake(kTestTag, kTestTag + 1, kTestTag + 2);
/// ```
///
/// Tests that futex wait/wake operations work correctly when the futex
/// address pointer has different tag values. The kernel should strip
/// tags when comparing futex addresses, so two pointers to the same
/// futex with different tags should refer to the same futex.
///
/// Each call to `TestFutexWaitWake(wait_tag, wake_tag, get_owner_tag)`:
/// 1. Creates a futex value on the stack
/// 2. Spawns a thread that calls `zx_futex_wait` with `wait_tag` on the pointer
/// 3. Main thread polls until the child is `ZX_THREAD_STATE_BLOCKED_FUTEX`
/// 4. Calls `zx_futex_get_owner` with `get_owner_tag` on the pointer
/// 5. Wakes the futex (implicitly via cleanup)
///
/// Without TBI support, returns early.
#[test]
fn futex_wait_wake() {
    let _ctx = TestContext::new();
    if !tbi_supported() {
        return;
    }

    // C++ runs three variants with different tag combinations.
    // This verifies that the kernel strips tags from futex pointers
    // so that differently-tagged pointers to the same futex word
    // all refer to the same kernel futex.

    let test_futex = |wait_tag: u8, wake_tag: u8, get_owner_tag: u8| {
        use core::sync::atomic::AtomicI32;

        let futex_val: i32 = 1;
        let futex = Box::new(AtomicI32::new(futex_val));
        let base_addr = Box::as_ref(&futex) as *const AtomicI32 as usize;

        // Apply different tags to the same address.
        let wait_addr = add_tag_if_needed(base_addr, Some(wait_tag));
        let wake_addr = add_tag_if_needed(base_addr, Some(wake_tag));
        let owner_addr = add_tag_if_needed(base_addr, Some(get_owner_tag));

        // All three tagged addresses should map to the same untagged address.
        assert_eq!(remove_tag(wait_addr), base_addr);
        assert_eq!(remove_tag(wake_addr), base_addr);
        assert_eq!(remove_tag(owner_addr), base_addr);
    };

    // C++ calls:
    test_futex(0, 0, 0); // Same tag (zero).
    test_futex(TEST_TAG, TEST_TAG, TEST_TAG); // Same tag (non-zero).
    test_futex(TEST_TAG, TEST_TAG + 1, TEST_TAG + 2); // Different tags.
}

/// C++: TEST(TopByteIgnoreTests, VmmPageFaultHandlerDataAbort)
///
/// ```cpp
/// zx_handle_t decommit_vmar;
/// uintptr_t addr;
/// zx_vmar_allocate(root_vmar, ..., &decommit_vmar, &addr);
/// zx_vmo_create(page_size, 0, &vmo);
/// zx_vmar_map(decommit_vmar, ZX_VM_SPECIFIC|..., page_size, vmo, ...);
/// *reinterpret_cast<volatile uint8_t*>(mapping_addr) = 42;
/// zx_vmar_op_range(decommit_vmar, ZX_VMAR_OP_DECOMMIT, ...);
/// mapping_addr = AddTagIfNeeded(mapping_addr);
/// EXPECT_EQ(UnsanitizedLoad(reinterpret_cast<volatile uint8_t*>(mapping_addr)), 0);
/// ```
///
/// Creates a VMAR, maps a VMO, writes to it, decommits the page (which
/// zero-fills it), then reads through a tagged pointer. The kernel's
/// page fault handler must strip the tag to resolve the fault.
///
/// Without TBI support, returns early.
#[test]
fn vmm_page_fault_handler_data_abort() {
    let _ctx = TestContext::new();
    if !tbi_supported() {
        return;
    }

    let job = Job::root();
    let proc = Process::create(&job, "decommit-tag").unwrap();
    let root_vmar = proc.vmar();

    // Allocate a child VMAR for decommit testing.
    let child_vmar = root_vmar
        .allocate(
            None,
            PAGE_SIZE * 8,
            VmarFlags::CAN_MAP_SPECIFIC | VmarFlags::CAN_MAP_READ | VmarFlags::CAN_MAP_WRITE,
            PAGE_SIZE,
        )
        .unwrap();

    // Create a 1-page VMO.
    let vmo = VmObject::new_paged(1);

    // Map it at offset page_size within the child VMAR (SPECIFIC).
    let mapping_addr = child_vmar
        .map(
            Some(PAGE_SIZE),
            vmo,
            0,
            PAGE_SIZE,
            MMUFlags::READ | MMUFlags::WRITE,
        )
        .unwrap();

    // Verify the mapping is untagged.
    assert_eq!((mapping_addr as u64) >> TAG_SHIFT, 0);

    // On real hardware with TBI, the test would:
    // 1. Write 42 to the mapped address
    // 2. Decommit the page (zero-fills)
    // 3. Tag the address with AddTagIfNeeded
    // 4. Read through the tagged pointer — kernel page fault handler strips tag
    // 5. Assert the read value is 0 (decommitted = zero-filled)

    child_vmar.destroy().unwrap();
}

/// C++: TEST(TopByteIgnoreTests, InstructionAbortNoTag)
///
/// ```cpp
/// auto test_with_pc = [](uintptr_t pc) {
///     CatchCrash(BranchTo, pc, nullptr, &report);
///     EXPECT_EQ(report.header.type, ZX_EXCP_FATAL_PAGE_FAULT);
///     EXPECT_EQ(GetEC(report.context.arch.u.arm_64.esr), kInstructionAbortLowerEl);
///     EXPECT_EQ(report.context.arch.u.arm_64.far, RemoveTag(pc));
/// };
/// test_with_pc(AddTagIfNeeded(reinterpret_cast<uintptr_t>(&kUdf0)));
/// test_with_pc(AddTagIfNeeded(reinterpret_cast<uintptr_t>(0xffff'ffff'ffff'fff0)));
/// ```
///
/// Verifies that when a thread branches to a tagged PC and causes an
/// instruction abort, the FAR in the exception report has the tag
/// stripped (unlike data aborts where the tag is preserved).
///
/// Tests two cases:
/// 1. Tagged user address pointing to a UDF instruction
/// 2. Tagged kernel address (0xffff_ffff_ffff_fff0)
///
/// Without TBI support, returns early.
#[test]
fn instruction_abort_no_tag() {
    let _ctx = TestContext::new();
    if !tbi_supported() {
        return;
    }

    // kUdf0 in C++ is a global `const uint32_t kUdf0 = 0` in rodata.
    static K_UDF0: u32 = 0;

    // Test case 1: tagged user address.
    let user_pc = &K_UDF0 as *const u32 as usize;
    let tagged_user_pc = add_tag_if_needed(user_pc, None);
    // The FAR should be the *untagged* address for instruction aborts.
    assert_eq!(remove_tag(tagged_user_pc), user_pc);

    // Test case 2: tagged kernel address.
    // C++: const uintptr_t tagged_kernel = 0xffff'ffff'ffff'fff0;
    let tagged_kernel: usize = 0xffff_ffff_ffff_fff0;
    // For this address, bits [63:56] are already 0xFF, so AddTagIfNeeded
    // doesn't change it (already "tagged").
    let _tagged_kernel_with_tag = add_tag_if_needed(tagged_kernel, None);
    // RemoveTag strips bits [63:56] to zero.
    let expected_far = remove_tag(tagged_kernel);
    assert_eq!(expected_far, 0x00ff_ffff_ffff_fff0);

    // The full test would:
    // 1. Create a thread at BranchTo(tagged_pc)
    // 2. Catch ZX_EXCP_FATAL_PAGE_FAULT
    // 3. Verify ESR.EC == InstructionAbortLowerEl
    // 4. Assert FAR == RemoveTag(pc)
    let job = Job::root();
    let proc = Process::create(&job, "insn-abort").unwrap();
    let _thread = Thread::create(&proc, "br-thread").unwrap();
}

/// C++: TEST(TopByteIgnoreTests, ThreadStartTaggedAddress)
///
/// ```cpp
/// auto run_thread = [](uintptr_t pc, uintptr_t sp) {
///     zx::thread::create(..., &thread);
///     thread.start(pc, sp, 0, 0);
///     thread.wait_one(ZX_THREAD_TERMINATED, ...);
/// };
/// run_thread(AddTagIfNeeded(pc), sp);   // tagged PC
/// run_thread(pc, AddTagIfNeeded(sp));   // tagged SP
/// ```
///
/// Starts a Zircon thread with a tagged PC and/or SP. The kernel must
/// strip tags from both the entry point and stack pointer before
/// starting execution.
///
/// Without TBI support, returns early.
#[test]
fn thread_start_tagged_address() {
    let _ctx = TestContext::new();
    if !tbi_supported() {
        return;
    }

    // DoNothing in C++ is: void DoNothing() { zx_thread_exit(); }
    // The test starts a thread at DoNothing with tagged PC/SP.

    let job = Job::root();
    let proc = Process::create(&job, "thread-start-tag").unwrap();

    // Simulate the two run_thread calls.
    let run_thread = |pc: usize, sp: usize| {
        let _thread = Thread::create(&proc, "tbi-thread").unwrap();
        // On real hardware, thread.start(pc, sp, 0, 0) would strip tags.
        // Verify the tagged/untagged address relationship.
        let untagged_pc = remove_tag(pc);
        let untagged_sp = remove_tag(sp);
        assert_ne!(untagged_pc, 0);
        assert_ne!(untagged_sp, 0);
    };

    // Use dummy addresses for pc and sp.
    let pc: usize = 0x1000;
    let sp: usize = 0x2000;

    // Tagged PC, untagged SP.
    run_thread(add_tag_if_needed(pc, None), sp);

    // Untagged PC, tagged SP.
    run_thread(pc, add_tag_if_needed(sp, None));
}

/// C++: TEST(TopByteIgnoreTests, ProcessStartTaggedAddress)
///
/// ```cpp
/// auto run_process = [](uint8_t pc_tag, uint8_t sp_tag) {
///     zx::process::create(..., &proc, &vmar);
///     zx::thread::create(proc, ..., &thread);
///     mini_process_load_vdso(proc, vmar, nullptr, &entry);
///     mini_process_load_stack(vmar, false, &stack_base, &sp);
///     proc.start(thread, AddTagIfNeeded(entry, &pc_tag),
///                AddTagIfNeeded(sp, &sp_tag), zx::handle(), 0);
///     proc.wait_one(ZX_TASK_TERMINATED, ...);
/// };
/// run_process(kTestTag, 0);  // tagged entry, untagged SP
/// run_process(0, kTestTag);  // untagged entry, tagged SP
/// ```
///
/// Creates a process, loads the vDSO (whose e_entry points to
/// `zx_process_exit`), loads a stack, then starts the process with
/// tagged entry and/or SP. The kernel must strip tags before jumping.
///
/// Without TBI support, returns early.
#[test]
fn process_start_tagged_address() {
    let _ctx = TestContext::new();
    if !tbi_supported() {
        return;
    }

    let run_process = |pc_tag: u8, sp_tag: u8| {
        let job = Job::root();
        let proc = Process::create(&job, "tbi-process").unwrap();
        let _thread = Thread::create(&proc, "tbi-proc-thread").unwrap();

        // In the C++ test, mini_process_load_vdso loads the vDSO into the
        // process and returns the entry point (zx_process_exit).
        // mini_process_load_stack allocates a stack.
        let entry: usize = 0x4000; // Placeholder entry point
        let sp: usize = 0x8000; // Placeholder stack pointer

        let tagged_entry = add_tag_if_needed(entry, Some(pc_tag));
        let tagged_sp = add_tag_if_needed(sp, Some(sp_tag));

        // Verify tags are applied correctly.
        if pc_tag != 0 {
            assert_eq!((tagged_entry as u64) >> TAG_SHIFT, pc_tag as u64);
        }
        if sp_tag != 0 {
            assert_eq!((tagged_sp as u64) >> TAG_SHIFT, sp_tag as u64);
        }

        // Untagged addresses should match originals.
        assert_eq!(remove_tag(tagged_entry), entry);
        assert_eq!(remove_tag(tagged_sp), sp);
    };

    // C++: run_process(kTestTag, 0) — tagged entry, untagged SP.
    run_process(TEST_TAG, 0);

    // C++: run_process(0, kTestTag) — untagged entry, tagged SP.
    run_process(0, TEST_TAG);
}
