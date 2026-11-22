//! LibOS executor: runs a future to completion on the current OS
//! thread. Replaces async-std's block_on (whose thread pool caused
//! the #873 teardown crash) with a minimal park-based executor.

use core::future::Future;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::sync::Arc;
use std::thread::{self, Thread};

/// A waker that unparks the calling thread.
struct ThreadWaker {
    thread: Thread,
}

/// Build a Waker that unparks the given thread.
fn thread_waker(thread: Thread) -> Waker {
    fn clone(ptr: *const ()) -> RawWaker {
        let waker = unsafe { Arc::from_raw(ptr as *const ThreadWaker) };
        let cloned = Arc::new(ThreadWaker {
            thread: waker.thread.clone(),
        });
        RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
    }

    fn wake(ptr: *const ()) {
        let waker = unsafe { Arc::from_raw(ptr as *const ThreadWaker) };
        waker.thread.unpark();
    }

    fn wake_by_ref(ptr: *const ()) {
        let waker = unsafe { Arc::from_raw(ptr as *const ThreadWaker) };
        waker.thread.unpark();
        std::mem::forget(waker); // don't drop — we only borrowed it
    }

    fn drop_fn(ptr: *const ()) {
        // Note: must use std::mem::drop explicitly — the fn name
        // `drop_fn` avoids shadowing, but be explicit for clarity.
        let _ = unsafe { Arc::from_raw(ptr as *const ThreadWaker) };
    }

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop_fn);

    let waker = Arc::new(ThreadWaker { thread });
    unsafe { Waker::from_raw(RawWaker::new(Arc::into_raw(waker) as *const (), &VTABLE)) }
}

/// Run a future to completion on the current OS thread, parking
/// between polls. This is a minimal executor — no thread pool, no
/// reactor, no I/O driver. The future must not rely on timers or
/// I/O readiness (those need a reactor); it should only use
/// thread-based wakeups.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let thread = thread::current();
    let waker = thread_waker(thread);
    let mut cx = Context::from_waker(&waker);

    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}

/// Run a future to completion and return an exit code (the public
/// executor API). Uses the same minimal block_on.
pub fn run_executor<F: Future<Output = i32> + Send + 'static>(future: F) -> i32 {
    block_on(future)
}
