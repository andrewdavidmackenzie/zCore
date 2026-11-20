//! Kernel object basis.
//!
//! # Create new kernel object
//!
//! - Create a new struct.
//! - Make sure it has a field named `base` with type [`KObjectBase`].
//! - Implement [`KernelObject`] trait with [`impl_kobject`] macro.
//!
//! ## Example
//! ```
//! use zircon_object::object::*;
//! extern crate alloc;
//!
//! pub struct SampleObject {
//!    base: KObjectBase,
//! }
//! impl_kobject!(SampleObject);
//! ```
//!
//! # Implement methods for kernel object
//!
//! ## Constructor
//!
//! Each kernel object should have a constructor returns `Arc<Self>`
//! (or a pair of them, e.g. [`Channel`]).
//!
//! Don't return `Self` since it must be created on heap.
//!
//! ### Example
//! ```
//! use zircon_object::object::*;
//! use std::sync::Arc;
//!
//! pub struct SampleObject {
//!     base: KObjectBase,
//! }
//! impl SampleObject {
//!     pub fn new() -> Arc<Self> {
//!         Arc::new(SampleObject {
//!             base: KObjectBase::new(),
//!         })
//!     }
//! }
//! ```
//!
//! ## Interior mutability
//!
//! All kernel objects use the [interior mutability pattern] :
//! each method takes either `&self` or `&Arc<Self>` as the first argument.
//!
//! To handle mutable variable, create another **inner structure**,
//! and put it into the object with a lock wrapped.
//!
//! ### Example
//! ```
//! use zircon_object::object::*;
//! use std::sync::Arc;
//! use lock::Mutex;
//!
//! pub struct SampleObject {
//!     base: KObjectBase,
//!     inner: Mutex<SampleObjectInner>,
//! }
//! struct SampleObjectInner {
//!     x: usize,
//! }
//!
//! impl SampleObject {
//!     pub fn set_x(&self, x: usize) {
//!         let mut inner = self.inner.lock();
//!         inner.x = x;
//!     }
//! }
//! ```
//!
//! # Downcast trait to concrete type
//!
//! [`KernelObject`] inherit [`downcast_rs::DowncastSync`] trait.
//! You can use `downcast_arc` method to downcast `Arc<dyn KernelObject>` to `Arc<T: KernelObject>`.
//!
//! ## Example
//! ```
//! use zircon_object::object::*;
//! use std::sync::Arc;
//!
//! let object: Arc<dyn KernelObject> = DummyObject::new();
//! let concrete = object.downcast_arc::<DummyObject>().unwrap();
//! ```
//!
//! [`Channel`]: crate::ipc::Channel
//! [`KObjectBase`]: KObjectBase
//! [`KernelObject`]: KernelObject
//! [`impl_kobject`]: impl_kobject
//! [`downcast_rs::DowncastSync`]: downcast_rs::DowncastSync
//! [interior mutability pattern]: https://doc.rust-lang.org/reference/interior-mutability.html

use {
    crate::signal::*,
    alloc::{string::String, sync::Arc, vec::Vec},
    core::{
        fmt::Debug,
        future::Future,
        pin::Pin,
        sync::atomic::*,
        task::{Context, Poll},
    },
    downcast_rs::{impl_downcast, DowncastSync},
    // KObjectBase::inner uses InterruptSpinLock (not KernelMutex)
    // because signal_set/signal_change are called from timer interrupt
    // callbacks. Using KernelMutex here would deadlock if the
    // interrupted thread holds the same lock.
    lock::Mutex,
};

pub use {super::*, handle::*, rights::*, signal::*};

mod handle;
mod rights;
pub mod signal;

/// Common interface of a kernel object.
///
/// Implemented by [`impl_kobject`] macro.
///
/// [`impl_kobject`]: impl_kobject
pub trait KernelObject: DowncastSync + Debug {
    /// Get object's KoID.
    fn id(&self) -> KoID;
    /// Get the name of the type of the kernel object.
    fn type_name(&self) -> &str;
    /// Get object's name.
    fn name(&self) -> alloc::string::String;
    /// Set object's name.
    fn set_name(&self, name: &str);
    /// Whether this object supports the ZX_PROP_NAME property.
    ///
    /// Most kernel objects support names, but some (e.g. Stream) do not.
    /// Returns true by default; override to false for unsupported types.
    fn supports_name(&self) -> bool {
        true
    }
    /// Get the signal status.
    fn signal(&self) -> Signal;
    /// Assert `signal`.
    fn signal_set(&self, signal: Signal);
    /// Deassert `signal`.
    fn signal_clear(&self, signal: Signal);
    /// Change signal status: first `clear` then `set` indicated bits.
    ///
    /// All signal callbacks will be called.
    fn signal_change(&self, clear: Signal, set: Signal);
    /// Add `callback` for signal status changes.
    ///
    /// The `callback` is a function of `Fn(Signal) -> bool`.
    /// It returns a bool indicating whether the handle process is over.
    /// If true, the function will never be called again.
    fn add_signal_callback(&self, callback: SignalHandler);
    /// Add a signal callback without immediate invocation (for edge-triggered).
    fn add_signal_callback_deferred(&self, callback: SignalHandler) {
        self.add_signal_callback(callback);
    }
    /// Return the current number of registered signal callbacks.
    fn signal_callback_count(&self) -> usize {
        0
    }
    /// Attempt to find a child of the object with given KoID.
    ///
    /// If the object is a *Process*, the *Threads* it contains may be obtained.
    ///
    /// If the object is a *Job*, its (immediate) child *Jobs* and the *Processes*
    /// it contains may be obtained.
    ///
    /// If the object is a *Resource*, its (immediate) child *Resources* may be obtained.
    fn get_child(&self, _id: KoID) -> ZxResult<Arc<dyn KernelObject>> {
        Err(ZxError::WRONG_TYPE)
    }
    /// Attempt to get the object's peer.
    ///
    /// An object peer is the opposite endpoint of a `Channel`, `Socket`, `Fifo`, or `EventPair`.
    fn peer(&self) -> ZxResult<Arc<dyn KernelObject>> {
        Err(ZxError::NOT_SUPPORTED)
    }
    /// If the object is related to another (such as the other end of a channel, or the parent of
    /// a job), returns the KoID of that object, otherwise returns zero.
    fn related_koid(&self) -> KoID {
        0
    }
    /// Get object's allowed signals.
    fn allowed_signals(&self) -> Signal {
        Signal::USER_ALL
    }
    /// Get the number of userspace handles referring to this object.
    fn handle_count(&self) -> u32 {
        0
    }
    /// Increment the handle count (called when a handle is added to a process).
    fn inc_handle_count(&self) {}
    /// Decrement the handle count (called when a handle is removed from a process).
    fn dec_handle_count(&self) {}
    /// Called after dec_handle_count when the handle count reaches zero.
    /// Override in impl_kobject! for types that need cleanup.
    fn on_zero_handles(&self) {}
    /// Downcast to `VmObject` without using `TypeId`.
    /// Returns `None` by default. Overridden for VmObject.
    fn as_vmo(&self) -> Option<&crate::vm::VmObject> {
        None
    }
    /// Downcast to `Resource` without using `TypeId`.
    ///
    /// Returns `None` by default. Overridden by `impl_kobject!(Resource ...)`
    /// to return `Some(self)`. This provides a type-safe downcast path that
    /// is immune to LTO-induced `TypeId` instability, and cannot be spoofed
    /// by other `KernelObject` implementations because only `Resource`
    /// can return a valid `&Resource` reference from `self`.
    fn as_resource(&self) -> Option<&crate::dev::Resource> {
        None
    }
}

impl_downcast!(sync KernelObject);

/// The base struct of a kernel object.
pub struct KObjectBase {
    /// The object's KoID.
    pub id: KoID,
    /// Number of userspace handles referring to this object.
    handle_count: AtomicU32,
    inner: Mutex<KObjectBaseInner>,
}

/// The mutable part of `KObjectBase`.
#[derive(Default)]
struct KObjectBaseInner {
    name: String,
    signal: Signal,
    signal_callbacks: Vec<SignalHandler>,
}

impl Default for KObjectBase {
    fn default() -> Self {
        KObjectBase {
            id: Self::new_koid(),
            handle_count: AtomicU32::new(0),
            inner: Default::default(),
        }
    }
}

impl KObjectBase {
    /// Create a new kernel object base.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a kernel object base with initial `signal`.
    pub fn with_signal(signal: Signal) -> Self {
        KObjectBase::with(Default::default(), signal)
    }

    /// Create a kernel object base with `name`.
    pub fn with_name(name: &str) -> Self {
        KObjectBase::with(name, Default::default())
    }

    /// Create a kernel object base with both signal and name
    pub fn with(name: &str, signal: Signal) -> Self {
        KObjectBase {
            id: Self::new_koid(),
            handle_count: AtomicU32::new(0),
            inner: Mutex::new(KObjectBaseInner {
                name: String::from(name),
                signal,
                ..Default::default()
            }),
        }
    }

    /// Generate a new KoID.
    fn new_koid() -> KoID {
        static KOID: AtomicU64 = AtomicU64::new(1024);
        KOID.fetch_add(1, Ordering::SeqCst)
    }

    /// Increment the handle count (called when a handle is added to a process).
    pub fn inc_handle_count(&self) {
        self.handle_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the handle count (called when a handle is removed from a process).
    pub fn dec_handle_count(&self) {
        self.handle_count.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get the number of userspace handles referring to this object.
    pub fn get_handle_count(&self) -> u32 {
        self.handle_count.load(Ordering::Relaxed)
    }

    /// Get object's name.
    pub fn name(&self) -> String {
        self.inner.lock().name.clone()
    }

    /// Set object's name.
    pub fn set_name(&self, name: &str) {
        self.inner.lock().name = String::from(name);
    }

    /// Get the signal status.
    pub fn signal(&self) -> Signal {
        self.inner.lock().signal
    }

    /// Change signal status: first `clear` then `set` indicated bits.
    ///
    /// Satisfied observers are collected (and one-shots removed) under
    /// the lock, then their delivery actions run after releasing it —
    /// see [`SignalHandler`] for why delivery must not run under the
    /// lock. Atomicity with registration is preserved by the lock: a
    /// handler is either registered before this runs (it is collected
    /// and fired after) or added after (its predicate is checked
    /// against the already-updated signal at registration time).
    pub fn signal_change(&self, clear: Signal, set: Signal) {
        // Collect satisfied handlers under the lock, in reverse (LIFO)
        // order matching Fuchsia's behavior where the most recently
        // registered observer is notified first. Order-preserving
        // `remove` keeps surviving callbacks in relative order.
        let (new_signal, mut to_fire) = {
            let mut inner = self.inner.lock();
            let old_signal = inner.signal;
            inner.signal.remove(clear);
            inner.signal.insert(set);
            let new_signal = inner.signal;
            if new_signal == old_signal {
                return;
            }
            let mut to_fire = Vec::new();
            let mut i = inner.signal_callbacks.len();
            while i > 0 {
                i -= 1;
                if inner.signal_callbacks[i].is_satisfied(new_signal) {
                    let handler = inner.signal_callbacks.remove(i);
                    to_fire.push(handler);
                }
            }
            (new_signal, to_fire)
        };
        // Deliver outside the lock. Non-one-shot handlers were also
        // removed above — they are re-registered after delivery.
        let mut permanent = Vec::new();
        for handler in to_fire.drain(..) {
            handler.fire(new_signal);
            if !handler.oneshot {
                permanent.push(handler);
            }
        }
        if !permanent.is_empty() {
            let mut inner = self.inner.lock();
            for handler in permanent {
                inner.signal_callbacks.push(handler);
            }
        }
    }

    /// Assert `signal`.
    pub fn signal_set(&self, signal: Signal) {
        self.signal_change(Signal::empty(), signal);
    }

    /// Deassert `signal`.
    pub fn signal_clear(&self, signal: Signal) {
        self.signal_change(signal, Signal::empty());
    }

    /// Maximum number of signal callbacks (observers) per object.
    /// Prevents OOM when creating many observers. Fuchsia handles
    /// OOM gracefully; we use a fixed limit since our allocator
    /// panics on OOM.
    const MAX_SIGNAL_CALLBACKS: usize = 16384;

    /// Add `callback` for signal status changes.
    ///
    /// The handler's predicate is checked against the current signal
    /// under the lock; if already satisfied, the delivery action runs
    /// immediately — OUTSIDE the lock. A satisfied one-shot is not
    /// stored (it is done); a satisfied permanent handler is stored
    /// AND fired (it keeps observing future changes).
    pub fn add_signal_callback(&self, callback: SignalHandler) {
        let fire_now = {
            let mut inner = self.inner.lock();
            if callback.is_satisfied(inner.signal) {
                let signal = inner.signal;
                let action = callback.fire_clone();
                if !callback.oneshot && inner.signal_callbacks.len() < Self::MAX_SIGNAL_CALLBACKS {
                    inner.signal_callbacks.push(callback);
                }
                Some((action, signal))
            } else if inner.signal_callbacks.len() < Self::MAX_SIGNAL_CALLBACKS {
                inner.signal_callbacks.push(callback);
                None
            } else {
                None
            }
        };
        if let Some((action, signal)) = fire_now {
            action(signal);
        }
    }

    /// Add a signal callback without immediate invocation.
    ///
    /// Unlike `add_signal_callback`, this does NOT check the current signal
    /// state. The callback will only fire on future signal changes.
    /// Used for edge-triggered wait_async. Enforces the callback limit.
    pub fn add_signal_callback_deferred(&self, callback: SignalHandler) {
        let mut inner = self.inner.lock();
        if inner.signal_callbacks.len() < Self::MAX_SIGNAL_CALLBACKS {
            inner.signal_callbacks.push(callback);
        }
    }

    /// Return the current number of registered signal callbacks.
    pub fn signal_callback_count(&self) -> usize {
        self.inner.lock().signal_callbacks.len()
    }

    /// Like `add_signal_callback` but returns `NO_MEMORY` when the
    /// observer limit is reached. Atomically checks count and inserts
    /// under the same lock to prevent TOCTOU races. Used by
    /// `send_signal_to_port_async` (wait_async syscall).
    pub fn try_add_signal_callback(&self, callback: SignalHandler) -> ZxResult {
        let fire_now = {
            let mut inner = self.inner.lock();
            if callback.is_satisfied(inner.signal) {
                let signal = inner.signal;
                let action = callback.fire_clone();
                if !callback.oneshot {
                    if inner.signal_callbacks.len() >= Self::MAX_SIGNAL_CALLBACKS {
                        return Err(ZxError::NO_MEMORY);
                    }
                    inner.signal_callbacks.push(callback);
                }
                Some((action, signal))
            } else {
                if inner.signal_callbacks.len() >= Self::MAX_SIGNAL_CALLBACKS {
                    return Err(ZxError::NO_MEMORY);
                }
                inner.signal_callbacks.push(callback);
                None
            }
        };
        if let Some((action, signal)) = fire_now {
            action(signal);
        }
        Ok(())
    }
}

impl dyn KernelObject {
    /// Asynchronous wait for one of `signal`.
    pub fn wait_signal(self: &Arc<Self>, signal: Signal) -> impl Future<Output = Signal> {
        #[must_use = "wait_signal does nothing unless polled/`await`-ed"]
        struct SignalFuture {
            object: Arc<dyn KernelObject>,
            signal: Signal,
        }

        impl Future for SignalFuture {
            type Output = Signal;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let current_signal = self.object.signal();
                if !(current_signal & self.signal).is_empty() {
                    return Poll::Ready(current_signal);
                }
                // Always register a callback when returning Pending.
                // A previous callback may have been consumed (it fired
                // and returned true) but another thread cleared the
                // signal before we were re-polled — a "spurious wakeup".
                // Without re-registering, we'd be stuck Pending forever
                // with no callback to wake us.
                // `add_signal_callback` atomically checks the current
                // signal state: if the signal was set between our check
                // above and the registration, the callback fires
                // immediately and isn't stored, so we get re-polled.
                let signal = self.signal;
                let waker = cx.waker().clone();
                self.object.add_signal_callback(SignalHandler::oneshot(
                    move |s| !(s & signal).is_empty(),
                    move |_| waker.wake_by_ref(),
                ));
                Poll::Pending
            }
        }

        SignalFuture {
            object: self.clone(),
            signal,
        }
    }

    /// Once one of the `signal` asserted, push a packet with `key` into the `port`.
    ///
    /// Used to implement `sys_object_wait_async`. Level-triggered (one-shot):
    /// fires immediately if signal already asserted, otherwise registers a
    /// callback that fires on the next matching signal change.
    /// If `timestamp` is true, the packet includes the current monotonic time.
    #[allow(unsafe_code)]
    pub fn send_signal_to_port_async(
        self: &Arc<Self>,
        signal: Signal,
        port: &Arc<Port>,
        key: u64,
        timestamp: bool,
    ) -> ZxResult {
        self.send_signal_to_port_async_inner(signal, port, key, false, timestamp)
    }

    /// Edge-triggered variant: only fires when signal transitions from
    /// deasserted to asserted (never fires immediately).
    #[allow(unsafe_code)]
    pub fn send_signal_to_port_async_edge(
        self: &Arc<Self>,
        signal: Signal,
        port: &Arc<Port>,
        key: u64,
        timestamp: bool,
    ) -> ZxResult {
        self.send_signal_to_port_async_inner(signal, port, key, true, timestamp)
    }

    #[allow(unsafe_code)]
    fn send_signal_to_port_async_inner(
        self: &Arc<Self>,
        signal: Signal,
        port: &Arc<Port>,
        key: u64,
        edge_triggered: bool,
        timestamp: bool,
    ) -> ZxResult {
        // Check observer limit BEFORE allocating anything. This ensures
        // we return ZX_ERR_NO_MEMORY instead of panicking on OOM when
        // the observer count is at the limit.
        if self.signal_callback_count() >= KObjectBase::MAX_SIGNAL_CALLBACKS {
            return Err(ZxError::NO_MEMORY);
        }

        // Register cancellation tracking BEFORE the immediate check,
        // so cancel_async can find it even for level-triggered waits
        // that fire immediately.
        let cancelled = port.register_async(self.id(), key);

        // For level-triggered mode, fire immediately if signal already set.
        if !edge_triggered {
            let current_signal = self.signal();
            if !(current_signal & signal).is_empty() {
                let ts = if timestamp {
                    hal_impl::timer::timer_now().as_nanos() as u64
                } else {
                    0
                };
                port.push_with_source(
                    PortPacketRepr {
                        key,
                        status: ZxError::OK,
                        data: PayloadRepr::Signal(PacketSignal {
                            trigger: signal,
                            observed: current_signal,
                            count: 1,
                            timestamp: ts,
                            _reserved1: 0,
                        }),
                    },
                    self.id(),
                );
                // Remove the subscription since we already fired.
                port.cancel_async(self.id(), key).ok();
                return Ok(());
            }
        }
        // Register the callback. For edge mode, use deferred registration
        // (skip immediate signal check) so the callback only fires on
        // future signal transitions.
        let cancelled_for_error = cancelled.clone();
        let source_koid = self.id();
        let port = port.clone();
        let callback = SignalHandler::oneshot(
            // Predicate (pure, runs under the object lock): satisfied
            // when cancelled (delivery becomes a no-op — see below) or
            // the observed signal matches.
            {
                let cancelled = cancelled.clone();
                move |s| {
                    cancelled.load(core::sync::atomic::Ordering::Relaxed)
                        || !(s & signal).is_empty()
                }
            },
            // Delivery (runs OUTSIDE the object lock): push the packet
            // under the port lock. `push_if_not_cancelled` atomically
            // re-checks the cancellation flag under the port lock,
            // preventing the TOCTOU race where cancel_by_key drains
            // the queue between the predicate check and the push. A
            // cancelled handler pushes nothing — the one-shot flag
            // still removes it.
            move |s| {
                let ts = if timestamp {
                    hal_impl::timer::timer_now().as_nanos() as u64
                } else {
                    0
                };
                if port.push_if_not_cancelled(
                    PortPacketRepr {
                        key,
                        status: ZxError::OK,
                        data: PayloadRepr::Signal(PacketSignal {
                            trigger: signal,
                            observed: s,
                            count: 1,
                            timestamp: ts,
                            _reserved1: 0,
                        }),
                    },
                    source_koid,
                    &cancelled,
                ) {
                    // Clean up subscription after one-shot delivery.
                    port.cancel_async(source_koid, key).ok();
                }
            },
        );
        // Check observer limit before registering. The limit is
        // enforced inside add_signal_callback/add_signal_callback_deferred
        // as well, but we check here to return an error to the caller.
        // The trait's signal_callback_count() and add_signal_callback()
        // each acquire the lock separately, so there is a small TOCTOU
        // window. The inner enforcement ensures we never exceed the limit
        // even if two calls race.
        if self.signal_callback_count() >= KObjectBase::MAX_SIGNAL_CALLBACKS {
            // Cancel only our own subscription.
            cancelled_for_error.store(true, core::sync::atomic::Ordering::Relaxed);
            return Err(ZxError::NO_MEMORY);
        }
        if edge_triggered {
            self.add_signal_callback_deferred(callback);
        } else {
            self.add_signal_callback(callback);
        }
        Ok(())
    }
}

/// Asynchronous wait signal for multiple objects.
pub fn wait_signal_many(
    targets: &[(Arc<dyn KernelObject>, Signal)],
) -> impl Future<Output = Vec<Signal>> {
    #[must_use = "wait_signal_many does nothing unless polled/`await`-ed"]
    struct SignalManyFuture {
        targets: Vec<(Arc<dyn KernelObject>, Signal)>,
    }

    impl SignalManyFuture {
        fn happened(&self, current_signals: &[Signal]) -> bool {
            self.targets
                .iter()
                .zip(current_signals)
                .any(|(&(_, desired), &current)| !(current & desired).is_empty())
        }
    }

    impl Future for SignalManyFuture {
        type Output = Vec<Signal>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let current_signals: Vec<_> =
                self.targets.iter().map(|(obj, _)| obj.signal()).collect();
            if self.happened(&current_signals) {
                return Poll::Ready(current_signals);
            }
            // Always re-register callbacks when returning Pending.
            // See SignalFuture::poll for rationale — a previous
            // callback may have been consumed by a signal that was
            // then cleared before we were re-polled.
            for (object, signal) in self.targets.iter() {
                let signal = *signal;
                let waker = cx.waker().clone();
                object.add_signal_callback(SignalHandler::oneshot(
                    move |s: Signal| !(s & signal).is_empty(),
                    move |_| waker.wake_by_ref(),
                ));
            }
            Poll::Pending
        }
    }

    SignalManyFuture {
        targets: Vec::from(targets),
    }
}

/// Macro to auto implement `KernelObject` trait.
#[macro_export]
macro_rules! impl_kobject {
    ($class:ident $( $fn:tt )*) => {
        impl $crate::object::KernelObject for $class {
            fn id(&self) -> KoID {
                self.base.id
            }
            fn type_name(&self) -> &str {
                stringify!($class)
            }
            fn name(&self) -> alloc::string::String {
                self.base.name()
            }
            fn set_name(&self, name: &str){
                self.base.set_name(name)
            }
            fn signal(&self) -> Signal {
                self.base.signal()
            }
            fn signal_set(&self, signal: Signal) {
                self.base.signal_set(signal);
            }
            fn signal_clear(&self, signal: Signal) {
                self.base.signal_clear(signal);
            }
            fn signal_change(&self, clear: Signal, set: Signal) {
                self.base.signal_change(clear, set);
            }
            fn add_signal_callback(&self, callback: $crate::object::SignalHandler) {
                self.base.add_signal_callback(callback);
            }
            fn signal_callback_count(&self) -> usize {
                self.base.signal_callback_count()
            }
            fn add_signal_callback_deferred(&self, callback: $crate::object::SignalHandler) {
                self.base.add_signal_callback_deferred(callback);
            }
            fn handle_count(&self) -> u32 {
                self.base.get_handle_count()
            }
            fn inc_handle_count(&self) {
                self.base.inc_handle_count();
            }
            fn dec_handle_count(&self) {
                self.base.dec_handle_count();
            }
            $( $fn )*
        }
        impl core::fmt::Debug for $class {
            fn fmt(
                &self,
                f: &mut core::fmt::Formatter<'_>,
            ) -> core::result::Result<(), core::fmt::Error> {
                use $crate::object::KernelObject;
                f.debug_tuple(&stringify!($class))
                    .field(&self.id())
                    .field(&self.name())
                    .finish()
            }
        }
    };
}

/// Define a pair of kcounter (create, destroy),
/// and a helper struct `CountHelper` which increases the counter on construction and drop.
#[macro_export]
macro_rules! define_count_helper {
    ($class:ident) => {
        struct CountHelper(());
        impl CountHelper {
            fn new() -> Self {
                $crate::kcounter!(CREATE_COUNT, concat!(stringify!($class), ".create"));
                CREATE_COUNT.add(1);
                CountHelper(())
            }
        }
        impl Drop for CountHelper {
            fn drop(&mut self) {
                $crate::kcounter!(DESTROY_COUNT, concat!(stringify!($class), ".destroy"));
                DESTROY_COUNT.add(1);
            }
        }
    };
}

/// The type of kernel object ID.
pub type KoID = u64;

/// A signal-change observer.
///
/// The handler is split into a pure **predicate** and a **delivery
/// action** so that satisfaction can be checked while holding the
/// object lock while the (side-effecting) delivery runs only OUTSIDE
/// it. This avoids nested locking from interrupt context: the timer
/// IRQ signals objects (Timer, Port deadlines), and delivery actions
/// wake tasks (WakerTarget lock + IPI) or push port packets (the
/// port lock) — invoking those under the object lock is a
/// lock-ordering hazard.
pub struct SignalHandler {
    /// Pure predicate: is this handler satisfied by the given signal?
    /// Evaluated while holding the object lock — MUST NOT take locks
    /// or have side effects.
    satisfied: Arc<dyn Fn(Signal) -> bool + Send + Sync>,
    /// Delivery action, invoked only OUTSIDE the object lock, with
    /// the signal that satisfied the predicate.
    fire: Arc<dyn Fn(Signal) + Send + Sync>,
    /// Remove the handler from the observer list after it fires.
    oneshot: bool,
}

impl SignalHandler {
    /// A handler that fires once and is removed.
    pub fn oneshot(
        satisfied: impl Fn(Signal) -> bool + Send + Sync + 'static,
        fire: impl Fn(Signal) + Send + Sync + 'static,
    ) -> Self {
        SignalHandler {
            satisfied: Arc::new(satisfied),
            fire: Arc::new(fire),
            oneshot: true,
        }
    }

    /// A handler that fires on every matching change and stays
    /// registered.
    pub fn permanent(
        satisfied: impl Fn(Signal) -> bool + Send + Sync + 'static,
        fire: impl Fn(Signal) + Send + Sync + 'static,
    ) -> Self {
        SignalHandler {
            satisfied: Arc::new(satisfied),
            fire: Arc::new(fire),
            oneshot: false,
        }
    }

    /// Evaluate the predicate (safe under the object lock).
    fn is_satisfied(&self, signal: Signal) -> bool {
        (self.satisfied)(signal)
    }

    /// Run the delivery action (must be called OUTSIDE the lock).
    fn fire(&self, signal: Signal) {
        (self.fire)(signal);
    }

    /// Clone the delivery action so it can be invoked after the
    /// handler itself has been stored or dropped.
    fn fire_clone(&self) -> Arc<dyn Fn(Signal) + Send + Sync> {
        self.fire.clone()
    }
}

/// Empty kernel object. Just for test.
pub struct DummyObject {
    base: KObjectBase,
}

impl_kobject!(DummyObject);

impl DummyObject {
    /// Create a new `DummyObject`.
    pub fn new() -> Arc<Self> {
        Arc::new(DummyObject {
            base: KObjectBase::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::sync::Barrier;
    use core::time::Duration;

    #[async_std::test]
    async fn wait() {
        let object = DummyObject::new();
        let barrier = Arc::new(Barrier::new(2));
        async_std::task::spawn({
            let object = object.clone();
            let barrier = barrier.clone();
            async move {
                async_std::task::sleep(Duration::from_millis(20)).await;

                // Assert an irrelevant signal to test the `false` branch of the callback for `READABLE`.
                object.signal_set(Signal::USER_SIGNAL_0);
                object.signal_clear(Signal::USER_SIGNAL_0);
                object.signal_set(Signal::READABLE);
                barrier.wait().await;

                object.signal_set(Signal::WRITABLE);
            }
        });
        let object: Arc<dyn KernelObject> = object;

        let signal = object.wait_signal(Signal::READABLE).await;
        assert_eq!(signal, Signal::READABLE);
        barrier.wait().await;

        let signal = object.wait_signal(Signal::WRITABLE).await;
        assert_eq!(signal, Signal::READABLE | Signal::WRITABLE);
    }

    #[async_std::test]
    async fn wait_many() {
        let objs = [DummyObject::new(), DummyObject::new()];
        let barrier = Arc::new(Barrier::new(2));
        async_std::task::spawn({
            let objs = objs.clone();
            let barrier = barrier.clone();
            async move {
                async_std::task::sleep(Duration::from_millis(20)).await;

                objs[0].signal_set(Signal::READABLE);
                barrier.wait().await;

                objs[1].signal_set(Signal::WRITABLE);
            }
        });
        let obj0: Arc<dyn KernelObject> = objs[0].clone();
        let obj1: Arc<dyn KernelObject> = objs[1].clone();

        let signals = wait_signal_many(&[
            (obj0.clone(), Signal::READABLE),
            (obj1.clone(), Signal::READABLE),
        ])
        .await;
        assert_eq!(signals, [Signal::READABLE, Signal::empty()]);
        barrier.wait().await;

        let signals = wait_signal_many(&[
            (obj0.clone(), Signal::WRITABLE),
            (obj1.clone(), Signal::WRITABLE),
        ])
        .await;
        assert_eq!(signals, [Signal::READABLE, Signal::WRITABLE]);
    }

    #[test]
    fn test_trait_with_dummy() {
        let dummy = DummyObject::new();
        assert_eq!(dummy.name(), String::from(""));
        dummy.set_name("test");
        assert_eq!(dummy.name(), String::from("test"));
        dummy.signal_set(Signal::WRITABLE);
        assert_eq!(dummy.signal(), Signal::WRITABLE);
        dummy.signal_change(Signal::WRITABLE, Signal::READABLE);
        assert_eq!(dummy.signal(), Signal::READABLE);

        assert_eq!(dummy.get_child(0).unwrap_err(), ZxError::WRONG_TYPE);
        assert_eq!(dummy.peer().unwrap_err(), ZxError::NOT_SUPPORTED);
        assert_eq!(dummy.related_koid(), 0);
        assert_eq!(dummy.allowed_signals(), Signal::USER_ALL);

        assert_eq!(
            format!("{:?}", dummy),
            format!("DummyObject({}, \"test\")", dummy.id())
        );
    }
}
