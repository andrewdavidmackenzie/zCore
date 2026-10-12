//! Thread sampler tests.
//!
//! Ports of Fuchsia's thread sampler test suite.
//! zCore has no thread sampler infrastructure. These tests verify
//! thread state read API which is the basis for sampling.

use crate::helpers::TestContext;
use zircon_object::object::KernelObject;
use zircon_object::task::{Job, Process, Thread, ThreadState};

/// C++: TEST(ThreadSampler, CreateSampler)
/// Create a thread and verify it can be inspected.
#[test]
fn create_sampler() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "sampler").unwrap();
    let thread = Thread::create(&proc, "sample-thread").unwrap();
    assert_ne!(thread.id(), 0);
    assert_eq!(thread.state(), ThreadState::New);
}

/// C++: TEST(ThreadSampler, StartStop)
/// Verify thread lifecycle: create, start, kill.
#[async_std::test]
async fn start_stop() {
    use crate::helpers;
    use zircon_object::object::{KernelObject, Signal};
    use zircon_object::task::Task;

    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "sampler").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;

    assert!(thread.signal().contains(Signal::THREAD_RUNNING));

    thread.kill();
    let ko: alloc::sync::Arc<dyn KernelObject> = thread.clone();
    ko.wait_signal(Signal::THREAD_TERMINATED).await;
}

/// C++: TEST(ThreadSampler, ReadState)
/// Read thread general register state while suspended.
#[async_std::test]
async fn read_state() {
    use crate::helpers;
    use zircon_object::object::{KernelObject, Signal};
    use zircon_object::task::{Task, ThreadStateKind};

    helpers::ensure_hal_init();
    let job = Job::root();
    let proc = Process::create(&job, "sampler").unwrap();
    let thread = Thread::create(&proc, "t1").unwrap();

    thread.start(helpers::test_loop_thread_fn).unwrap();
    async_std::task::yield_now().await;
    async_std::task::yield_now().await;

    thread.suspend();
    let ko: alloc::sync::Arc<dyn KernelObject> = thread.clone();
    ko.wait_signal(Signal::THREAD_SUSPENDED).await;

    let mut buf = vec![0u8; 1024];
    let size = thread
        .read_state(ThreadStateKind::General, &mut buf)
        .unwrap();
    assert!(size > 0);

    thread.resume();
    thread.kill();
    ko.wait_signal(Signal::THREAD_TERMINATED).await;
}

/// C++: TEST(ThreadSampler, InvalidArgs)
/// Verify thread creation with various names.
#[test]
fn invalid_args() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "sampler").unwrap();

    let t1 = Thread::create(&proc, "").unwrap();
    assert_ne!(t1.id(), 0);

    let t2 = Thread::create(&proc, "a-very-long-thread-name-for-testing").unwrap();
    assert_ne!(t2.id(), 0);
}

/// C++: TEST(ThreadSampler, MultipleThreads)
/// Sample multiple threads.
#[test]
fn multiple_threads() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "sampler").unwrap();

    let threads: Vec<_> = (0..5)
        .map(|i| Thread::create(&proc, &format!("t{}", i)).unwrap())
        .collect();

    for (i, t) in threads.iter().enumerate() {
        assert_ne!(t.id(), 0);
        assert_eq!(t.name(), format!("t{}", i));
    }
}

/// C++: TEST(ThreadSampler, BufferOverflow)
/// Verify thread identity is unique.
#[test]
fn buffer_overflow() {
    let _ctx = TestContext::new();
    let job = Job::root();
    let proc = Process::create(&job, "sampler").unwrap();

    let mut ids = alloc::vec::Vec::new();
    for _ in 0..10 {
        let t = Thread::create(&proc, "t").unwrap();
        ids.push(t.id());
    }

    // All IDs should be unique.
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 10, "all thread IDs should be unique");
}

extern crate alloc;
