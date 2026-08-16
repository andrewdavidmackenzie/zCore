//! Linux Thread

use crate::error::SysResult;
use crate::process::ProcessExt;
use crate::signal::{SigInfo, Signal, SignalStack, SignalUserContext, Sigset};
use alloc::sync::Arc;
use core::task::Waker;
use kernel_hal::context::{UserContext, UserContextField};
use kernel_hal::user::{Out, UserInPtr, UserOutPtr, UserPtr};
use lock::{Mutex, MutexGuard};
use zircon_object::task::{CurrentThread, Process, Thread};
use zircon_object::ZxResult;

/// Thread extension for linux
pub trait ThreadExt {
    /// create linux thread
    fn create_linux(proc: &Arc<Process>) -> ZxResult<Arc<Self>>;
    /// lock and get Linux thread
    fn lock_linux(&self) -> MutexGuard<'_, LinuxThread>;
    /// Set pointer to thread ID.
    fn set_tid_address(&self, tidptr: UserOutPtr<i32>);
    /// Get robust list.
    fn get_robust_list(
        &self,
        _head_ptr: UserOutPtr<UserOutPtr<RobustList>>,
        _len_ptr: UserOutPtr<usize>,
    ) -> SysResult;
    /// Set robust list.
    fn set_robust_list(&self, head: UserInPtr<RobustList>, len: usize);
}

/// CurrentThread extension for linux
pub trait CurrentThreadExt {
    /// exit linux thread
    fn exit_linux(&self, exit_code: i32);
}

impl ThreadExt for Thread {
    fn create_linux(proc: &Arc<Process>) -> ZxResult<Arc<Self>> {
        let linux_thread = Mutex::new(LinuxThread {
            clear_child_tid: 0.into(),
            signals: Sigset::default(),
            signal_mask: Sigset::default(),
            signal_alternate_stack: SignalStack::default(),
            robust_list: 0.into(),
            robust_list_len: 0,
            handling_signal: None,
            signal_waker: None,
        });
        Thread::create_with_ext(proc, "", linux_thread)
    }

    fn lock_linux(&self) -> MutexGuard<'_, LinuxThread> {
        self.ext()
            .downcast_ref::<Mutex<LinuxThread>>()
            .unwrap()
            .lock()
    }

    /// Set pointer to thread ID.
    fn set_tid_address(&self, tidptr: UserPtr<i32, Out>) {
        self.lock_linux().clear_child_tid = tidptr;
    }

    fn get_robust_list(
        &self,
        mut _head_ptr: UserOutPtr<UserOutPtr<RobustList>>,
        mut _len_ptr: UserOutPtr<usize>,
    ) -> SysResult {
        _head_ptr = (self.lock_linux().robust_list.as_addr() as *mut RobustList as usize).into();
        _len_ptr = (&self.lock_linux().robust_list_len as *const usize as usize).into();
        Ok(0)
    }

    fn set_robust_list(&self, head: UserInPtr<RobustList>, len: usize) {
        self.lock_linux().robust_list = head;
        self.lock_linux().robust_list_len = len;
    }
}

impl CurrentThreadExt for CurrentThread {
    /// Exit current thread for Linux.
    fn exit_linux(&self, _exit_code: i32) {
        let mut linux_thread = self.lock_linux();
        let clear_child_tid = &mut linux_thread.clear_child_tid;
        // perform futex wake 1
        // ref: http://man7.org/linux/man-pages/man2/set_tid_address.2.html
        if !clear_child_tid.is_null() {
            info!("exit: do futex {:?} wake 1", clear_child_tid);
            #[cfg(target_os = "none")]
            {
                let vaddr = clear_child_tid.as_addr();
                let vmar = self.proc().vmar();
                if vmar.contains(vaddr) {
                    let mut is_handle_write_pagefault = true;

                    match vmar.get_vaddr_flags(vaddr) {
                        Ok(vaddr_flags) => {
                            is_handle_write_pagefault &=
                                !vaddr_flags.contains(kernel_hal::MMUFlags::WRITE);
                        }
                        Err(kernel_hal::vm::PagingError::NotMapped) => {
                            is_handle_write_pagefault &= true;
                        }
                        Err(kernel_hal::vm::PagingError::NoMemory) => {
                            is_handle_write_pagefault &= true;
                        }
                        Err(kernel_hal::vm::PagingError::AlreadyMapped) => {
                            is_handle_write_pagefault &= true;
                        }
                    }

                    if !is_handle_write_pagefault {
                        clear_child_tid.write(0).unwrap();
                        let uaddr = clear_child_tid.as_addr();
                        let futex = self.proc().linux().get_futex(uaddr);
                        futex.wake(1);
                    }
                }
            }
            #[cfg(not(target_os = "none"))]
            {
                clear_child_tid.write(0).unwrap();
                let uaddr = clear_child_tid.as_addr();
                let futex = self.proc().linux().get_futex(uaddr);
                futex.wake(1);
            }
        }
        self.exit();
    }
}

/// robust_list
#[derive(Default)]
pub struct RobustList {
    /// head
    pub head: usize,
    /// off
    pub off: isize,
    /// pending
    pub pending: usize,
}

/// Linux specific thread information.
pub struct LinuxThread {
    /// Kernel performs futex wake when thread exits.
    /// Ref: <http://man7.org/linux/man-pages/man2/set_tid_address.2.html>
    clear_child_tid: UserOutPtr<i32>,
    /// Linux signals
    pub signals: Sigset,
    /// Signal mask
    pub signal_mask: Sigset,
    /// signal alternate stack
    pub signal_alternate_stack: SignalStack,
    /// robust_list
    robust_list: UserInPtr<RobustList>,
    robust_list_len: usize,
    /// handling signals
    pub handling_signal: Option<u32>,
    /// Waker for the currently-blocked Future (if any).
    /// Used to wake a thread when a signal is delivered, enabling
    /// EINTR returns from blocking syscalls.
    signal_waker: Option<Waker>,
}

fn unmodified_check(siginfo: &SigInfo, user_ctx: &SignalUserContext) -> usize {
    let mut check = 0usize;
    let default_info = SigInfo::default();
    let mut default_ctx = SignalUserContext::default();
    default_ctx.context.set_pc(user_ctx.context.get_pc());
    check |= (*siginfo != default_info) as usize;
    check |= ((user_ctx.flags != default_ctx.flags) as usize) << 1;
    check |= ((user_ctx.link != default_ctx.link) as usize) << 2;
    check |= ((user_ctx.stack != default_ctx.stack) as usize) << 3;
    check |= ((user_ctx._pad != default_ctx._pad) as usize) << 4;
    check |= ((user_ctx.context != default_ctx.context) as usize) << 5;
    #[cfg(target_arch = "x86_64")]
    {
        check |= ((user_ctx.fpregs_mem != default_ctx.fpregs_mem) as usize) << 6;
    }
    check
}

#[allow(unsafe_code)]
impl LinuxThread {
    /// Restore the information after the signal handler returns
    pub fn restore_after_handle_signal(
        &mut self,
        ctx: &mut UserContext,
        old_ctx: &UserContext,
        siginfo_ptr: usize,
        uctx_ptr: usize,
    ) {
        let siginfo = unsafe { &*(siginfo_ptr as *const SigInfo) };
        let user_ctx = unsafe { &*(uctx_ptr as *const SignalUserContext) };
        let check = unmodified_check(siginfo, user_ctx);
        if check != 0 {
            warn!(
                "signal handler modified context fields (mask={:#b}), restoring saved context",
                check
            );
            trace!("uctx = {:x?}", *user_ctx);
        }
        *ctx = *old_ctx;
        ctx.set_field(UserContextField::InstrPointer, user_ctx.context.get_pc());
        self.signal_mask = Sigset::new(user_ctx.sig_mask.val());
        self.handling_signal = None;
    }

    /// Get signal info
    pub fn get_signal_info(&self) -> (Sigset, Sigset, Option<u32>) {
        (self.signals, self.signal_mask, self.handling_signal)
    }

    /// Handle signal
    pub fn handle_signal(&mut self) -> Option<(Signal, Sigset)> {
        if self.handling_signal.is_none() {
            let signal = self
                .signals
                .mask_with(&self.signal_mask)
                .find_first_signal();
            if let Some(signal) = signal {
                self.handling_signal = Some(signal as u32);
                self.signals.remove(signal);
                return Some((signal, self.signal_mask));
            }
        }
        None
    }

    /// Insert a signal into the pending set and wake any blocked Future.
    ///
    /// This replaces direct `signals.insert()` calls. When an unmasked
    /// signal is inserted, it wakes the thread's signal_waker (if any),
    /// causing blocking syscalls to return EINTR.
    ///
    /// Callers should also call `proc.signal_set(Signal::SIGCHLD)` on
    /// the target process to wake any `wait_signal` futures (e.g. in
    /// wait4/waitpid).
    pub fn insert_signal(&mut self, sig: Signal) {
        self.signals.insert(sig);
        if !self.signal_mask.contains(sig) {
            if let Some(waker) = self.signal_waker.as_ref() {
                waker.wake_by_ref();
            }
        }
    }

    /// Register a waker that will be called when a signal is delivered.
    /// Used by interruptible blocking syscalls.
    pub fn set_signal_waker(&mut self, waker: Waker) {
        self.signal_waker = Some(waker);
    }

    /// Clear the signal waker (called when a blocking syscall returns).
    pub fn clear_signal_waker(&mut self) {
        self.signal_waker = None;
    }

    /// Check if any unmasked signal is pending.
    pub fn has_pending_signal(&self) -> bool {
        self.signals.mask_with(&self.signal_mask).is_not_empty()
    }
}
