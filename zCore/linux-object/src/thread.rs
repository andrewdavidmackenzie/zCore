//! Linux Thread

use crate::error::{LxError, SysResult};
use crate::process::ProcessExt;
use crate::signal::{SigInfo, Signal, SignalCode, SignalStack, SignalUserContext, Sigset};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
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
    fn get_robust_list(&self, head_ptr: UserOutPtr<usize>, len_ptr: UserOutPtr<usize>)
        -> SysResult;
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
            user_time_ns: 0,
            sys_time_ns: 0,
            siginfo_queue: VecDeque::new(),
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
        mut head_ptr: UserOutPtr<usize>,
        mut len_ptr: UserOutPtr<usize>,
    ) -> SysResult {
        let linux = self.lock_linux();
        head_ptr.write(linux.robust_list.as_addr())?;
        len_ptr.write(linux.robust_list_len)?;
        Ok(0)
    }

    fn set_robust_list(&self, head: UserInPtr<RobustList>, len: usize) {
        self.lock_linux().robust_list = head;
        self.lock_linux().robust_list_len = len;
    }
}

/// Process a single robust futex entry: if the futex word indicates
/// this thread owns it (TID matches), set FUTEX_OWNER_DIED and wake one waiter.
/// Process a single robust futex entry on thread exit.
///
/// If the futex word's TID matches `tid` (or `is_pending` and owner is 0),
/// atomically sets `FUTEX_OWNER_DIED` and clears the TID, preserving the
/// `FUTEX_WAITERS` bit. Then wakes one waiter.
#[allow(unsafe_code)]
fn handle_robust_entry(
    futex_addr: usize,
    tid: u32,
    proc: &zircon_object::task::Process,
    is_pending: bool,
) {
    use core::sync::atomic::{AtomicU32, Ordering};
    // Safety: futex_addr points into the calling thread's own address space.
    // The kernel accesses it atomically, matching Linux kernel behavior.
    let futex_ptr = futex_addr as *const AtomicU32;
    let futex_word = unsafe { &*futex_ptr };

    loop {
        let old = futex_word.load(Ordering::SeqCst);
        let owner = old & FUTEX_TID_MASK;
        // Process if: we own it, OR it's a pending unlock (owner cleared to 0)
        if owner != tid && !(is_pending && owner == 0) {
            return;
        }
        // New value: OWNER_DIED | preserve WAITERS bit | clear TID
        let new = (old & FUTEX_WAITERS) | FUTEX_OWNER_DIED;
        if futex_word
            .compare_exchange(old, new, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            break;
        }
    }
    // Wake one waiter
    let linux_proc = proc
        .ext()
        .downcast_ref::<crate::process::LinuxProcess>()
        .unwrap();
    let futex = linux_proc.get_futex(futex_addr);
    futex.wake(1);
}

/// Bit 0 in robust list pointers indicates a PI futex entry.
const ROBUST_LIST_PI_BIT: usize = 1;

/// Walk the robust futex list for a thread and set FUTEX_OWNER_DIED on any
/// futexes still held by it. Called during thread exit.
fn walk_robust_list(thread: &CurrentThread) {
    use zircon_object::object::KernelObject;
    let linux = thread.lock_linux();
    let robust_ptr = &linux.robust_list;
    if robust_ptr.is_null() {
        return;
    }
    let head = match robust_ptr.read() {
        Ok(h) => h,
        Err(_) => return,
    };
    let head_addr = robust_ptr.as_addr();
    let tid = thread.id() as u32;
    let proc = thread.proc();
    drop(linux); // release lock before touching futexes

    // Process list_op_pending first (in-progress lock/unlock).
    // Strip the PI flag (bit 0) from the pending pointer.
    let pending_addr = head.pending & !ROBUST_LIST_PI_BIT;
    if pending_addr != 0 {
        let futex_addr = (pending_addr as isize + head.off) as usize;
        handle_robust_entry(futex_addr, tid, proc, true);
    }

    // Walk the circular linked list.
    // Strip the PI flag (bit 0) from each link pointer.
    let mut entry = head.head & !ROBUST_LIST_PI_BIT;
    for _ in 0..ROBUST_LIST_LIMIT {
        if entry == head_addr {
            break; // full circle
        }
        if entry == 0 {
            break;
        }
        if entry != pending_addr {
            let futex_addr = (entry as isize + head.off) as usize;
            handle_robust_entry(futex_addr, tid, proc, false);
        }
        let next_ptr: UserInPtr<usize> = entry.into();
        entry = match next_ptr.read() {
            Ok(next) => next & !ROBUST_LIST_PI_BIT,
            Err(_) => break,
        };
    }
}

impl CurrentThreadExt for CurrentThread {
    /// Exit current thread for Linux.
    fn exit_linux(&self, _exit_code: i32) {
        // Walk the robust futex list and mark any held mutexes as OWNER_DIED.
        // This must happen before clear_child_tid (matches Linux kernel order).
        walk_robust_list(self);

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

/// Robust futex list head, matching Linux's `struct robust_list_head`.
#[repr(C)]
#[derive(Default)]
pub struct RobustList {
    /// Pointer to the first entry in the circular linked list
    pub head: usize,
    /// Byte offset from a robust_list entry to the futex word
    pub off: isize,
    /// Entry currently being locked/unlocked (in-progress operation)
    pub pending: usize,
}

/// Bit set in the futex word when the owning thread dies.
const FUTEX_OWNER_DIED: u32 = 0x40000000;
/// Bit set when there are waiters on the futex.
const FUTEX_WAITERS: u32 = 0x80000000;
/// Mask for the thread ID portion of a futex word.
const FUTEX_TID_MASK: u32 = 0x3FFFFFFF;
/// Maximum number of entries to walk (prevents infinite loops).
const ROBUST_LIST_LIMIT: usize = 2048;

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
    /// Accumulated user-mode CPU time (nanoseconds).
    user_time_ns: u128,
    /// Accumulated system/kernel-mode CPU time (nanoseconds).
    sys_time_ns: u128,
    /// Per-signal siginfo payloads. Standard signals coalesce (at most
    /// one entry per signal number); real-time signals may queue.
    siginfo_queue: VecDeque<SigInfo>,
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

    /// Handle signal -- returns the signal, its siginfo, and the current mask.
    pub fn handle_signal(&mut self) -> Option<(Signal, SigInfo, Sigset)> {
        if self.handling_signal.is_none() {
            let signal = self
                .signals
                .mask_with(&self.signal_mask)
                .find_first_signal();
            if let Some(signal) = signal {
                self.handling_signal = Some(signal as u32);
                self.signals.remove(signal);
                let info = self.pop_siginfo(signal).unwrap_or(SigInfo {
                    signo: signal as i32,
                    ..Default::default()
                });
                return Some((signal, info, self.signal_mask));
            }
        }
        None
    }

    /// Insert a signal into the pending set and wake any blocked Future.
    ///
    /// Stores a default `SigInfo` for the signal. For signals with
    /// payload (rt_sigqueueinfo), use `insert_signal_info` instead.
    pub fn insert_signal(&mut self, sig: Signal) {
        let info = SigInfo {
            signo: sig as i32,
            code: SignalCode::KERNEL,
            ..Default::default()
        };
        self.insert_signal_info(sig, info);
    }

    /// Insert a signal with a specific `SigInfo` payload.
    ///
    /// Standard signals (1-31) coalesce: if the signal is already
    /// pending, the new siginfo replaces the old one. Real-time
    /// signals (32-64) queue individually.
    pub fn insert_signal_info(&mut self, sig: Signal, info: SigInfo) {
        let is_rt = (sig as u32) >= 32;
        if is_rt || !self.signals.contains(sig) {
            // Queue the siginfo (RT signals always queue; standard
            // signals only queue if not already pending).
            self.siginfo_queue.push_back(info);
        }
        self.signals.insert(sig);
        if !self.signal_mask.contains(sig) {
            if let Some(waker) = self.signal_waker.as_ref() {
                waker.wake_by_ref();
            }
        }
    }

    /// Pop the siginfo for a specific signal from the queue.
    ///
    /// Returns `None` if no siginfo was stored (shouldn't happen in
    /// normal usage since `insert_signal` always stores one).
    pub fn pop_siginfo(&mut self, sig: Signal) -> Option<SigInfo> {
        let signo = sig as i32;
        if let Some(pos) = self.siginfo_queue.iter().position(|i| i.signo == signo) {
            self.siginfo_queue.remove(pos)
        } else {
            None
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

    /// Add user-mode CPU time (nanoseconds).
    pub fn add_user_time(&mut self, ns: u128) {
        self.user_time_ns += ns;
    }

    /// Add system/kernel-mode CPU time (nanoseconds).
    pub fn add_sys_time(&mut self, ns: u128) {
        self.sys_time_ns += ns;
    }

    /// Get accumulated user-mode CPU time (nanoseconds).
    pub fn user_time_ns(&self) -> u128 {
        self.user_time_ns
    }

    /// Get accumulated system-mode CPU time (nanoseconds).
    pub fn sys_time_ns(&self) -> u128 {
        self.sys_time_ns
    }
}

/// A future wrapper that makes any inner future interruptible by signals.
///
/// When polled, it:
/// 1. Checks for pending unmasked signals → returns `Err(EINTR)` immediately
/// 2. Registers the thread's `signal_waker` so `insert_signal()` can wake us
/// 3. Polls the inner future
/// 4. Clears the signal waker on completion
///
/// This enables blocking syscalls to return `EINTR` when a signal is
/// delivered to the thread.
pub struct InterruptibleFuture<'a, F> {
    inner: F,
    thread: &'a Thread,
}

impl<'a, F: Future + Unpin> Future for InterruptibleFuture<'a, F> {
    type Output = Result<F::Output, LxError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Atomically check for pending signals and register the waker.
        // Both must happen under one lock to prevent a race where a
        // signal arrives between the check and the waker registration.
        {
            let mut linux = self.thread.lock_linux();
            if linux.has_pending_signal() {
                linux.clear_signal_waker();
                return Poll::Ready(Err(LxError::EINTR));
            }
            linux.set_signal_waker(cx.waker().clone());
        }
        // Poll the inner future
        match Pin::new(&mut self.inner).poll(cx) {
            Poll::Ready(val) => {
                let mut linux = self.thread.lock_linux();
                linux.clear_signal_waker();
                Poll::Ready(Ok(val))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<F> Drop for InterruptibleFuture<'_, F> {
    fn drop(&mut self) {
        let mut linux = self.thread.lock_linux();
        linux.clear_signal_waker();
    }
}

/// Extension trait to make any future interruptible by signals.
pub trait Interruptible: Sized {
    /// Wrap this future to return `Err(EINTR)` when a signal is delivered.
    fn interruptible(self, thread: &Thread) -> InterruptibleFuture<'_, Self>;
}

impl<F: Future + Unpin> Interruptible for F {
    fn interruptible(self, thread: &Thread) -> InterruptibleFuture<'_, Self> {
        InterruptibleFuture {
            inner: self,
            thread,
        }
    }
}
