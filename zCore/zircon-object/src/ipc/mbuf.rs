//! Fixed-size buffer chain for socket payloads — zCore's port of
//! Zircon's `kernel/object/mbuf.rs` (#901, step 1).
//!
//! Zircon stores socket data in a chain of fixed-size `MBuf` nodes
//! (page-sized minus a small header). The fixed granularity is the
//! architectural point: appends allocate in bounded, equal chunks
//! rather than growing one contiguous buffer, and (step 2) user
//! memory can be copied directly into and out of node payloads —
//! no count-sized intermediate kernel buffer.
//!
//! This port keeps the node structure and granularity but holds the
//! nodes in a `VecDeque<Box<MBuf>>` instead of a hand-rolled linked
//! list with a raw tail pointer: same O(1) push/pop at both ends,
//! same fixed-size allocations, no unsafe code.

use alloc::boxed::Box;
use alloc::collections::VecDeque;

/// Payload bytes per node. Mirrors Zircon: page size minus the mbuf
/// header (`MBuf::PAYLOAD_SIZE = page::SIZE - HEADER_SIZE`).
pub(super) const MBUF_PAYLOAD: usize = 4064;

struct MBuf {
    data: [u8; MBUF_PAYLOAD],
    /// Offset of the first valid byte (advances as the head is consumed).
    off: usize,
    /// Number of valid bytes starting at `off`.
    len: usize,
}

impl MBuf {
    fn new() -> Self {
        MBuf {
            data: [0; MBUF_PAYLOAD],
            off: 0,
            len: 0,
        }
    }

    /// Bytes appended after the current valid region.
    fn tail_space(&self) -> usize {
        MBUF_PAYLOAD - self.off - self.len
    }
}

/// The buffered payload of one socket endpoint.
///
/// Stream mode: an unstructured byte chain. Datagram mode: the same
/// byte chain plus per-datagram lengths (`datagram_len`), whose sum
/// always equals the buffered byte count (INV-SOCKET-1).
pub(super) struct MBufChain {
    nodes: VecDeque<Box<MBuf>>,
    /// Total buffered bytes across all nodes.
    size: usize,
    /// Datagram lengths, in order. Empty in stream mode.
    datagram_len: VecDeque<usize>,
}

impl Default for MBufChain {
    fn default() -> Self {
        Self::new()
    }
}

impl MBufChain {
    pub(super) fn new() -> Self {
        MBufChain {
            nodes: VecDeque::new(),
            size: 0,
            datagram_len: VecDeque::new(),
        }
    }

    /// Total buffered bytes.
    pub(super) fn len(&self) -> usize {
        self.size
    }

    pub(super) fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Length of the front datagram, if any.
    pub(super) fn front_datagram_len(&self) -> Option<usize> {
        self.datagram_len.front().copied()
    }

    /// Sum of the recorded datagram lengths (INV-SOCKET-1 accounting).
    #[cfg(feature = "invariant-checks")]
    pub(super) fn datagram_bytes_accounted(&self) -> usize {
        self.datagram_len.iter().sum()
    }

    /// Length of the most recently written datagram.
    #[cfg(feature = "invariant-checks")]
    pub(super) fn last_datagram_len(&self) -> Option<usize> {
        self.datagram_len.back().copied()
    }

    /// Append stream bytes: fill the tail node's free space, then
    /// whole new nodes.
    pub(super) fn write_stream(&mut self, data: &[u8]) {
        self.append(data);
    }

    /// Append one datagram and record its length.
    pub(super) fn write_datagram(&mut self, data: &[u8]) {
        self.append(data);
        self.datagram_len.push_back(data.len());
    }

    fn append(&mut self, data: &[u8]) {
        let mut remaining = data;
        if remaining.is_empty() {
            return;
        }
        // Fill the tail node's free space first.
        if let Some(tail) = self.nodes.back_mut() {
            let free = tail.tail_space();
            if free > 0 {
                let n = free.min(remaining.len());
                let start = tail.off + tail.len;
                tail.data[start..start + n].copy_from_slice(&remaining[..n]);
                tail.len += n;
                self.size += n;
                remaining = &remaining[n..];
            }
        }
        // Whole new nodes for the rest.
        while !remaining.is_empty() {
            let n = remaining.len().min(MBUF_PAYLOAD);
            let mut node = Box::new(MBuf::new());
            node.data[..n].copy_from_slice(&remaining[..n]);
            node.len = n;
            self.size += n;
            remaining = &remaining[n..];
            self.nodes.push_back(node);
        }
    }

    /// Read stream bytes: copy up to `buf.len()` bytes (bounded by
    /// what is buffered) from the chain front. If `consume`, remove
    /// them; otherwise leave the chain untouched (peek).
    pub(super) fn read_stream(&mut self, buf: &mut [u8], consume: bool) -> usize {
        let want = buf.len().min(self.size);
        self.copy_out(buf, want, consume)
    }

    /// Read the front datagram: copy up to `buf.len()` bytes of it.
    /// A consuming read discards the WHOLE datagram even if only part
    /// was copied (the remainder is dropped, matching Zircon). A peek
    /// leaves everything buffered.
    pub(super) fn read_datagram(&mut self, buf: &mut [u8], consume: bool) -> usize {
        let Some(&datagram_len) = self.datagram_len.front() else {
            return 0;
        };
        let read_size = buf.len().min(datagram_len);
        if consume {
            // Copy the visible part, then discard the whole datagram.
            let copied = self.copy_out(buf, read_size, false);
            self.discard_front(datagram_len);
            self.datagram_len.pop_front();
            copied
        } else {
            self.copy_out(buf, read_size, false)
        }
    }

    /// Copy `want` bytes from the chain front into `buf`, optionally
    /// consuming them.
    fn copy_out(&mut self, buf: &mut [u8], want: usize, consume: bool) -> usize {
        let mut filled = 0;
        if !consume {
            // Peek: walk the nodes without mutating.
            for node in self.nodes.iter() {
                if filled >= want || filled >= buf.len() {
                    break;
                }
                let n = (want - filled).min(node.len).min(buf.len() - filled);
                let start = node.off;
                buf[filled..filled + n].copy_from_slice(&node.data[start..start + n]);
                filled += n;
            }
        } else {
            while filled < want && filled < buf.len() {
                let Some(node) = self.nodes.front_mut() else {
                    break;
                };
                let n = (want - filled).min(node.len).min(buf.len() - filled);
                let start = node.off;
                buf[filled..filled + n].copy_from_slice(&node.data[start..start + n]);
                node.off += n;
                node.len -= n;
                self.size -= n;
                filled += n;
                if node.len == 0 {
                    self.nodes.pop_front();
                }
            }
        }
        filled
    }

    /// Append `len` bytes produced by `fill`, which is called with
    /// each destination region (the tail node's free space, then
    /// whole new nodes) and must fill it entirely or fail.
    ///
    /// On failure the chain is rolled back to its pre-call state —
    /// a faulting source (user-memory copy) leaves no partial data.
    /// This is zCore's stricter variant of Zircon's semantics
    /// (Zircon commits partially-written tail bytes; its mbuf.rs
    /// carries a TODO acknowledging the resulting data duplication).
    pub(super) fn append_with<E>(
        &mut self,
        len: usize,
        mut fill: impl FnMut(&mut [u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<(), E> {
        let mark_size = self.size;
        let mark_nodes = self.nodes.len();
        let mut tail_filled = 0usize;
        let result = (|| {
            let mut remaining = len;
            if remaining > 0 {
                if let Some(tail) = self.nodes.back_mut() {
                    let free = tail.tail_space();
                    if free > 0 {
                        let n = free.min(remaining);
                        let start = tail.off + tail.len;
                        fill(&mut tail.data[start..start + n])?;
                        tail.len += n;
                        self.size += n;
                        tail_filled = n;
                        remaining -= n;
                    }
                }
            }
            while remaining > 0 {
                let n = remaining.min(MBUF_PAYLOAD);
                let mut node = Box::new(MBuf::new());
                fill(&mut node.data[..n])?;
                node.len = n;
                self.size += n;
                remaining -= n;
                self.nodes.push_back(node);
            }
            Ok(())
        })();
        if result.is_err() {
            // Roll back: drop the nodes this call added and un-fill
            // whatever landed in the pre-existing tail.
            while self.nodes.len() > mark_nodes {
                self.nodes.pop_back();
            }
            if tail_filled > 0 {
                if let Some(tail) = self.nodes.back_mut() {
                    tail.len -= tail_filled;
                }
            }
            self.size = mark_size;
        }
        result
    }

    /// Append `len` stream bytes from a source that fills each region.
    pub(super) fn write_stream_with<E>(
        &mut self,
        len: usize,
        fill: impl FnMut(&mut [u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<(), E> {
        self.append_with(len, fill)
    }

    /// Append one datagram of `len` bytes from a filling source, with
    /// the datagram length recorded only on success (all-or-nothing).
    pub(super) fn write_datagram_with<E>(
        &mut self,
        len: usize,
        fill: impl FnMut(&mut [u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<(), E> {
        self.append_with(len, fill)?;
        self.datagram_len.push_back(len);
        Ok(())
    }

    /// Copy up to `len` stream bytes from the chain front to `sink`,
    /// which is called with each node's valid region and must consume
    /// it entirely or fail.
    ///
    /// With `consume`, successfully-copied regions are removed from
    /// the chain even if a later region fails (Zircon's read
    /// semantics: the copied prefix is committed); returns the total
    /// copied. With `!consume` (peek), the chain is never modified.
    /// Copy up to `len` stream bytes from the chain front to `sink`,
    /// which is called with each node's valid region and must consume
    /// it entirely or fail.
    ///
    /// With `consume`, successfully-copied regions are removed from
    /// the chain even if a later region fails (Zircon's read
    /// semantics: the copied prefix is committed). Returns
    /// `(copied, consumed, fault)`: `copied` reached the destination,
    /// `consumed` left the chain (equal for streams — a peek consumes
    /// nothing), and the fault if any. The Socket layer needs all
    /// three: userspace gets `copied`, the signal updates guard on
    /// `consumed` (a faulted datagram read discards the whole
    /// datagram with `copied == 0` — the signals must still update).
    pub(super) fn read_stream_with<E>(
        &mut self,
        len: usize,
        mut sink: impl FnMut(&[u8]) -> core::result::Result<(), E>,
        consume: bool,
    ) -> (usize, usize, Option<E>) {
        let want = len.min(self.size);
        let mut filled = 0usize;
        if !consume {
            // Peek: walk the nodes without mutating.
            for node in self.nodes.iter() {
                if filled >= want {
                    break;
                }
                let n = (want - filled).min(node.len);
                let start = node.off;
                if let Err(e) = sink(&node.data[start..start + n]) {
                    return (filled, 0, Some(e));
                }
                filled += n;
            }
            return (filled, 0, None);
        }
        while filled < want {
            let Some(node) = self.nodes.front_mut() else {
                break;
            };
            let n = (want - filled).min(node.len);
            let start = node.off;
            if let Err(e) = sink(&node.data[start..start + n]) {
                return (filled, filled, Some(e));
            }
            node.off += n;
            node.len -= n;
            self.size -= n;
            filled += n;
            if node.len == 0 {
                self.nodes.pop_front();
            }
        }
        (filled, filled, None)
    }

    /// Copy up to `len` bytes of the front datagram to `sink`.
    ///
    /// With `consume`, the WHOLE datagram is discarded even if the
    /// copy fails partway (Zircon: "If an error occurs while copying
    /// a datagram to dst, the datagram is dropped"); the copied
    /// prefix is still delivered to the sink. With `!consume`, the
    /// chain is untouched. Returns `(copied, consumed, fault)` — see
    /// `read_stream_with`; for a consuming datagram read `consumed`
    /// is the full datagram length even when `copied` is 0 (fault),
    /// so the caller's signal updates are not skipped.
    pub(super) fn read_datagram_with<E>(
        &mut self,
        len: usize,
        mut sink: impl FnMut(&[u8]) -> core::result::Result<(), E>,
        consume: bool,
    ) -> (usize, usize, Option<E>) {
        let Some(&datagram_len) = self.datagram_len.front() else {
            return (0, 0, None);
        };
        let want = len.min(datagram_len);
        // Copy (peek-style walk), tracking progress.
        let mut filled = 0usize;
        let mut node_idx = 0usize;
        let mut fault = None;
        while filled < want {
            let Some(node) = self.nodes.get(node_idx) else {
                break;
            };
            let n = (want - filled).min(node.len);
            let start = node.off;
            if let Err(e) = sink(&node.data[start..start + n]) {
                fault = Some(e);
                break;
            }
            filled += n;
            node_idx += 1;
        }
        let consumed = if consume {
            // The datagram is consumed regardless of the copy result.
            self.discard_front(datagram_len);
            self.datagram_len.pop_front();
            datagram_len
        } else {
            0
        };
        (filled, consumed, fault)
    }

    /// Discard `n` bytes from the chain front without copying.
    fn discard_front(&mut self, n: usize) {
        let mut remaining = n.min(self.size);
        while remaining > 0 {
            let Some(node) = self.nodes.front_mut() else {
                break;
            };
            let take = remaining.min(node.len);
            node.off += take;
            node.len -= take;
            self.size -= take;
            remaining -= take;
            if node.len == 0 {
                self.nodes.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_write_read_consume() {
        let mut chain = MBufChain::new();
        assert!(chain.is_empty());
        chain.write_stream(&[1, 2, 3]);
        chain.write_stream(&[4, 5]);
        assert_eq!(chain.len(), 5);

        let mut buf = [0u8; 3];
        assert_eq!(chain.read_stream(&mut buf, true), 3);
        assert_eq!(buf, [1, 2, 3]);
        assert_eq!(chain.len(), 2);
        let mut buf = [0u8; 10];
        assert_eq!(chain.read_stream(&mut buf, false), 2);
        assert_eq!(&buf[..2], &[4, 5]);
        assert_eq!(chain.len(), 2);
        assert_eq!(chain.read_stream(&mut buf, true), 2);
        assert!(chain.is_empty());
    }

    #[test]
    fn stream_spans_nodes() {
        // Payload larger than one node: node boundaries are invisible.
        let mut chain = MBufChain::new();
        let data: Vec<u8> = (0..MBUF_PAYLOAD * 2 + 7).map(|i| (i % 251) as u8).collect();
        chain.write_stream(&data);
        assert_eq!(chain.len(), data.len());

        let mut buf = vec![0u8; data.len()];
        assert_eq!(chain.read_stream(&mut buf, false), data.len());
        assert_eq!(buf, data);
        assert_eq!(chain.len(), data.len());
        assert_eq!(chain.read_stream(&mut buf, true), data.len());
        assert!(chain.is_empty());
    }

    #[test]
    fn stream_fills_tail_partial_node() {
        // A second write must fill the tail node's free space before
        // allocating a new node.
        let mut chain = MBufChain::new();
        chain.write_stream(&[1, 2, 3]);
        chain.write_stream(&[4, 5, 6]);
        // 6 bytes still fit in the first node.
        assert_eq!(chain.nodes.len(), 1);
        let mut buf = [0u8; 6];
        assert_eq!(chain.read_stream(&mut buf, true), 6);
        assert_eq!(buf, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn datagram_write_read() {
        let mut chain = MBufChain::new();
        chain.write_datagram(&[1, 2, 3]);
        chain.write_datagram(&[4, 5, 6, 7]);
        assert_eq!(chain.len(), 7);
        #[cfg(feature = "invariant-checks")]
        assert_eq!(chain.datagram_bytes_accounted(), 7);
        assert_eq!(chain.front_datagram_len(), Some(3));

        // Peek the first datagram.
        let mut buf = [0u8; 4];
        assert_eq!(chain.read_datagram(&mut buf, false), 3);
        assert_eq!(&buf[..3], &[1, 2, 3]);
        assert_eq!(chain.len(), 7);

        // Consume with a smaller buffer: the rest of the datagram is
        // discarded.
        let mut buf = [0u8; 2];
        assert_eq!(chain.read_datagram(&mut buf, true), 2);
        assert_eq!(&buf, &[1, 2]);
        assert_eq!(chain.len(), 4);
        assert_eq!(chain.front_datagram_len(), Some(4));

        // Consume the second datagram whole.
        let mut buf = [0u8; 10];
        assert_eq!(chain.read_datagram(&mut buf, true), 4);
        assert_eq!(&buf[..4], &[4, 5, 6, 7]);
        assert!(chain.is_empty());
    }

    #[test]
    fn datagram_spans_nodes() {
        let mut chain = MBufChain::new();
        let big: Vec<u8> = (0..MBUF_PAYLOAD * 2).map(|i| (i % 251) as u8).collect();
        chain.write_datagram(&big);
        chain.write_datagram(&[9, 9]);
        assert_eq!(chain.len(), MBUF_PAYLOAD * 2 + 2);

        let mut buf = vec![0u8; MBUF_PAYLOAD * 2];
        assert_eq!(chain.read_datagram(&mut buf, false), buf.len());
        assert_eq!(buf, big);
        assert_eq!(chain.len(), MBUF_PAYLOAD * 2 + 2);

        assert_eq!(chain.read_datagram(&mut buf, true), buf.len());
        assert_eq!(chain.len(), 2);
        assert_eq!(chain.front_datagram_len(), Some(2));
    }

    #[test]
    fn zero_length_reads() {
        let mut chain = MBufChain::new();
        chain.write_stream(&[1]);
        let mut buf = [0u8; 0];
        assert_eq!(chain.read_stream(&mut buf, true), 0);
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn append_with_fault_rolls_back() {
        // A failing fill (user-memory fault) leaves the chain at its
        // pre-write state — including the tail's free space.
        let mut chain = MBufChain::new();
        chain.write_stream(&[1, 2, 3]);
        let mark = chain.len();
        let tail_free = MBUF_PAYLOAD - 3;

        // The append length exceeds the tail's free space, so the
        // second fill targets a new node and fails there.
        let mut calls = 0;
        let r: core::result::Result<(), ()> = chain.append_with(tail_free + 10, |dst| {
            calls += 1;
            if calls == 1 {
                assert_eq!(dst.len(), tail_free);
                dst.fill(9);
                Ok(())
            } else {
                Err(())
            }
        });
        assert!(r.is_err());
        assert_eq!(calls, 2);
        assert_eq!(chain.len(), mark, "rollback must restore the size");
        let mut buf = [0u8; 3];
        assert_eq!(chain.read_stream(&mut buf, false), 3);
        assert_eq!(buf, [1, 2, 3], "original data must be intact");
        // The tail's free space must still be usable after rollback.
        chain.write_stream(&[4, 5, 6]);
        assert_eq!(chain.len(), 6);
        let mut buf = [0u8; 6];
        assert_eq!(chain.read_stream(&mut buf, false), 6);
        assert_eq!(buf, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn datagram_with_fault_rolls_back_length() {
        let mut chain = MBufChain::new();
        chain.write_datagram(&[1, 2, 3]);
        let r: core::result::Result<(), ()> = chain.write_datagram_with(2, |_| Err(()));
        assert!(r.is_err());
        // No phantom datagram length may survive the rollback.
        assert_eq!(chain.front_datagram_len(), Some(3));
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn stream_read_with_fault_commits_prefix() {
        // A failing sink mid-read consumes only the successfully
        // copied regions; the count survives the fault.
        let mut chain = MBufChain::new();
        let data: Vec<u8> = (0..MBUF_PAYLOAD + 10).map(|i| i as u8).collect();
        chain.write_stream(&data);

        let mut sink_calls = 0;
        let (copied, consumed, fault) = chain.read_stream_with(
            usize::MAX,
            |src| {
                sink_calls += 1;
                if sink_calls == 1 {
                    // First node region copies fine.
                    assert_eq!(src.len(), MBUF_PAYLOAD);
                    Ok(())
                } else {
                    Err(())
                }
            },
            true,
        );
        assert!(fault.is_some());
        assert_eq!(copied, MBUF_PAYLOAD, "the first region was copied");
        assert_eq!(consumed, MBUF_PAYLOAD, "stream consumed == copied");
        assert_eq!(chain.len(), 10, "the faulting region is not consumed");
    }

    #[test]
    fn datagram_read_with_fault_drops_datagram() {
        // Zircon: a fault while copying a datagram drops it entirely.
        let mut chain = MBufChain::new();
        chain.write_datagram(&[1, 2, 3]);
        chain.write_datagram(&[7, 8]);
        let (copied, consumed, fault) = chain.read_datagram_with(3, |_| Err(()), true);
        assert!(fault.is_some());
        assert_eq!(copied, 0);
        // The signal-consistency contract (#910 review): consumed
        // carries the discarded datagram's length even on fault.
        assert_eq!(consumed, 3, "the discarded datagram is accounted");
        assert_eq!(chain.len(), 2, "the whole datagram is discarded");
        assert_eq!(chain.front_datagram_len(), Some(2));
    }

    #[test]
    fn peek_read_with_fault_consumes_nothing() {
        let mut chain = MBufChain::new();
        chain.write_stream(&[1, 2, 3]);
        let (copied, consumed, fault) = chain.read_stream_with(3, |_| Err(()), false);
        assert!(fault.is_some());
        assert_eq!(copied, 0);
        assert_eq!(consumed, 0, "peek never consumes");
        assert_eq!(chain.len(), 3, "peek never consumes");
    }
}
