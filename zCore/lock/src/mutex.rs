//! Kernel mutex: adaptive spin-then-yield mutex.
//!
//! This is the standard mutex for kernel object state that is only
//! accessed from task context. It matches Fuchsia's `Mutex` /
//! `CriticalMutex` semantics:
//!
//! 1. **Fast path**: Uncontended CAS acquire (single atomic).
//! 2. **Spin phase**: Spin for ~150µs with `hint::spin_loop()`.
//! 3. **Yield phase**: Call the registered yield function to let the
//!    lock holder run, then retry.
//!
//! Unlike [`InterruptSpinLock`](crate::Mutex), this mutex does NOT
//! disable interrupts — the holder may be preempted mid-critical-
//! section (the executor resumes it via the weak-executor mechanism)
//! and waiters yield instead of burning CPU. Interrupt handlers must
//! NOT acquire a `KernelMutex` (see the lock-ordering work in #829).
//!
//! # Yield function
//!
//! Call [`set_yield_fn`] during kernel init to register the platform's
//! yield/context-switch function. Without it, the mutex degrades to a
//! pure spinlock (no yielding).

use core::cell::UnsafeCell;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

// ── Yield function registration ─────────────────────────────────────

/// Stored yield function pointer. 0 = not set (pure spin).
static YIELD_FN: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Register the platform's yield function.
///
/// This should be called once during kernel initialization, before
/// any `KernelMutex` contention can occur. The function should
/// perform an executor-level context switch (e.g., `sched_yield()`).
///
/// # Safety
///
/// `f` must be a valid function pointer that remains valid for the
/// lifetime of the kernel.
pub fn set_yield_fn(f: fn()) {
    YIELD_FN.store(f as usize, Ordering::Release);
}

/// Call the registered yield function, or spin_loop if none registered.
#[inline]
fn do_yield() {
    // #862: never yield while an interrupt-disabling critical section
    // is active on this CPU (i.e., an `InterruptSpinLock` — including
    // the kernel heap lock — is held). The registered yield function
    // switches executor tasks; doing that here abandons the current
    // task mid-critical-section with the lock guard live in its
    // future, permanently leaking the lock. Every later acquisition
    // on any CPU then spins forever — the kernel-heap deadlock behind
    // the intermittent core-tests hangs (six CPUs spinning on the heap
    // lock held by a parked task, caught via gdb).
    #[cfg(target_os = "none")]
    {
        if !crate::interrupt::intr_get() {
            // In a critical section: spinning is safe (the holder is
            // either on another CPU or will be scheduled once this
            // task releases the spinlock it holds).
            core::hint::spin_loop();
            return;
        }
    }
    let ptr = YIELD_FN.load(Ordering::Acquire);
    if ptr != 0 {
        let f: fn() = unsafe { core::mem::transmute(ptr) };
        f();
    } else {
        // No yield function registered — just hint the CPU.
        core::hint::spin_loop();
    }
}

// ── KernelMutex ─────────────────────────────────────────────────────

/// Maximum spin iterations before yielding.
/// At ~1ns per iteration on a multi-GHz CPU, this is ~150µs — matching
/// Fuchsia's `SPIN_MAX_DURATION = ZX_USEC(150)`.
const SPIN_MAX_ITERS: u32 = 150_000;

/// Adaptive kernel mutex: CAS → spin → yield.
///
/// Does NOT disable interrupts. For interrupt-safe locks, use
/// [`InterruptSpinLock`](crate::Mutex).
pub struct KernelMutex<T: ?Sized> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for KernelMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for KernelMutex<T> {}

impl<T> KernelMutex<T> {
    /// Create a new unlocked mutex.
    pub const fn new(val: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(val),
        }
    }

    /// Consume the mutex and return the inner value.
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> KernelMutex<T> {
    /// Acquire the mutex, blocking if necessary.
    ///
    /// 1. Fast CAS attempt for uncontended case.
    /// 2. Bounded spin (~150µs) with `hint::spin_loop()`.
    /// 3. Yield to let the lock holder run, then retry.
    ///
    /// Does NOT disable interrupts.
    #[cfg_attr(target_os = "none", track_caller)]
    pub fn lock(&self) -> KernelMutexGuard<'_, T> {
        // #862/#829: acquiring a KernelMutex while holding an
        // interrupt-disabling spinlock is a lock-ordering violation.
        // The uncontended fast path below is benign (a single CAS, no
        // yield), and such nesting exists in common kernel paths — but
        // once CONTENDED with interrupts disabled we cannot yield
        // (that would abandon the critical section we hold) and cannot
        // spin forever if the holder is parked on this CPU. Spin a
        // bounded time (the holder may be on another CPU and release),
        // then fail loudly instead of deadlocking silently.
        #[cfg(target_os = "none")]
        const NESTED_SPIN_MAX_ITERS: u32 = 1_500_000;

        loop {
            // Fast path: try to acquire immediately.
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return KernelMutexGuard { mutex: self };
            }

            #[cfg(target_os = "none")]
            {
                // Contended with interrupts disabled. Two very
                // different situations share this state (#862/#523):
                //
                // 1. Task-context kernel code running inside a trap
                //    frame: the hardware interrupt gate cleared IF on
                //    trap entry (every IDT entry is an interrupt
                //    gate) and nothing re-enables it until the iretq.
                //    No IrqLock is held — enabling interrupts and
                //    yielding is safe: the coroutine switch suspends
                //    the whole trap frame and resumes it later
                //    (exactly like timer preemption), and the final
                //    iretq restores the interrupted rflags. Before
                //    this case was handled, every contended
                //    KernelMutex in it panicked — the multi-CPU panic
                //    cascade during process-crash teardown.
                //
                //    Distinguished by the per-CPU IrqLock depth
                //    (TSS.rsp2): zero here means no critical section
                //    is held on this CPU.
                //
                // 2. An IrqLock critical section is held by THIS
                //    call chain (spinlock → KernelMutex nesting).
                //    Yielding would abandon the critical section, so
                //    retry the acquisition in a bounded spin — the
                //    KernelMutex holder is typically on ANOTHER CPU
                //    and releases within microseconds (this nesting
                //    is legitimate and common: e.g. the futex
                //    priority-inheritance path holds the FutexInner
                //    IrqLock across boost_priority's ThreadInner
                //    KernelMutex). Only if the lock cannot be
                //    acquired at all — a holder parked on THIS CPU,
                //    which can never be scheduled while we spin —
                //    fail loudly.
                //
                //    NOTE: do NOT wait for the depth to clear here —
                //    the IrqLock holder is the current call chain
                //    itself; the depth can never reach zero while we
                //    spin, and waiting on it turns every legitimate
                //    cross-CPU contention into a guaranteed panic
                //    (the core-tests C11MutexTest hang: futex_wait's
                //    PI boost under FutexInner, #523).
                if !crate::interrupt::intr_get() && !crate::interrupt::irq_lock_held_on_this_cpu() {
                    // Gate context (case 1): take the normal
                    // spin/yield path below with interrupts
                    // re-enabled.
                    crate::interrupt::intr_on();
                } else if !crate::interrupt::intr_get() {
                    // Case 2: bounded retry of the acquisition.
                    let mut nested_spins = 0u32;
                    loop {
                        if self
                            .locked
                            .compare_exchange_weak(
                                false,
                                true,
                                Ordering::Acquire,
                                Ordering::Relaxed,
                            )
                            .is_ok()
                        {
                            return KernelMutexGuard { mutex: self };
                        }
                        nested_spins += 1;
                        if nested_spins >= NESTED_SPIN_MAX_ITERS {
                            // #523 diagnostics: report WHERE the contended
                            // acquisition comes from. The mutex address and
                            // the caller identify which lock; the raw stack
                            // words above rsp are mostly return addresses
                            // for offline symbolization against the kernel
                            // ELF (x86_64 — reading sp is arch-specific).
                            let mutex_addr = core::ptr::addr_of!(self.locked) as usize;
                            let caller = core::panic::Location::caller();
                            #[cfg(target_arch = "x86_64")]
                            let stack = {
                                let rsp: usize;
                                unsafe {
                                    core::arch::asm!(
                                        "mov {}, rsp",
                                        out(reg) rsp,
                                        options(nomem, nostack, preserves_flags)
                                    )
                                };
                                unsafe { core::slice::from_raw_parts(rsp as *const usize, 40) }
                            };
                            #[cfg(not(target_arch = "x86_64"))]
                            let stack: &[usize] = &[];
                            panic!(
                                "KernelMutex @{:#x} contended while interrupts disabled \
                                 and IrqLock(s) held — spinlock→KernelMutex nesting with \
                                 a non-releasing holder (#829/#862) \
                                 [caller {}:{}:{}] stack={:#x?}",
                                mutex_addr,
                                caller.file(),
                                caller.line(),
                                caller.column(),
                                stack,
                            );
                        }
                        core::hint::spin_loop();
                    }
                }
            }

            // Spin phase: spin for a bounded time.
            let mut spins = 0u32;
            loop {
                if !self.locked.load(Ordering::Relaxed) {
                    break; // Lock appears free — retry CAS.
                }
                core::hint::spin_loop();
                spins += 1;
                if spins >= SPIN_MAX_ITERS {
                    do_yield();
                    break;
                }
            }
        }
    }

    /// Try to acquire the mutex without blocking.
    pub fn try_lock(&self) -> Option<KernelMutexGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(KernelMutexGuard { mutex: self })
        } else {
            None
        }
    }

    /// Returns a mutable reference to the underlying data.
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }

    /// Returns whether the mutex is currently locked.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }
}

// ── Guard ───────────────────────────────────────────────────────────

/// RAII guard that releases the mutex on drop.
pub struct KernelMutexGuard<'a, T: ?Sized> {
    mutex: &'a KernelMutex<T>,
}

impl<T: ?Sized> Deref for KernelMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T: ?Sized> DerefMut for KernelMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T: ?Sized> Drop for KernelMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.locked.store(false, Ordering::Release);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for KernelMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => write!(f, "KernelMutex {{ data: {:?} }}", &*guard),
            None => write!(f, "KernelMutex {{ <locked> }}"),
        }
    }
}

impl<T: Default> Default for KernelMutex<T> {
    fn default() -> Self {
        KernelMutex::new(T::default())
    }
}

impl<T> From<T> for KernelMutex<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

/// #523 RCA breakpoint target: called when a KernelMutex is contended
/// while this CPU holds an IrqLock — the genuine #829 anomaly. A gdb
/// breakpoint at its entry stops the system with the live call
/// hierarchy intact. Arguments (SysV ABI): rdi = mutex address,
/// rsi = `core::panic::Location` of the acquiring caller. The volatile
/// reads keep LTO from eliminating the (otherwise empty) body.
#[cfg(target_os = "none")]
#[no_mangle]
#[inline(never)]
pub extern "C" fn lock_anomaly_detected(mutex: *const (), caller: *const ()) {
    unsafe {
        core::ptr::read_volatile(&mutex);
        core::ptr::read_volatile(&caller);
    }
}
