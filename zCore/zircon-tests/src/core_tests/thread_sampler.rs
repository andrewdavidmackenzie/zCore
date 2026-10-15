//! Thread sampler tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/thread-sampler/thread-sampler.cc.
//!
//! The thread sampler (zx_sampler_create/start/stop/read) is a kernel-level
//! profiling facility that periodically samples thread register state.
//! zCore does not implement the sampler syscalls or the sampling resource.

use crate::helpers::TestContext;

/// C++: TEST(ThreadSampler, StartStop)
/// Creates a sampler, spawns a thread, starts sampling, reads at least
/// 10 records containing the thread's TID, stops, and cleans up.
#[test]
fn start_stop() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. Get sampling resource via standalone::GetSystemResourceWithBase(ZX_RSRC_SYSTEM_SAMPLING_BASE)
    //   2. zx_sampler_create(sampling_resource, 0, &config{period=1ms, buffer_size=page_size}, &sampler)
    //   3. Create event, spawn thread running TestFn (signals event, busy-loops)
    //   4. Wait for event signal
    //   5. zx_sampler_start(sampler)
    //   6. ReadNRecordsContainingTid(sampler, tid, 10) — polls for 30s
    //   7. zx_sampler_stop(sampler)
    //   8. Signal thread to exit, join
    //   9. zx_handle_close(sampler)

    todo!("implement test");
}

/// C++: TEST(ThreadSampler, SamplerLifetime)
/// Verifies that only one sampler can exist at a time. Creating a
/// second sampler while the first exists returns ZX_ERR_ALREADY_EXISTS.
/// After closing the first, a new one can be created.
#[test]
fn sampler_lifetime() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. zx_sampler_create → sampler1 (OK)
    //   2. zx_sampler_create → sampler2 (ZX_ERR_ALREADY_EXISTS)
    //   3. zx_handle_close(sampler1)
    //   4. zx_sampler_create → sampler3 (OK)
    //   5. zx_handle_close(sampler3)

    todo!("implement test");
}

/// C++: TEST(ThreadSampler, InvalidOptions)
/// Creating a sampler with non-zero options returns ZX_ERR_INVALID_ARGS.
#[test]
fn invalid_options() {
    let _ctx = TestContext::new();
    // C++ test:
    //   zx_sampler_create(sampling_resource, 1 /*non-zero options*/, &config, &sampler)
    //   ASSERT_EQ(create_res, ZX_ERR_INVALID_ARGS);

    todo!("implement test");
}

/// C++: TEST(ThreadSampler, DroppedSampler)
/// Drops a sampler mid-session (close handle while sampling is active),
/// then creates a new one and verifies it works.
#[test]
fn dropped_sampler() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. Create sampler, spawn thread, start sampling
    //   2. zx_handle_close(sampler) — drop mid-session
    //   3. Create new sampler
    //   4. Start, read 10 records, stop
    //   5. Clean up thread

    todo!("implement test");
}

/// C++: TEST(ThreadSampler, NonRunningThread)
/// Creates a sampler, creates a thread (Init but not Start), starts
/// sampling, then starts the thread. Samples from the now-running
/// thread should appear.
#[test]
fn non_running_thread() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. zx_sampler_create
    //   2. TestThread::Init("NonRunningThread") — thread created but not started
    //   3. zx_sampler_start(sampler)
    //   4. TestThread::Start(threads_test_wait_loop, event_handle)
    //   5. Wait for event signal
    //   6. ReadNRecordsContainingTid(sampler, tid, 10)
    //   7. Stop, signal thread, wait

    todo!("implement test");
}

/// C++: TEST(ThreadSampler, HighFrequency)
/// Stress test: 4 threads, 50us sample period, 1024-page buffer.
/// Reads at least 10 records total.
#[test]
fn high_frequency() {
    let _ctx = TestContext::new();
    // C++ test:
    //   1. config.period = 50us, buffer_size = 1024 * page_size
    //   2. Create sampler
    //   3. Spawn 4 threads (TestFn), wait for each event
    //   4. Start sampler
    //   5. ReadNRecords(sampler, 10)
    //   6. Stop, signal all threads, join all

    todo!("implement test");
}
