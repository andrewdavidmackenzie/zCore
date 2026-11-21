//! Type-level lock ordering (lockgraph).
//!
//! Adapted from Fuchsia's netstack3 `lock-order` crate (BSD-3-Clause,
//! Copyright 2023 The Fuchsia Authors).
//!
//! Locks are given type-level *names* (zero-sized marker types), the
//! acquisition-order graph is encoded with the [`LockAfter`]/
//! [`LockBefore`] traits plus [`impl_lock_after!`], and a
//! [`Locked<T, L>`] context threads the *current* lock level through
//! code. The compiler then enforces two properties:
//!
//! 1. **Out-of-order acquisition does not compile**: [`Locked::lock`]
//!    requires `L: LockBefore<M>` — the current level must come
//!    before the lock being acquired in the declared graph.
//! 2. **Cyclic graphs do not compile**: [`impl_lock_after!`]'s
//!    blanket impls propagate the graph transitively; a cycle makes
//!    the compiler derive an implicit impl identical to an explicit
//!    one, which coherence rejects. See the macro's documentation
//!    for the worked example.
//!
//! # Example
//!
//! ```
//! use lock::impl_lock_after;
//! use lock::lockgraph::{LockAfter, LockFor, Locked, Unlocked};
//! use lock::KernelMutex;
//!
//! struct State {
//!     a: KernelMutex<u8>,
//!     b: KernelMutex<u16>,
//! }
//!
//! // Lock level names — zero-sized, never constructed.
//! enum LockA {}
//! enum LockB {}
//!
//! // Declare which field each lock level refers to.
//! impl LockFor<LockA> for State {
//!     type Data = u8;
//!     type Guard<'l> = lock::KernelMutexGuard<'l, u8>;
//!     fn lock(&self) -> Self::Guard<'_> {
//!         self.a.lock()
//!     }
//! }
//!
//! impl LockFor<LockB> for State {
//!     type Data = u16;
//!     type Guard<'l> = lock::KernelMutexGuard<'l, u16>;
//!     fn lock(&self) -> Self::Guard<'_> {
//!         self.b.lock()
//!     }
//! }
//!
//! // Declare the graph: A before B (A is a root).
//! impl LockAfter<Unlocked> for LockA {}
//! impl_lock_after!(LockA => LockB);
//!
//! let state = State { a: KernelMutex::new(1), b: KernelMutex::new(2) };
//! let mut locked = Locked::new(&state);
//! let (a, mut locked) = locked.lock_and::<LockA>();
//! assert_eq!(*a, 1);
//! let b = locked.lock::<LockB>();
//! assert_eq!(*b, 2);
//! ```
//!
//! Out-of-order acquisition does not compile:
//!
//! ```compile_fail
//! # use lock::impl_lock_after;
//! # use lock::lockgraph::{LockAfter, LockFor, Locked, Unlocked};
//! # use lock::KernelMutex;
//! # struct State { a: KernelMutex<u8>, b: KernelMutex<u16> }
//! # enum LockA {}
//! # enum LockB {}
//! # impl LockFor<LockA> for State {
//! #     type Data = u8;
//! #     type Guard<'l> = lock::KernelMutexGuard<'l, u8>;
//! #     fn lock(&self) -> Self::Guard<'_> { self.a.lock() }
//! # }
//! # impl LockFor<LockB> for State {
//! #     type Data = u16;
//! #     type Guard<'l> = lock::KernelMutexGuard<'l, u16>;
//! #     fn lock(&self) -> Self::Guard<'_> { self.b.lock() }
//! # }
//! # impl LockAfter<Unlocked> for LockA {}
//! # impl_lock_after!(LockA => LockB);
//! let state = State { a: KernelMutex::new(1), b: KernelMutex::new(2) };
//! let mut locked = Locked::new(&state);
//! let (_b, mut locked) = locked.lock_and::<LockB>();
//! // LockA does not come after LockB in the graph:
//! let _a = locked.lock::<LockA>();
//! ```
//!
//! And declaring a cyclic graph does not compile either — see
//! [`impl_lock_after!`].

use core::marker::PhantomData;
use core::ops::Deref;

// ── Relation traits ─────────────────────────────────────────────────

/// Marker trait: `Self` (a lock level) can be locked **after** `A`.
///
/// Prefer the [`impl_lock_after!`] macro over writing impls directly —
/// in addition to emitting the direct implementation, it provides a
/// blanket impl that makes the relation transitive AND makes cycles
/// fail to compile (see the macro docs).
///
/// Note: like Fuchsia's netstack3 lock-order crate, this is a plain
/// trait rather than an `unsafe` one — the macro's blanket impls make
/// cycles a hard compile error, which is "hard to circumvent by
/// accident" rather than unsafe-enforced.
pub trait LockAfter<A> {}

/// Marker trait: `Self` (a lock level) comes before `X`.
///
/// Prefer bounds on `LockBefore` in APIs (it is the query direction);
/// implement [`LockAfter`] (via the macro) instead of this trait. A
/// blanket impl of `LockBefore` is provided for all types implementing
/// `LockAfter`.
pub trait LockBefore<X> {}

impl<B: LockAfter<A>, A> LockBefore<B> for A {}

/// Declare an edge in the lock-order graph: locks of `$B` may be
/// acquired while holding locks of `$A`.
///
/// In addition to the direct implementation
/// `impl LockAfter<$A> for $B {}`, this emits a blanket impl that
/// makes the relation transitive and — crucially — makes **cycles
/// fail to compile**.
///
/// # How cycle rejection works
///
/// For a chain `A → B → C`, the macro emits (eliding paths):
///
/// ```ignore
/// impl LockAfter<A> for B {}
/// impl<X: LockBefore<A>> LockAfter<X> for B {}   // transitivity
/// impl LockAfter<B> for C {}
/// impl<X: LockBefore<B>> LockAfter<X> for C {}
/// ```
///
/// Now suppose we add the edge `C => A`, closing the cycle. The
/// compiler follows the blankets around the loop and derives an
/// *implicit* `impl LockAfter<C> for A` — which is exactly the
/// explicit impl the macro just emitted for the new edge. Coherence
/// detects the duplication and rejects the code. This works for any
/// graph containing a cycle, not just triangles.
///
/// Declaring a cycle therefore fails to compile:
///
/// ```compile_fail
/// # use lock::impl_lock_after;
/// # use lock::lockgraph::LockAfter;
/// enum LockA {}
/// enum LockB {}
///
/// impl LockAfter<lock::lockgraph::Unlocked> for LockA {}
/// impl_lock_after!(LockA => LockB);
/// impl_lock_after!(LockB => LockA); // cycle — conflict
/// ```
///
/// # Safety
///
/// The caller asserts that adding this edge does not create a cycle.
/// The macro's blanket impls enforce this mechanically — a cyclic
/// declaration fails to compile.
#[macro_export]
macro_rules! impl_lock_after {
    ($A:ty => $B:ty) => {
        impl $crate::lockgraph::LockAfter<$A> for $B {}
        impl<X: $crate::lockgraph::LockBefore<$A>> $crate::lockgraph::LockAfter<X> for $B {}
    };
}

// ── Lock level access ───────────────────────────────────────────────

/// The root of every lock-order graph: "no locks held".
///
/// The top level of each tree must implement `LockAfter<Unlocked>`
/// directly (`impl LockAfter<Unlocked> for MyRoot {}`). Do NOT use
/// `impl_lock_after!` for the root edge: the macro's direct and
/// blanket impls could overlap for the `Unlocked` root in downstream
/// crates.
pub enum Unlocked {}

/// Declares that `Self` has a lockable field for lock level `M`,
/// and how to acquire it.
///
/// The guard type is a generic associated type so the machinery works
/// with any lock primitive (`KernelMutex`, the irq-safe `Mutex`,
/// rwlocks — see #829).
pub trait LockFor<M> {
    /// The data protected by the lock.
    type Data;
    /// The guard type returned by acquiring it.
    type Guard<'l>
    where
        Self: 'l;

    /// Acquire the lock for level `M`.
    fn lock(&self) -> Self::Guard<'_>;
}

// ── The lock context ────────────────────────────────────────────────

/// A context tracking the currently-held lock level `L`.
///
/// `Locked` enforces lock ordering by construction:
/// - [`Locked::lock`] takes `&mut self`, so the previous context is
///   unusable while the guard lives (the borrow ends when the guard
///   drops), and requires `L: LockBefore<M>`.
/// - [`Locked::lock_and`] additionally returns a new context at
///   level `M`; the original context stays mutably borrowed while
///   either the guard or the new context lives, so only locks
///   downstream of `M` can be acquired.
///
/// `T` is typically `&State`; `Locked` adds nothing at runtime.
pub struct Locked<T, L>(T, PhantomData<L>);

impl<'a, T> Locked<&'a T, Unlocked>
where
    T: 'a,
{
    /// Entry point: create a context at the root level (no locks
    /// held) over shared state.
    pub fn new(t: &'a T) -> Self {
        Locked(t, PhantomData)
    }
}

impl<T, L> Locked<T, L> {
    /// Acquire the lock for level `M`, returning only the guard.
    ///
    /// The guard borrows this context mutably, so no further locks
    /// can be acquired through it until the guard is dropped. Use
    /// [`Locked::lock_and`] to continue acquiring downstream locks
    /// while holding this one.
    ///
    /// Requires `L: LockBefore<M>`: the current level must precede
    /// `M` in the declared graph — out-of-order acquisition does not
    /// compile.
    pub fn lock<M>(&mut self) -> <T::Target as LockFor<M>>::Guard<'_>
    where
        T: Deref,
        T::Target: LockFor<M>,
        L: LockBefore<M>,
    {
        let (guard, _) = self.lock_and::<M>();
        guard
    }

    /// Acquire the lock for level `M` and a new context at that
    /// level.
    ///
    /// The returned [`Locked`] holds a shared borrow of the state at
    /// level `M`; it (and the guard) keep the original context
    /// mutably borrowed, so only the new context — restricted to
    /// locks downstream of `M` — can be used while this one is held.
    pub fn lock_and<M>(&mut self) -> (<T::Target as LockFor<M>>::Guard<'_>, Locked<&T::Target, M>)
    where
        T: Deref,
        T::Target: LockFor<M>,
        L: LockBefore<M>,
    {
        let t: &T::Target = &*self.0;
        let guard = t.lock();
        (guard, Locked(t, PhantomData))
    }
}
