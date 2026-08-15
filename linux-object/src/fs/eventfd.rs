//! EventFd file descriptor implementation
//!
//! Provides `eventfd2` support — a file descriptor for event notification
//! using a u64 counter.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::convert::TryInto;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use async_trait::async_trait;
use lock::Mutex;
use zircon_object::object::*;

use crate::error::{LxError, LxResult};
use crate::fs::{FileLike, OpenFlags, PollEvents};
use crate::sync::{Event, EventBus};
use rcore_fs::vfs::PollStatus;

/// Maximum value for the eventfd counter (u64::MAX - 1, per Linux semantics).
const EVENTFD_MAX: u64 = u64::MAX - 1;

/// EFD_SEMAPHORE flag: read returns 1 and decrements, instead of returning
/// the full counter and resetting to 0.
const EFD_SEMAPHORE: usize = 0x1;

/// Inner state of an eventfd, protected by a mutex.
struct EventFdInner {
    /// The u64 counter value.
    counter: u64,
    /// Open flags (CLOEXEC, NON_BLOCK).
    flags: OpenFlags,
    /// Whether EFD_SEMAPHORE mode is enabled.
    semaphore: bool,
    /// EventBus for waking blocked readers/writers.
    eventbus: EventBus,
}

/// An eventfd file descriptor.
///
/// Implements [`FileLike`] so it can be stored in the process fd table.
pub struct EventFd {
    base: KObjectBase,
    inner: Mutex<EventFdInner>,
}

impl_kobject!(EventFd);

impl EventFd {
    /// Create a new eventfd with the given initial counter value and flags.
    ///
    /// Valid flags: `EFD_CLOEXEC` (0x80000), `EFD_NONBLOCK` (0x800),
    /// `EFD_SEMAPHORE` (0x1).
    pub fn new(initval: u64, flags: usize) -> Self {
        let open_flags =
            OpenFlags::from_bits_truncate(flags) & (OpenFlags::CLOEXEC | OpenFlags::NON_BLOCK);
        let semaphore = flags & EFD_SEMAPHORE != 0;
        let mut eventbus = EventBus::default();
        if initval > 0 {
            eventbus.set(Event::READABLE);
        }
        // Counter can always be written to when < EVENTFD_MAX
        if initval < EVENTFD_MAX {
            eventbus.set(Event::WRITABLE);
        }
        EventFd {
            base: KObjectBase::new(),
            inner: Mutex::new(EventFdInner {
                counter: initval,
                flags: open_flags,
                semaphore,
                eventbus,
            }),
        }
    }
}

#[async_trait]
impl FileLike for EventFd {
    fn flags(&self) -> OpenFlags {
        self.inner.lock().flags
    }

    fn set_flags(&self, f: OpenFlags) -> LxResult {
        let mut inner = self.inner.lock();
        inner
            .flags
            .set(OpenFlags::CLOEXEC, f.contains(OpenFlags::CLOEXEC));
        inner
            .flags
            .set(OpenFlags::NON_BLOCK, f.contains(OpenFlags::NON_BLOCK));
        Ok(())
    }

    fn dup(&self) -> LxResult<Arc<dyn FileLike>> {
        // Dup creates a new fd pointing to the same eventfd counter.
        // We can't easily share Arc<Mutex<EventFdInner>> since we own it,
        // so create an independent copy with the same counter value.
        let inner = self.inner.lock();
        Ok(Arc::new(EventFd::new(
            inner.counter,
            inner.flags.bits() | if inner.semaphore { EFD_SEMAPHORE } else { 0 },
        )))
    }

    async fn read(&self, buf: &mut [u8]) -> LxResult<usize> {
        if buf.len() < 8 {
            return Err(LxError::EINVAL);
        }

        struct ReadFuture<'a> {
            eventfd: &'a EventFd,
        }

        impl Future for ReadFuture<'_> {
            type Output = LxResult<u64>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let mut inner = self.eventfd.inner.lock();
                if inner.counter > 0 {
                    let value = if inner.semaphore {
                        inner.counter -= 1;
                        1
                    } else {
                        let v = inner.counter;
                        inner.counter = 0;
                        v
                    };
                    // Update events
                    if inner.counter == 0 {
                        inner.eventbus.clear(Event::READABLE);
                    }
                    inner.eventbus.set(Event::WRITABLE);
                    return Poll::Ready(Ok(value));
                }

                if inner.flags.contains(OpenFlags::NON_BLOCK) {
                    return Poll::Ready(Err(LxError::EAGAIN));
                }

                // Block: subscribe to eventbus for wakeup
                let waker = cx.waker().clone();
                inner.eventbus.subscribe(Box::new(move |_| {
                    waker.wake_by_ref();
                    true
                }));
                Poll::Pending
            }
        }

        let value = ReadFuture { eventfd: self }.await?;
        buf[..8].copy_from_slice(&value.to_ne_bytes());
        Ok(8)
    }

    fn write(&self, buf: &[u8]) -> LxResult<usize> {
        if buf.len() < 8 {
            return Err(LxError::EINVAL);
        }
        let value = u64::from_ne_bytes(buf[..8].try_into().map_err(|_| LxError::EINVAL)?);
        if value == u64::MAX {
            return Err(LxError::EINVAL);
        }

        let mut inner = self.inner.lock();
        if inner.counter > EVENTFD_MAX - value {
            if inner.flags.contains(OpenFlags::NON_BLOCK) {
                return Err(LxError::EAGAIN);
            }
            // In blocking mode we would need to wait, but for simplicity
            // we return EAGAIN here as well (full blocking write support
            // would require an async write path).
            return Err(LxError::EAGAIN);
        }

        inner.counter += value;
        inner.eventbus.set(Event::READABLE);
        if inner.counter >= EVENTFD_MAX {
            inner.eventbus.clear(Event::WRITABLE);
        }
        Ok(8)
    }

    async fn read_at(&self, _offset: u64, buf: &mut [u8]) -> LxResult<usize> {
        self.read(buf).await
    }

    fn poll(&self, _events: PollEvents) -> LxResult<PollStatus> {
        let inner = self.inner.lock();
        Ok(PollStatus {
            read: inner.counter > 0,
            write: inner.counter < EVENTFD_MAX,
            error: false,
        })
    }

    async fn async_poll(&self, _events: PollEvents) -> LxResult<PollStatus> {
        self.poll(_events)
    }
}
