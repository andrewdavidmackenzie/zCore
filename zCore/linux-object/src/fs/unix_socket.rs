//! Unix domain socket (AF_UNIX) implementation.
//!
//! Supports:
//! - `socketpair()` — connected SOCK_STREAM pairs
//! - `socket()` + `bind()` + `listen()` + `accept()` — server sockets
//! - `socket()` + `connect()` — client sockets
//!
//! Path-based addressing uses a global registry mapping filesystem
//! paths to listener queues.

use crate::fs::{FileLike, OpenFlags};
use crate::sync::{Event, EventBus};
use alloc::{
    boxed::Box,
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    string::String,
    sync::Arc,
    vec::Vec,
};
use core::any::Any;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use lock::Mutex;
use rcore_fs::vfs::PollStatus;
use zircon_object::impl_kobject;
use zircon_object::object::*;

use crate::error::{LxError, LxResult};

/// Global registry of bound Unix socket listeners.
///
/// Maps filesystem paths to listener queues. When a client calls
/// `connect(path)`, it looks up the listener here and pushes a
/// new connection into the queue.
/// Global registry — public for accept() in linux-syscall.
pub static UNIX_LISTENERS: Mutex<BTreeMap<String, Arc<UnixListener>>> = Mutex::new(BTreeMap::new());

/// A Unix domain socket listener (created by bind + listen).
pub struct UnixListener {
    /// Pending connections waiting to be accept()ed.
    /// Each entry is one end of a connected pair — the other end
    /// was returned to the connecting client.
    queue: Mutex<VecDeque<Arc<UnixSocketEnd>>>,
    /// Notifies accept() when a new connection arrives.
    eventbus: Mutex<EventBus>,
    /// Maximum queue length (backlog from listen()).
    _backlog: usize,
}

impl UnixListener {
    fn new(backlog: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            eventbus: Mutex::new(EventBus::default()),
            _backlog: backlog,
        }
    }

    /// Push a new connection into the accept queue.
    fn push_connection(&self, end: Arc<UnixSocketEnd>) {
        self.queue.lock().push_back(end);
        self.eventbus.lock().set(Event::READABLE);
    }

    /// Pop a connection from the accept queue.
    pub fn pop_connection(&self) -> Option<Arc<UnixSocketEnd>> {
        let mut q = self.queue.lock();
        let conn = q.pop_front();
        if q.is_empty() {
            self.eventbus.lock().clear(Event::READABLE);
        }
        conn
    }

    /// Check if there are pending connections.
    pub fn has_pending(&self) -> bool {
        !self.queue.lock().is_empty()
    }
}

/// Bind a path to a listener in the global registry.
pub fn bind_listener(path: String, backlog: usize) -> LxResult<Arc<UnixListener>> {
    let mut listeners = UNIX_LISTENERS.lock();
    if listeners.contains_key(&path) {
        return Err(LxError::EADDRINUSE);
    }
    let listener = Arc::new(UnixListener::new(backlog));
    listeners.insert(path, listener.clone());
    Ok(listener)
}

/// Connect to a listener at the given path.
/// Returns the client's end of the connected socket pair.
pub fn connect_to(path: &str) -> LxResult<Arc<UnixSocketEnd>> {
    let listeners = UNIX_LISTENERS.lock();
    let listener = listeners.get(path).ok_or(LxError::ECONNREFUSED)?;
    // Create a connected pair — server gets one end, client gets the other.
    let (server_end, client_end) = UnixSocketEnd::create_pair();
    listener.push_connection(server_end);
    Ok(client_end)
}

/// Remove a listener from the registry (called on close/cleanup).
pub fn unbind_listener(path: &str) {
    UNIX_LISTENERS.lock().remove(path);
}

/// Ancillary data attached to a message (e.g. SCM_RIGHTS file descriptors).
///
/// Each ancillary message is paired with a byte offset in the data stream,
/// so `recvmsg` can deliver them with the correct data segment.
#[derive(Clone)]
pub struct AncillaryFds {
    /// File-like objects being passed (SCM_RIGHTS).
    pub fds: Vec<Arc<dyn FileLike>>,
}

/// Shared data for one direction of a Unix socket pair.
struct ChannelData {
    buf: VecDeque<u8>,
    /// Queue of ancillary fd sets waiting to be received.
    /// Each `sendmsg` with SCM_RIGHTS pushes one entry; each `recvmsg`
    /// pops it.
    ancillary_fds: VecDeque<AncillaryFds>,
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
            ancillary_fds: VecDeque::new(),
            eventbus: EventBus::default(),
            ref_count: 2,
        }));
        let channel_ba = Arc::new(Mutex::new(ChannelData {
            buf: VecDeque::new(),
            ancillary_fds: VecDeque::new(),
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

    /// Write data to the peer, optionally attaching file descriptors.
    ///
    /// Used by `sendmsg` (with fds) and `sendto`/`write` (without).
    pub fn send_with_fds(&self, data: &[u8], fds: Option<AncillaryFds>) -> LxResult<usize> {
        if self.is_peer_closed() {
            return Err(LxError::EPIPE);
        }
        let mut ch = self.write_channel.lock();
        for &b in data {
            ch.buf.push_back(b);
        }
        if let Some(ancillary) = fds {
            if !ancillary.fds.is_empty() {
                ch.ancillary_fds.push_back(ancillary);
            }
        }
        ch.eventbus.set(Event::READABLE);
        Ok(data.len())
    }

    /// Read data from the channel, optionally receiving file descriptors.
    ///
    /// Returns `(bytes_read, ancillary_fds)`. The ancillary fds are
    /// drained from the queue — at most one set per `recvmsg` call.
    pub async fn recv_with_fds(&self, buf: &mut [u8]) -> LxResult<(usize, Option<AncillaryFds>)> {
        if buf.is_empty() {
            return Ok((0, None));
        }
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
                    // Pop one ancillary message if available.
                    let ancillary = ch.ancillary_fds.pop_front();
                    return Ok((len, ancillary));
                }
                if ch.ref_count < 2 {
                    return Ok((0, None));
                }
            }
            ReadFuture { socket: self }.await;
        }
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
        self.send_with_fds(buf, None)
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
