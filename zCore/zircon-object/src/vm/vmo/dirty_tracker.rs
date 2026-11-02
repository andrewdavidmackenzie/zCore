//! Interval-based dirty state tracker for pager-backed VMOs.
//!
//! Replaces the flat `Vec<u8>` per-page dirty state array with an
//! interval model matching Fuchsia's `VmPageList` dirty tracking:
//!
//! - **Committed pages** have per-page dirty state (Clean/Dirty/AwaitingClean).
//! - **Zero intervals** represent contiguous ranges of zero pages with a
//!   single dirty state and an `AwaitingCleanLength` that tracks how far
//!   writeback coverage extends from the interval start.
//!
//! The `AwaitingCleanLength` mechanism is the key to correct writeback
//! generation tracking: `WritebackBegin` records the length on the
//! interval start; `WritebackEnd` only cleans up to that length. This
//! allows overlapping/redundant writeback sequences to be independent.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::PAGE_SIZE;

/// Per-page dirty state constants (for committed pages and legacy API).
pub const PAGE_CLEAN: u8 = 0;
pub const PAGE_DIRTY: u8 = 1;
pub const PAGE_AWAITING_CLEAN: u8 = 2;
/// Zero page from resize — implicitly dirty.
pub const PAGE_ZERO_DIRTY: u8 = 3;
/// Page beyond stream_size on unbounded VMO.
pub const PAGE_UNTRACKED: u8 = 4;

/// Dirty state for a zero interval (no committed frame).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZeroDirtyState {
    /// Written back successfully; page source knows about the zeros.
    Clean,
    /// Newly created (from resize or decommit); needs writeback.
    Dirty,
    /// Beyond stream_size on an unbounded VMO; not tracked for dirty.
    Untracked,
}

/// An entry in the dirty tracker.
#[derive(Clone, Debug)]
enum Entry {
    /// A range of committed pages, each with its own dirty state.
    /// The Vec holds one state per page in the range. This allows
    /// individual pages within the range to have different states
    /// (e.g., some Dirty, some Clean after partial writeback).
    Pages(Vec<u8>),
    /// A zero interval (no committed frames) with a single dirty state
    /// and an optional AwaitingClean length (in pages from interval start).
    /// When `awaiting_clean_len > 0`, writeback has been initiated for
    /// the first `awaiting_clean_len` pages of this interval.
    ZeroInterval {
        len: usize,
        state: ZeroDirtyState,
        /// How many pages from the start of this interval are authorized
        /// to be cleaned by WritebackEnd. 0 = no writeback pending.
        /// Uses max() semantics on set (never shrinks except by clipping).
        awaiting_clean_len: usize,
    },
}

/// Interval-based dirty state tracker.
///
/// Stores a sequence of `Entry` values keyed by start page index.
/// The entries cover the entire `[0, page_count)` range without gaps.
pub struct DirtyTracker {
    /// Intervals keyed by start page index.
    entries: BTreeMap<usize, Entry>,
    /// Total number of pages tracked.
    page_count: usize,
}

impl DirtyTracker {
    /// Create an empty tracker (no pages).
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            page_count: 0,
        }
    }

    /// Create a tracker with `count` pages, all in the given state.
    pub fn new_with_state(count: usize, state: u8) -> Self {
        let mut t = Self::new();
        if count > 0 {
            t.entries.insert(0, Entry::Pages(alloc::vec![state; count]));
            t.page_count = count;
        }
        t
    }

    /// Whether the tracker has been initialized (has any pages).
    pub fn is_empty(&self) -> bool {
        self.page_count == 0
    }

    /// Total tracked pages.
    pub fn len(&self) -> usize {
        self.page_count
    }

    /// Get the dirty state of a single page (legacy u8 API).
    pub fn get(&self, page_idx: usize) -> u8 {
        if page_idx >= self.page_count {
            return PAGE_CLEAN;
        }
        let (start, entry) = match self.entries.range(..=page_idx).next_back() {
            Some(e) => e,
            None => return PAGE_CLEAN,
        };
        match entry {
            Entry::Pages(states) => {
                let offset = page_idx - start;
                if offset < states.len() {
                    states[offset]
                } else {
                    PAGE_CLEAN
                }
            }
            Entry::ZeroInterval { state, .. } => match state {
                ZeroDirtyState::Clean => PAGE_CLEAN,
                ZeroDirtyState::Dirty => PAGE_ZERO_DIRTY,
                ZeroDirtyState::Untracked => PAGE_UNTRACKED,
            },
        }
    }

    /// Set the dirty state of a single page.
    pub fn set(&mut self, page_idx: usize, state: u8) {
        if page_idx >= self.page_count {
            return;
        }
        // Find the entry containing this page.
        let (start, entry) = match self.entries.range_mut(..=page_idx).next_back() {
            Some((s, e)) => (*s, e),
            None => return,
        };
        match entry {
            Entry::Pages(states) => {
                let offset = page_idx - start;
                if offset < states.len() {
                    states[offset] = state;
                }
            }
            Entry::ZeroInterval { .. } => {
                // Cannot set individual page state on a zero interval.
                // The caller should split the interval first if needed.
                // For now, this is a no-op for zero intervals.
            }
        }
    }

    /// Ensure the tracker covers at least `pages` pages.
    /// Extends with PAGE_CLEAN entries if needed.
    pub fn ensure_size(&mut self, pages: usize) {
        if pages <= self.page_count {
            return;
        }
        // Add clean pages to fill the gap.
        let gap_start = self.page_count;
        let gap_len = pages - self.page_count;
        self.entries
            .insert(gap_start, Entry::Pages(alloc::vec![PAGE_CLEAN; gap_len]));
        self.page_count = pages;
    }

    /// Extend with PAGE_ZERO_DIRTY entries from `old_count` to `new_count`.
    /// Used during VMO resize-up for pager-backed VMOs.
    pub fn extend_zero_dirty(&mut self, old_count: usize, new_count: usize) {
        if new_count <= old_count {
            return;
        }
        // Ensure existing pages are tracked up to old_count.
        self.ensure_size(old_count);
        // Add the new zero-dirty interval.
        self.entries.insert(
            old_count,
            Entry::ZeroInterval {
                len: new_count - old_count,
                state: ZeroDirtyState::Dirty,
                awaiting_clean_len: 0,
            },
        );
        self.page_count = new_count;
    }

    /// Truncate to `new_count` pages.
    pub fn truncate(&mut self, new_count: usize) {
        if new_count >= self.page_count {
            return;
        }
        // Remove entries starting at or after new_count.
        let to_remove: Vec<usize> = self.entries.range(new_count..).map(|(&k, _)| k).collect();
        for k in to_remove {
            self.entries.remove(&k);
        }
        // Truncate the entry that spans new_count.
        if let Some((&start, entry)) = self.entries.range_mut(..=new_count).next_back() {
            match entry {
                Entry::Pages(states) => {
                    let keep = new_count - start;
                    states.truncate(keep);
                }
                Entry::ZeroInterval { len, .. } => {
                    *len = new_count - start;
                }
            }
        }
        self.page_count = new_count;
    }

    /// Mark a range of pages as dirty (PAGE_DIRTY).
    /// For committed pages, sets per-page state.
    /// For zero intervals, this is a no-op (zero intervals have
    /// their own dirty state).
    pub fn mark_range_dirty(&mut self, start: usize, end: usize) {
        for idx in start..end.min(self.page_count) {
            self.set(idx, PAGE_DIRTY);
        }
    }

    /// Mark a page as zero-dirty (from resize or zero operation).
    /// If the page is in a Pages entry, sets it to PAGE_ZERO_DIRTY.
    pub fn mark_zero_dirty(&mut self, page_idx: usize) {
        self.set(page_idx, PAGE_ZERO_DIRTY);
    }

    /// Begin writeback: transition dirty pages in range.
    ///
    /// For committed pages: Dirty → AwaitingClean (per-page).
    /// For zero-dirty intervals: records AwaitingCleanLength on the
    /// interval (max of existing and new coverage).
    ///
    /// `zero_only`: if true, skip committed PAGE_DIRTY pages (only
    /// process zero-dirty intervals).
    /// `is_pinned`: callback to check if a page is pinned.
    pub fn writeback_begin(
        &mut self,
        start_page: usize,
        end_page: usize,
        zero_only: bool,
        is_pinned: &dyn Fn(usize) -> bool,
    ) {
        let max_page = end_page.min(self.page_count);
        // Collect interval keys that overlap the range.
        let keys: Vec<usize> = self.entries.range(..max_page).map(|(&k, _)| k).collect();

        for key in keys {
            let entry = match self.entries.get_mut(&key) {
                Some(e) => e,
                None => continue,
            };
            match entry {
                Entry::Pages(states) => {
                    if zero_only {
                        continue;
                    }
                    let entry_end = key + states.len();
                    let overlap_start = start_page.max(key) - key;
                    let overlap_end = max_page.min(entry_end) - key;
                    for i in overlap_start..overlap_end {
                        if states[i] == PAGE_DIRTY && !is_pinned(key + i) {
                            states[i] = PAGE_AWAITING_CLEAN;
                        }
                    }
                }
                Entry::ZeroInterval {
                    len,
                    state,
                    awaiting_clean_len,
                } => {
                    if *state != ZeroDirtyState::Dirty {
                        continue;
                    }
                    let interval_end = key + *len;
                    if interval_end <= start_page {
                        continue; // No overlap.
                    }
                    // Only update AwaitingCleanLength if the begin range
                    // covers the interval START. In Fuchsia, the iterator
                    // only encounters the IntervalStart sentinel when the
                    // range includes it; starting mid-interval sees
                    // interior pages but NOT the start, so no ACL update.
                    if start_page <= key {
                        // New AwaitingCleanLength: from interval start
                        // to min(end_page, interval_end).
                        let new_len = max_page.min(interval_end) - key;
                        *awaiting_clean_len = (*awaiting_clean_len).max(new_len);
                    }
                }
            }
        }
    }

    /// End writeback: transition AwaitingClean pages to Clean.
    ///
    /// For committed pages: AwaitingClean → Clean.
    /// For zero-dirty intervals: cleans pages up to AwaitingCleanLength.
    ///
    /// Returns the list of page indices where zero markers should
    /// be removed (caller must do this outside the tracker).
    pub fn writeback_end(&mut self, start_page: usize, end_page: usize) -> Vec<usize> {
        let max_page = end_page.min(self.page_count);
        let mut remove_zero_markers = Vec::new();

        // Running high-water mark for zero interval cleaning.
        let mut awaiting_clean_end: usize = 0;

        let keys: Vec<usize> = self.entries.range(..max_page).map(|(&k, _)| k).collect();

        for key in keys {
            let entry = match self.entries.get_mut(&key) {
                Some(e) => e,
                None => continue,
            };
            match entry {
                Entry::Pages(states) => {
                    let entry_end = key + states.len();
                    let overlap_start = start_page.max(key) - key;
                    let overlap_end = max_page.min(entry_end) - key;
                    for i in overlap_start..overlap_end {
                        if states[i] == PAGE_AWAITING_CLEAN {
                            states[i] = PAGE_CLEAN;
                            remove_zero_markers.push(key + i);
                        }
                    }
                }
                Entry::ZeroInterval {
                    len,
                    state,
                    awaiting_clean_len,
                } => {
                    if *state != ZeroDirtyState::Dirty {
                        continue;
                    }
                    let interval_end = key + *len;
                    // Update running high-water mark.
                    awaiting_clean_end = awaiting_clean_end.max(key + *awaiting_clean_len);

                    // Clean pages within [max(start_page, key), min(interval_end, max_page, awaiting_clean_end)).
                    let clean_from = start_page.max(key);
                    let clean_to = interval_end.min(max_page).min(awaiting_clean_end);

                    if clean_from < clean_to {
                        // Pages [clean_from, clean_to) should be cleaned.
                        for pg in clean_from..clean_to {
                            remove_zero_markers.push(pg);
                        }

                        // Update the interval: split or shrink.
                        let cleaned_count = clean_to - key;
                        if cleaned_count >= *len {
                            // Entire interval cleaned → mark as Clean.
                            *state = ZeroDirtyState::Clean;
                            *awaiting_clean_len = 0;
                        } else {
                            // Partial clean: carry over remaining
                            // AwaitingCleanLength to the new start.
                            let remaining_acl = awaiting_clean_len.saturating_sub(cleaned_count);
                            let new_start = key + cleaned_count;
                            let new_len = *len - cleaned_count;
                            // Replace this entry with a clean prefix +
                            // dirty remainder.
                            *len = cleaned_count;
                            *state = ZeroDirtyState::Clean;
                            *awaiting_clean_len = 0;
                            // Insert the dirty remainder.
                            self.entries.insert(
                                new_start,
                                Entry::ZeroInterval {
                                    len: new_len,
                                    state: ZeroDirtyState::Dirty,
                                    awaiting_clean_len: remaining_acl,
                                },
                            );
                        }
                    }
                }
            }
        }
        remove_zero_markers
    }

    /// Query dirty ranges in [start_page, end_page).
    /// Returns Vec of (offset_bytes, length_bytes, options) tuples.
    /// `is_committed`: callback returns true if the page has a committed frame.
    /// `has_zero_marker`: callback returns true if the page has a zero marker.
    pub fn query_dirty_ranges(
        &self,
        start_page: usize,
        end_page: usize,
        is_committed: &dyn Fn(usize) -> bool,
        has_zero_marker: &dyn Fn(usize) -> bool,
    ) -> Vec<(u64, u64, u64)> {
        const ZX_VMO_DIRTY_RANGE_IS_ZERO: u64 = 1;
        let max_page = end_page.min(self.page_count);
        let mut ranges = Vec::new();
        let mut run_start: Option<(usize, u64)> = None;

        for pg in start_page..max_page {
            let state = self.get(pg);
            let is_dirty =
                state == PAGE_DIRTY || state == PAGE_AWAITING_CLEAN || state == PAGE_ZERO_DIRTY;

            if is_dirty {
                let committed = is_committed(pg);
                let has_zm = has_zero_marker(pg);
                let opts = if committed && !has_zm {
                    0u64
                } else {
                    ZX_VMO_DIRTY_RANGE_IS_ZERO
                };
                match run_start {
                    Some((_, run_opts)) if run_opts == opts => {}
                    Some((start, run_opts)) => {
                        ranges.push((
                            (start * PAGE_SIZE) as u64,
                            ((pg - start) * PAGE_SIZE) as u64,
                            run_opts,
                        ));
                        run_start = Some((pg, opts));
                    }
                    None => {
                        run_start = Some((pg, opts));
                    }
                }
            } else {
                if let Some((start, run_opts)) = run_start.take() {
                    ranges.push((
                        (start * PAGE_SIZE) as u64,
                        ((pg - start) * PAGE_SIZE) as u64,
                        run_opts,
                    ));
                }
            }
        }
        if let Some((start, run_opts)) = run_start {
            ranges.push((
                (start * PAGE_SIZE) as u64,
                ((max_page - start) * PAGE_SIZE) as u64,
                run_opts,
            ));
        }
        ranges
    }

    /// Iterate all pages, returning their dirty state.
    /// Used for migration from legacy API.
    pub fn iter_states(&self, start: usize, end: usize) -> Vec<u8> {
        let max = end.min(self.page_count);
        (start..max).map(|i| self.get(i)).collect()
    }
}

impl Default for DirtyTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_get_set() {
        let mut t = DirtyTracker::new_with_state(5, PAGE_CLEAN);
        assert_eq!(t.get(0), PAGE_CLEAN);
        t.set(2, PAGE_DIRTY);
        assert_eq!(t.get(2), PAGE_DIRTY);
        assert_eq!(t.get(3), PAGE_CLEAN);
    }

    #[test]
    fn extend_zero_dirty() {
        let mut t = DirtyTracker::new_with_state(3, PAGE_CLEAN);
        t.extend_zero_dirty(3, 8);
        assert_eq!(t.len(), 8);
        assert_eq!(t.get(2), PAGE_CLEAN);
        assert_eq!(t.get(3), PAGE_ZERO_DIRTY);
        assert_eq!(t.get(7), PAGE_ZERO_DIRTY);
    }

    #[test]
    fn writeback_begin_end_zero_interval() {
        let mut t = DirtyTracker::new_with_state(1, PAGE_CLEAN);
        t.extend_zero_dirty(1, 6); // pages 1-5 zero dirty

        // Begin writeback for pages 1-4.
        t.writeback_begin(1, 5, true, &|_| false);

        // End writeback for pages 1-4.
        let removed = t.writeback_end(1, 5);
        assert_eq!(removed.len(), 4); // pages 1-4 cleaned

        // Page 5 should still be zero-dirty.
        assert_eq!(t.get(5), PAGE_ZERO_DIRTY);
        // Pages 1-4 should be clean.
        assert_eq!(t.get(1), PAGE_CLEAN);
        assert_eq!(t.get(4), PAGE_CLEAN);
    }

    #[test]
    fn overlapping_writeback_generation() {
        // Test the key scenario from ResizeWritebackRepeatedNoTrapDirty.
        let mut t = DirtyTracker::new_with_state(1, PAGE_CLEAN);
        t.extend_zero_dirty(1, 10); // pages 1-9 zero dirty

        // First writeback: pages 1-4.
        t.writeback_begin(1, 5, true, &|_| false);
        // Redundant: pages 1-2 (sub-range). max(4, 2) = 4, no change.
        t.writeback_begin(1, 3, true, &|_| false);
        // End first writeback: pages 1-4 cleaned.
        let removed = t.writeback_end(1, 5);
        assert_eq!(removed.len(), 4);

        // All 4 pages clean, pages 5-9 still dirty.
        for pg in 1..5 {
            assert_eq!(t.get(pg), PAGE_CLEAN, "page {} should be clean", pg);
        }
        for pg in 5..10 {
            assert_eq!(t.get(pg), PAGE_ZERO_DIRTY, "page {} should be dirty", pg);
        }

        // End redundant writeback — no-op (pages already clean).
        let removed2 = t.writeback_end(1, 3);
        assert!(removed2.is_empty());

        // Begin writeback for pages 5-6 (covers interval start at 5).
        t.writeback_begin(5, 7, true, &|_| false);
        // Begin writeback for pages 7-9 — starts IN THE MIDDLE of
        // the interval [5, 9], so AwaitingCleanLength is NOT set.
        // This matches Fuchsia where the IntervalStart at page 5 is
        // outside the begin range [7, 10).
        t.writeback_begin(7, 10, true, &|_| false);

        // End first sub-range: pages 5-6 cleaned.
        let removed3 = t.writeback_end(5, 7);
        assert_eq!(removed3.len(), 2); // pages 5-6

        // End second sub-range: should be a NO-OP because
        // WritebackBegin(7, 10) didn't set AwaitingCleanLength
        // (started mid-interval). Pages 7-9 remain dirty.
        let removed4 = t.writeback_end(7, 10);
        assert!(removed4.is_empty(), "pages 7-9 should NOT be cleaned");

        // Verify pages 7-9 are still dirty.
        for pg in 7..10 {
            assert_eq!(
                t.get(pg),
                PAGE_ZERO_DIRTY,
                "page {} should still be dirty",
                pg
            );
        }
    }
}
