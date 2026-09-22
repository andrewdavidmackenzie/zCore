//! Implement INode for Stdin & Stdout

use super::ioctl::*;
use crate::{sync::Event, sync::EventBus};
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::any::Any;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use hal_impl::console::{self, ConsoleWinSize};
use lock::Mutex;
use rcore_fs::vfs::*;

/// STDIN global reference.
///
/// Reads from the shared console input buffer (`ConsoleInput`), which
/// is fed by whatever input devices the platform provides (UART, PS/2
/// keyboard, etc.).
pub static STDIN: spin::Lazy<Arc<Stdin>> = spin::Lazy::new(|| Arc::new(Stdin::default()));
/// STDOUT global reference
pub static STDOUT: spin::Lazy<Arc<Stdout>> = spin::Lazy::new(Default::default);

/// Stdin struct, for Stdin buffer
#[derive(Default)]
pub struct Stdin {
    buf: Mutex<VecDeque<char>>,
    eventbus: Mutex<EventBus>,
}

impl Stdin {
    /// push a char in Stdin buffer
    pub fn push(&self, c: char) {
        self.buf.lock().push_back(c);
        self.eventbus.lock().set(Event::READABLE);
    }
    /// pop a char in Stdin buffer
    pub fn pop(&self) -> char {
        let mut buf_lock = self.buf.lock();
        let c = buf_lock.pop_front().unwrap();
        if buf_lock.is_empty() {
            self.eventbus.lock().clear(Event::READABLE);
        }
        c
    }
    /// specify whether the Stdin buffer is readable
    pub fn can_read(&self) -> bool {
        !self.buf.lock().is_empty()
    }
}

/// Stdout struct, empty now
#[derive(Default)]
pub struct Stdout;

impl INode for Stdin {
    fn read_at(&self, _offset: usize, buf: &mut [u8]) -> Result<usize> {
        // Try the internal buffer first (for any previously pushed chars).
        if self.can_read() {
            buf[0] = self.pop() as u8;
            return Ok(1);
        }
        // Try the shared console input buffer (no waker for sync read).
        let n = hal_impl::console::console_input_poll(buf, None);
        if n > 0 {
            Ok(n)
        } else {
            Err(FsError::Again)
        }
    }
    fn write_at(&self, _offset: usize, _buf: &[u8]) -> Result<usize> {
        Err(FsError::NotSupported)
    }
    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus {
            read: self.can_read(),
            write: false,
            error: false,
        })
    }
    fn async_poll<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<PollStatus>> + Send + Sync + 'a>> {
        #[must_use = "future does nothing unless polled/`await`-ed"]
        struct SerialFuture<'a> {
            stdin: &'a Stdin,
        }

        impl<'a> Future for SerialFuture<'a> {
            type Output = Result<PollStatus>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                if self.stdin.can_read() {
                    return Poll::Ready(self.stdin.poll());
                }
                // Atomically check for data and register waker if empty.
                let mut probe = [0u8; 1];
                let n = hal_impl::console::console_input_poll(&mut probe, Some(cx.waker().clone()));
                if n > 0 {
                    self.stdin.push(probe[0] as char);
                    return Poll::Ready(self.stdin.poll());
                }
                Poll::Pending
            }
        }

        Box::pin(SerialFuture { stdin: self })
    }

    //
    fn io_control(&self, cmd: u32, data: usize) -> Result<usize> {
        match cmd as usize {
            TIOCGWINSZ => {
                let winsize = data as *mut ConsoleWinSize;
                unsafe { *winsize = console::console_win_size() };
                Ok(0)
            }
            TCGETS | TIOCSPGRP => {
                trace!("stdin TCGETS | TIOCSPGRP, pretend to be tty.");
                // pretend to be tty
                Ok(0)
            }
            TIOCGPGRP => {
                // pretend to have a tty process group
                if data == 0 {
                    return Err(FsError::InvalidParam);
                }
                unsafe { *(data as *mut u32) = 0 };
                Ok(0)
            }
            _ => Err(FsError::NotSupported),
        }
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}

impl INode for Stdout {
    fn read_at(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize> {
        Err(FsError::NotSupported)
    }
    fn write_at(&self, _offset: usize, buf: &[u8]) -> Result<usize> {
        // we do not care the utf-8 things, we just want to print it!
        let s = unsafe { core::str::from_utf8_unchecked(buf) };
        hal_impl::console::console_write_str(s);
        Ok(buf.len())
    }
    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus {
            read: false,
            write: true,
            error: false,
        })
    }
    fn io_control(&self, cmd: u32, data: usize) -> Result<usize> {
        match cmd as usize {
            TIOCGWINSZ => {
                let winsize = data as *mut ConsoleWinSize;
                unsafe { *winsize = console::console_win_size() };
                Ok(0)
            }
            TCGETS | TIOCSPGRP => {
                trace!("stdout TCGETS | TIOCSPGRP, pretend to be tty.");
                // pretend to be tty
                Ok(0)
            }
            TIOCGPGRP => {
                // pretend to have a tty process group
                if data == 0 {
                    return Err(FsError::InvalidParam);
                }
                unsafe { *(data as *mut u32) = 0 };
                Ok(0)
            }
            _ => Err(FsError::NotSupported),
        }
    }

    /// Get metadata of the INode
    fn metadata(&self) -> Result<Metadata> {
        Ok(Metadata {
            dev: 1,
            inode: 13,
            size: 0,
            blk_size: 0,
            blocks: 0,
            atime: Timespec { sec: 0, nsec: 0 },
            mtime: Timespec { sec: 0, nsec: 0 },
            ctime: Timespec { sec: 0, nsec: 0 },
            type_: FileType::CharDevice,
            mode: 0o666,
            nlinks: 1,
            uid: 0,
            gid: 0,
            rdev: make_rdev(5, 0),
        })
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}
