//! Epoll file descriptor implementation
//!
//! Provides `epoll_create1`, `epoll_ctl`, and `epoll_pwait` support.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use async_trait::async_trait;
use lock::Mutex;
use zircon_object::object::*;

use crate::error::{LxError, LxResult};
use crate::fs::{FileLike, OpenFlags, PollEvents, PollStatus};

/// Epoll control operation: add a file descriptor to the interest list.
pub const EPOLL_CTL_ADD: usize = 1;
/// Epoll control operation: delete a file descriptor from the interest list.
pub const EPOLL_CTL_DEL: usize = 2;
/// Epoll control operation: modify the events for a file descriptor.
pub const EPOLL_CTL_MOD: usize = 3;

/// Epoll event structure, matching the Linux kernel ABI.
///
/// On x86_64 this struct is packed, but for aarch64/riscv64 it is naturally
/// aligned. We use `repr(C)` which is correct for our supported targets.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct EpollEvent {
    /// Epoll event flags (EPOLLIN, EPOLLOUT, etc.)
    pub events: u32,
    /// User data (opaque, returned as-is in epoll_wait)
    pub data: u64,
}

/// An entry in the epoll interest list.
struct EpollEntry {
    /// The registered event interest and user data.
    event: EpollEvent,
    /// Cached reference to the monitored file.
    file: Arc<dyn FileLike>,
}

/// Inner state of an epoll instance, protected by a mutex.
struct EpollInner {
    /// Open flags (e.g. CLOEXEC).
    flags: OpenFlags,
    /// Map from monitored file descriptor to its interest entry.
    interest: BTreeMap<i32, EpollEntry>,
}

/// An epoll file descriptor.
///
/// Implements [`FileLike`] so it can be stored in the process fd table.
pub struct EpollFile {
    base: KObjectBase,
    inner: Mutex<EpollInner>,
}

impl_kobject!(EpollFile);

impl EpollFile {
    /// Create a new epoll instance with the given flags.
    pub fn new(flags: OpenFlags) -> Arc<Self> {
        Arc::new(EpollFile {
            base: KObjectBase::new(),
            inner: Mutex::new(EpollInner {
                flags,
                interest: BTreeMap::new(),
            }),
        })
    }

    /// Add a file descriptor to the interest list (EPOLL_CTL_ADD).
    pub fn ctl_add(&self, fd: i32, event: EpollEvent, file: Arc<dyn FileLike>) -> LxResult {
        let mut inner = self.inner.lock();
        if inner.interest.contains_key(&fd) {
            return Err(LxError::EEXIST);
        }
        inner.interest.insert(fd, EpollEntry { event, file });
        Ok(())
    }

    /// Modify the events for a file descriptor (EPOLL_CTL_MOD).
    pub fn ctl_mod(&self, fd: i32, event: EpollEvent) -> LxResult {
        let mut inner = self.inner.lock();
        match inner.interest.get_mut(&fd) {
            Some(entry) => {
                entry.event = event;
                Ok(())
            }
            None => Err(LxError::ENOENT),
        }
    }

    /// Remove a file descriptor from the interest list (EPOLL_CTL_DEL).
    pub fn ctl_del(&self, fd: i32) -> LxResult {
        let mut inner = self.inner.lock();
        match inner.interest.remove(&fd) {
            Some(_) => Ok(()),
            None => Err(LxError::ENOENT),
        }
    }

    /// Wait for events on the interest list.
    ///
    /// Returns a vector of ready events, up to `max_events` entries.
    /// `timeout_ms` semantics: -1 = block indefinitely, 0 = return immediately,
    /// >0 = block for at most that many milliseconds.
    pub async fn wait(&self, max_events: usize, timeout_ms: isize) -> LxResult<Vec<EpollEvent>> {
        use core::future::Future;
        use core::pin::Pin;
        use core::task::{Context, Poll};
        use core::time::Duration;
        use kernel_hal::timer;

        struct EpollWaitFuture<'a> {
            epoll: &'a EpollFile,
            max_events: usize,
            timeout_ms: isize,
            begin_time_ms: usize,
        }

        impl Future for EpollWaitFuture<'_> {
            type Output = LxResult<Vec<EpollEvent>>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let inner = self.epoll.inner.lock();
                let mut ready = Vec::new();

                for entry in inner.interest.values() {
                    if ready.len() >= self.max_events {
                        break;
                    }
                    let poll_events = epoll_to_poll_events(entry.event.events);
                    let mut fut = Box::pin(entry.file.async_poll(poll_events));
                    let status = match fut.as_mut().poll(cx) {
                        Poll::Ready(Ok(s)) => s,
                        Poll::Ready(Err(_)) => PollStatus {
                            error: true,
                            read: false,
                            write: false,
                        },
                        Poll::Pending => continue,
                    };

                    let mut revents = 0u32;
                    if status.read {
                        revents |= EPOLLIN;
                    }
                    if status.write {
                        revents |= EPOLLOUT;
                    }
                    if status.error {
                        revents |= EPOLLERR;
                    }
                    // Only report events the caller asked for (plus ERR/HUP)
                    let masked = revents & (entry.event.events | EPOLLERR | EPOLLHUP);
                    if masked != 0 {
                        ready.push(EpollEvent {
                            events: masked,
                            data: entry.event.data,
                        });
                    }
                }
                // Release the lock before potentially blocking
                drop(inner);

                if !ready.is_empty() {
                    return Poll::Ready(Ok(ready));
                }

                use crate::time::TimeVal;
                match self.timeout_ms {
                    0 => Poll::Ready(Ok(Vec::new())),
                    t if t > 0 => {
                        let current_ms = TimeVal::now().to_msec();
                        let deadline = self.begin_time_ms + t as usize;
                        if current_ms >= deadline {
                            Poll::Ready(Ok(Vec::new()))
                        } else {
                            let waker = cx.waker().clone();
                            timer::timer_set(
                                Duration::from_millis(deadline as u64),
                                Box::new(move |_| waker.wake_by_ref()),
                            );
                            Poll::Pending
                        }
                    }
                    _ => {
                        // -1: block indefinitely, re-poll every 500ms
                        let current_ms = TimeVal::now().to_msec();
                        let deadline = current_ms + 500;
                        let waker = cx.waker().clone();
                        timer::timer_set(
                            Duration::from_millis(deadline as u64),
                            Box::new(move |_| waker.wake_by_ref()),
                        );
                        Poll::Pending
                    }
                }
            }
        }

        use crate::time::TimeVal;
        let future = EpollWaitFuture {
            epoll: self,
            max_events,
            timeout_ms,
            begin_time_ms: TimeVal::now().to_msec(),
        };
        future.await
    }
}

/// Epoll event flag: available for read.
const EPOLLIN: u32 = 0x001;
/// Epoll event flag: available for write.
const EPOLLOUT: u32 = 0x004;
/// Epoll event flag: error condition.
const EPOLLERR: u32 = 0x008;
/// Epoll event flag: hang up.
const EPOLLHUP: u32 = 0x010;

/// Convert epoll event flags to PollEvents for the underlying poll call.
fn epoll_to_poll_events(epoll_events: u32) -> PollEvents {
    let mut pe = PollEvents::empty();
    if epoll_events & EPOLLIN != 0 {
        pe |= PollEvents::IN;
    }
    if epoll_events & EPOLLOUT != 0 {
        pe |= PollEvents::OUT;
    }
    if epoll_events & EPOLLERR != 0 {
        pe |= PollEvents::ERR;
    }
    if epoll_events & EPOLLHUP != 0 {
        pe |= PollEvents::HUP;
    }
    pe
}

#[async_trait]
impl FileLike for EpollFile {
    fn flags(&self) -> OpenFlags {
        self.inner.lock().flags
    }

    fn set_flags(&self, f: OpenFlags) -> LxResult {
        let mut inner = self.inner.lock();
        inner
            .flags
            .set(OpenFlags::CLOEXEC, f.contains(OpenFlags::CLOEXEC));
        Ok(())
    }

    fn dup(&self) -> LxResult<Arc<dyn FileLike>> {
        // Duplicating an epoll fd shares the underlying interest list.
        // For simplicity, create a new independent instance.
        let inner = self.inner.lock();
        let new = Arc::new(EpollFile {
            base: KObjectBase::new(),
            inner: Mutex::new(EpollInner {
                flags: inner.flags,
                interest: BTreeMap::new(),
            }),
        });
        Ok(new)
    }

    async fn read(&self, _buf: &mut [u8]) -> LxResult<usize> {
        Err(LxError::EINVAL)
    }

    fn write(&self, _buf: &[u8]) -> LxResult<usize> {
        Err(LxError::EINVAL)
    }

    async fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> LxResult<usize> {
        Err(LxError::EINVAL)
    }

    fn poll(&self, _events: PollEvents) -> LxResult<PollStatus> {
        // An epoll fd itself is not pollable in this implementation.
        Ok(PollStatus {
            read: false,
            write: false,
            error: false,
        })
    }

    async fn async_poll(&self, _events: PollEvents) -> LxResult<PollStatus> {
        Ok(PollStatus {
            read: false,
            write: false,
            error: false,
        })
    }
}

/// Get the number of ready events without blocking (for testing).
#[allow(dead_code)]
pub fn ready_count(epoll: &EpollFile) -> usize {
    let inner = epoll.inner.lock();
    let mut count = 0;
    for entry in inner.interest.values() {
        let poll_events = epoll_to_poll_events(entry.event.events);
        if let Ok(status) = entry.file.poll(poll_events) {
            let revents = (if status.read { EPOLLIN } else { 0 })
                | (if status.write { EPOLLOUT } else { 0 })
                | (if status.error { EPOLLERR } else { 0 });
            if revents & (entry.event.events | EPOLLERR | EPOLLHUP) != 0 {
                count += 1;
            }
        }
    }
    count
}
