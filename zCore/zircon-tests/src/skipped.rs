//! Tests ported from C++ core-tests that require infrastructure not
//! available in libos mode (VMAR mapping, threads, pager, exceptions).
//!
//! These are gated behind `#[cfg(not(feature = "libos"))]` so they are
//! excluded from host testing but will automatically become active
//! when a bare-metal test runner exists.
//!
//! Tests that correspond to entries in `tests/core-tests-skip.txt`
//! (known to hang/crash in QEMU) are additionally marked with a
//! `should_panic` or comment explaining the expected failure.
//!
//! See also: issue #619 (libos test infrastructure gaps)

// ── Tests that need VMAR mapping (#619 gap 1) ──────────────────────

/// C++: TEST(VmoSignalTestCase, ChildSignalMap)
/// Needs VMAR mapping to keep VMO alive while mapped.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_signal_child_signal_map() {
    // Creates clones and maps them with zx_vmar_map.
    // VMO stays alive while mapped (ZERO_CHILDREN not asserted until unmap).
    todo!("needs VMAR mapping infrastructure")
}

/// C++: TEST(VmoReference, MappingUpdate)
/// Maps reference, writes, decommits, verifies mapping reads zeros.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_reference_mapping_update() {
    todo!("needs VMAR mapping infrastructure")
}

/// C++: TEST(VmoReference, MappingUpdateAfterDroppingRef)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_reference_mapping_update_after_dropping_ref() {
    todo!("needs VMAR mapping infrastructure")
}

/// C++: TEST(VmoReference, MappingUpdateAfterDroppingParent)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_reference_mapping_update_after_dropping_parent() {
    todo!("needs VMAR mapping infrastructure")
}

/// C++: TEST(VmoReference, MappingUpdateNestedChildDestroy)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_reference_mapping_update_nested_child_destroy() {
    todo!("needs VMAR mapping infrastructure")
}

/// C++: TEST(VmoReference, MappingUpdateNestedRootDestroy)
/// Also in core-tests-skip.txt: mapping invalidation doesn't propagate.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_reference_mapping_update_nested_root_destroy() {
    todo!("needs VMAR mapping infrastructure — also in core-tests-skip.txt")
}

/// C++: TEST(ChannelTest, ReadFaultDoesNotLeakHandles)
/// Also in core-tests-skip.txt.
#[cfg(not(feature = "libos"))]
#[test]
fn channel_read_fault_does_not_leak_handles() {
    todo!("needs VMAR bad pointer infrastructure")
}

// ── Tests that need multi-threaded infrastructure (#619 gap 2) ──────

/// C++: TEST(ChannelTest, NoSpuriousReadableSignalWhenRacing)
/// Also in core-tests-skip.txt: needs preemptive scheduling.
#[cfg(not(feature = "libos"))]
#[test]
fn channel_no_spurious_readable_signal_when_racing() {
    todo!("needs multi-threaded test infrastructure")
}

/// C++: TEST(ChannelTest, ConcurrentReadsConsumeUniqueElements)
#[cfg(not(feature = "libos"))]
#[test]
fn channel_concurrent_reads_consume_unique_elements() {
    todo!("needs multi-threaded test infrastructure")
}

/// C++: TEST(ChannelTest, WaitManyIsSignaledOnAnyElementWrite)
#[cfg(not(feature = "libos"))]
#[test]
fn channel_wait_many_is_signaled_on_any_element_write() {
    todo!("needs multi-threaded test infrastructure")
}

// ── Tests that need exception/policy infrastructure ─────────────────

/// C++: TEST(ChannelTest, ChannelFullException)
/// Also in core-tests-skip.txt.
#[cfg(not(feature = "libos"))]
#[test]
fn channel_full_exception() {
    todo!("needs policy exception infrastructure")
}

/// C++: TEST(PortTest, QueuePacketLimitExceededGeneratesPolicyException)
/// Also in core-tests-skip.txt.
#[cfg(not(feature = "libos"))]
#[test]
fn port_queue_packet_limit_exceeded_generates_policy_exception() {
    todo!("needs policy exception infrastructure")
}

/// C++: TEST(PortTest, TooManyObservers)
/// Also in core-tests-skip.txt: kernel panics on OOM.
#[cfg(not(feature = "libos"))]
#[test]
fn port_too_many_observers() {
    todo!("kernel panics on OOM instead of returning ZX_ERR_NO_MEMORY")
}

// ── Tests that need pager infrastructure (#619 gap 3) ───────────────

/// C++: TEST(VmoReference, ChildSnapshotAtLeastOnWrite)
/// Also in core-tests-skip.txt.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_reference_child_snapshot_at_least_on_write() {
    todo!("needs pager infrastructure + true SNAPSHOT_AT_LEAST_ON_WRITE")
}

/// C++: TEST(VmoReference, UnsupportedChild)
/// Pager-backed VMO reference, tests snapshot creation fails.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_reference_unsupported_child() {
    todo!("needs pager infrastructure")
}

/// C++: TEST(VmoReference, Streams)
/// Creates zx::stream objects against references.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_reference_streams() {
    todo!("needs stream syscall support for references")
}

// ── Tests that need populated_bytes polling (known hangs) ───────────
// These are in core-tests-skip.txt because fractional COW attribution
// doesn't converge, causing infinite polling loops.

/// C++: TEST(VmoZeroTestCase, WriteCowParent)
/// Also in core-tests-skip.txt (#577, #603).
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_zero_write_cow_parent() {
    todo!("populated_bytes polling hang — COW fractional attribution (#577, #603)")
}

/// C++: TEST(VmoZeroTestCase, AllocateAfterMergeMultipleChildren)
/// Also in core-tests-skip.txt.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_zero_allocate_after_merge_multiple_children() {
    todo!("populated_bytes polling hang — COW fractional attribution (#577, #603)")
}

/// C++: TEST(VmoZeroTestCase, MergeZeroChildren)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_zero_merge_zero_children() {
    todo!("populated_bytes polling — PollVmoPopulatedBytes")
}

/// C++: TEST(VmoZeroTestCase, AllocateAfterMerge)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_zero_allocate_after_merge() {
    todo!("populated_bytes polling — PollVmoPopulatedBytes")
}

// ── VmoZero tests that need VMAR mapping ────────────────────────────

/// C++: TEST(VmoZeroTestCase, UnalignedSubPage)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_zero_unaligned_sub_page() {
    todo!("needs VMAR mapping for pointer verification")
}

/// C++: TEST(VmoZeroTestCase, UnalignedCommitted)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_zero_unaligned_committed() {
    todo!("needs VMAR mapping for cross-page verification")
}

/// C++: TEST(VmoZeroTestCase, DecommitMiddle)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_zero_decommit_middle() {
    todo!("needs VMAR mapping for memset + verify")
}

// ── VmoZero tests that need special infrastructure ──────────────────

/// C++: TEST(VmoZeroTestCase, Contiguous)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_zero_contiguous() {
    todo!("needs BTI/IOMMU for contiguous VMO")
}

/// C++: TEST(VmoZeroTestcase, ZeroHiddenNoContent)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_zero_hidden_no_content() {
    todo!("needs pager infrastructure")
}

// ── VmoClone tests that need VMAR/handle rights ─────────────────────

/// C++: TEST(VmoCloneTestCase, Commit)
/// Needs VMAR mapping to verify clone data via mapped pointers.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone_commit() {
    todo!("needs VMAR mapping")
}

/// C++: TEST(VmoCloneTestCase, Rights)
/// Needs handle rights checking via get_info(ZX_INFO_HANDLE_BASIC).
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone_rights() {
    todo!("needs handle rights + get_info infrastructure")
}

/// C++: TEST(VmoCloneTestCase, NoResize) — VMAR portion
/// The set_size rejection is tested; the VMAR mapping portion is not.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone_no_resize_with_mapping() {
    todo!("needs VMAR mapping with ZX_VM_REQUIRE_NON_RESIZABLE")
}

// ── VmoSlice tests that need BTI/IOMMU ──────────────────────────────

/// C++: TEST(VmoSliceTestCase, ChildSliceOfContiguousParentIsContiguous)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_slice_contiguous_parent() {
    todo!("needs BTI/IOMMU for contiguous VMO")
}

/// C++: TEST(VmoSliceTestCase, Pin)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_slice_pin() {
    todo!("needs BTI/IOMMU for pin operations")
}

/// C++: TEST(VmoSliceTestCase, RoundUpSizePhysical)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_slice_round_up_size_physical() {
    todo!("needs BTI/IOMMU for physical VMO")
}

/// C++: TEST(VmoSliceTestCase, ParentContiguousVmoStaysPinnedWithoutHandle)
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_slice_parent_contiguous_stays_pinned() {
    todo!("needs BTI/IOMMU for pin/unpin + contiguous VMO")
}
