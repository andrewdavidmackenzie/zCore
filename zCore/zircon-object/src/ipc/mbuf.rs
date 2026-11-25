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
}
