//! Unix domain socket (AF_UNIX) -- minimal implementation.
//!
//! Currently supports `socketpair()` only (connected SOCK_STREAM pairs).
//! Path-based addressing (bind/listen/accept/connect) is not yet implemented.

use crate::fs::{FileLike, OpenFlags};
use crate::sync::{Event, EventBus};
use alloc::{boxed::Box, collections::vec_deque::VecDeque, sync::Arc};
use core::any::Any;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use lock::Mutex;
use rcore_fs::vfs::PollStatus;
use zircon_object::impl_kobject;
use zircon_object::object::*;

use crate::error::{LxError, LxResult};

/// Shared data for one direction of a Unix socket pair.
struct ChannelData {
    buf: VecDeque<u8>,
    eventbus: EventBus,
    /// Number of endpoints referencing this channel.
    /// When it drops to 0, the channel is closed.
    ref_count: u32,
}

/// One end of a Unix domain socket pair.
///
/// Each end has a `read_channel` (incoming data) and a `write_channel`
/// (outgoing data). The write channel of one end is the read channel
/// of the other.
pub struct UnixSocketEnd {
    base: KObjectBase,
    flags: Mutex<OpenFlags>,
    /// Channel we read from (peer writes here).
    read_channel: Arc<Mutex<ChannelData>>,
    /// Channel we write to (peer reads from here).
    write_channel: Arc<Mutex<ChannelData>>,
}

impl_kobject!(UnixSocketEnd);

impl UnixSocketEnd {
    /// Create a connected pair of Unix domain sockets.
    ///
    /// Returns `(end_a, end_b)` where writing to `end_a` can be read
    /// from `end_b` and vice versa.
    pub fn create_pair() -> (Arc<Self>, Arc<Self>) {
        let channel_ab = Arc::new(Mutex::new(ChannelData {
            buf: VecDeque::new(),
            eventbus: EventBus::default(),
            ref_count: 2,
        }));
        let channel_ba = Arc::new(Mutex::new(ChannelData {
            buf: VecDeque::new(),
            eventbus: EventBus::default(),
            ref_count: 2,
        }));
        let a = Arc::new(UnixSocketEnd {
            base: KObjectBase::new(),
            flags: Mutex::new(OpenFlags::RDWR),
            read_channel: channel_ab.clone(),
            write_channel: channel_ba.clone(),
        });
        let b = Arc::new(UnixSocketEnd {
            base: KObjectBase::new(),
            flags: Mutex::new(OpenFlags::RDWR),
            read_channel: channel_ba,
            write_channel: channel_ab,
        });
        (a, b)
    }

    fn can_read(&self) -> bool {
        let ch = self.read_channel.lock();
        !ch.buf.is_empty() || ch.ref_count < 2
    }

    fn can_write(&self) -> bool {
        self.write_channel.lock().ref_count == 2
    }

    fn is_peer_closed(&self) -> bool {
        self.write_channel.lock().ref_count < 2
    }
}

impl Drop for UnixSocketEnd {
    fn drop(&mut self) {
        // Decrement ref count on both channels.
        {
            let mut ch = self.read_channel.lock();
            ch.ref_count -= 1;
            ch.eventbus.set(Event::CLOSED);
        }
        {
            let mut ch = self.write_channel.lock();
            ch.ref_count -= 1;
            ch.eventbus.set(Event::CLOSED);
        }
    }
}

#[async_trait::async_trait]
impl FileLike for UnixSocketEnd {
    fn flags(&self) -> OpenFlags {
        *self.flags.lock()
    }

    fn set_flags(&self, f: OpenFlags) -> LxResult {
        *self.flags.lock() = f;
        Ok(())
    }

    fn dup(&self) -> LxResult<Arc<dyn FileLike>> {
        // Increment ref counts on both channels.
        self.read_channel.lock().ref_count += 1;
        self.write_channel.lock().ref_count += 1;
        Ok(Arc::new(UnixSocketEnd {
            base: KObjectBase::new(),
            flags: Mutex::new(*self.flags.lock()),
            read_channel: self.read_channel.clone(),
            write_channel: self.write_channel.clone(),
        }))
    }

    async fn read(&self, buf: &mut [u8]) -> LxResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // If buffer is empty and peer is still connected, block.
        loop {
            {
                let mut ch = self.read_channel.lock();
                if !ch.buf.is_empty() {
                    let len = core::cmp::min(buf.len(), ch.buf.len());
                    for item in buf.iter_mut().take(len) {
                        *item = ch.buf.pop_front().unwrap();
                    }
                    if ch.buf.is_empty() {
                        ch.eventbus.clear(Event::READABLE);
                    }
                    return Ok(len);
                }
                if ch.ref_count < 2 {
                    // Peer closed -- return 0 (EOF).
                    return Ok(0);
                }
            }
            // Wait for data or peer close.
            ReadFuture { socket: self }.await;
        }
    }

    fn write(&self, buf: &[u8]) -> LxResult<usize> {
        if self.is_peer_closed() {
            return Err(LxError::EPIPE);
        }
        let mut ch = self.write_channel.lock();
        for &b in buf {
            ch.buf.push_back(b);
        }
        ch.eventbus.set(Event::READABLE);
        Ok(buf.len())
    }

    async fn read_at(&self, _offset: u64, buf: &mut [u8]) -> LxResult<usize> {
        self.read(buf).await
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> LxResult<usize> {
        self.write(buf)
    }

    fn poll(&self, _events: crate::fs::PollEvents) -> LxResult<PollStatus> {
        Ok(PollStatus {
            read: self.can_read(),
            write: self.can_write(),
            error: false,
        })
    }

    async fn async_poll(&self, _events: crate::fs::PollEvents) -> LxResult<PollStatus> {
        self.poll(_events)
    }

    fn as_socket(&self) -> LxResult<&dyn Any> {
        Ok(self)
    }
}

/// Future that resolves when data is available to read or peer closes.
struct ReadFuture<'a> {
    socket: &'a UnixSocketEnd,
}

impl Future for ReadFuture<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        if self.socket.can_read() {
            return Poll::Ready(());
        }
        let waker = cx.waker().clone();
        let mut ch = self.socket.read_channel.lock();
        ch.eventbus.subscribe(Box::new(move |_| {
            waker.wake_by_ref();
            true
        }));
        Poll::Pending
    }
}
