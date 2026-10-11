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

// vmo_signal_child_signal_map: moved to vmo.rs (child_signal_map)

// VmoReference MappingUpdate tests: moved to vmo_reference.rs (using read/write instead of VMAR mapping)

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

// vmo_reference_unsupported_child: moved to vmo_reference.rs (unsupported_child)
// vmo_reference_streams: moved to vmo_reference.rs (streams)

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

// vmo_zero_unaligned_sub_page: moved to vmo_zero.rs (unaligned_sub_page)

// vmo_zero_unaligned_committed: moved to vmo_zero.rs (unaligned_committed)

// vmo_zero_decommit_middle: moved to vmo_zero.rs (decommit_middle)

// ── VmoZero tests that need special infrastructure ──────────────────

// vmo_zero_contiguous: moved to vmo_zero.rs (contiguous test)

// vmo_zero_hidden_no_content: moved to vmo_zero.rs (hidden_no_content)

// ── VmoClone tests that need VMAR/handle rights ─────────────────────

// vmo_clone_commit: moved to vmo_clone.rs (clone_commit_via_mapping)

// vmo_clone_rights: moved to vmo_clone.rs (clone_rights)

// vmo_clone_no_resize_with_mapping: moved to vmo_clone.rs (clone_no_resize_with_mapping)

// ── VmoSlice tests that need BTI/IOMMU ──────────────────────────────

// vmo_slice_contiguous_parent: moved to vmo_slice.rs (contiguous_parent_slice_is_contiguous)
// vmo_slice_pin: moved to vmo_slice.rs (pin_slice)

// vmo_slice_round_up_size_physical: moved to vmo_slice.rs (round_up_size_physical)

// vmo_slice_parent_contiguous_stays_pinned: moved to vmo_slice.rs (parent_contiguous_stays_pinned)

// ── VmoClone2 tests that need populated_bytes (fractional COW attribution) ──

/// C++: TEST_F(VmoClone2TestCase, ResizeOffsetChild)
/// Resize parent to 0 with offset clone — needs populated_bytes checks.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone2_resize_offset_child() {
    todo!("populated_bytes polling — COW fractional attribution")
}

/// C++: TEST_F(VmoClone2TestCase, ResizeDisjointChild)
/// Resize with 3 disjoint single-page clones, all 6 permutations — needs populated_bytes.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone2_resize_disjoint_child() {
    todo!("populated_bytes polling — COW fractional attribution + CallPermutations")
}

/// C++: TEST_F(VmoClone2TestCase, ResizeMultipleProgressive)
/// Resize with progressive writes + two clones — needs populated_bytes.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone2_resize_multiple_progressive() {
    todo!("populated_bytes polling — COW fractional attribution")
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneClose)
/// 6 progressive clones, discard by close — needs populated_bytes.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone2_progressive_clone_close() {
    todo!("populated_bytes polling — COW fractional attribution")
}

/// C++: TEST_F(ProgressiveCloneDiscardTests, ProgressiveCloneTruncate)
/// 6 progressive clones, discard by truncate — needs populated_bytes.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone2_progressive_clone_truncate() {
    todo!("populated_bytes polling — COW fractional attribution")
}

/// C++: TEST_F(VmoCloneResizeTests, ResizeChild)
/// Resize child and check populated_bytes attribution.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone_resize_child() {
    todo!("populated_bytes polling — COW fractional attribution")
}

/// C++: TEST_F(VmoCloneResizeTests, ResizeOriginal)
/// Resize original and check populated_bytes attribution.
#[cfg(not(feature = "libos"))]
#[test]
fn vmo_clone_resize_original() {
    todo!("populated_bytes polling — COW fractional attribution")
}
