//! Thread sampler tests.
//!
//! 1:1 ports of Fuchsia's zircon/system/utest/core/thread-sampler/thread-sampler.cc.
//!
//! The thread sampler (zx_sampler_create/start/stop/read) is a kernel-level
//! profiling facility that periodically samples thread register state.
//! zCore does not implement the sampler syscalls or the sampling resource.
//! All tests are ignored until the sampler infrastructure is added.

/// C++: TEST(ThreadSampler, StartStop)
/// Creates a sampler, spawns a thread, starts sampling, reads at least
/// 10 records containing the thread's TID, stops, and cleans up.
#[test]
#[ignore = "zCore does not implement zx_sampler_create/start/stop/read syscalls (#715)"]
fn start_stop() {}

/// C++: TEST(ThreadSampler, SamplerLifetime)
/// Verifies that only one sampler can exist at a time. Creating a
/// second sampler while the first exists returns ZX_ERR_ALREADY_EXISTS.
/// After closing the first, a new one can be created.
#[test]
#[ignore = "zCore does not implement zx_sampler_create/start/stop/read syscalls (#715)"]
fn sampler_lifetime() {}

/// C++: TEST(ThreadSampler, InvalidOptions)
/// Creating a sampler with non-zero options returns ZX_ERR_INVALID_ARGS.
#[test]
#[ignore = "zCore does not implement zx_sampler_create/start/stop/read syscalls (#715)"]
fn invalid_options() {}

/// C++: TEST(ThreadSampler, DroppedSampler)
/// Drops a sampler mid-session (close handle while sampling is active),
/// then creates a new one and verifies it works.
#[test]
#[ignore = "zCore does not implement zx_sampler_create/start/stop/read syscalls (#715)"]
fn dropped_sampler() {}

/// C++: TEST(ThreadSampler, NonRunningThread)
/// Creates a sampler, creates a thread (Init but not Start), starts
/// sampling, then starts the thread. Samples from the now-running
/// thread should appear.
#[test]
#[ignore = "zCore does not implement zx_sampler_create/start/stop/read syscalls (#715)"]
fn non_running_thread() {}

/// C++: TEST(ThreadSampler, HighFrequency)
/// Stress test: 4 threads, 50us sample period, 1024-page buffer.
/// Reads at least 10 records total.
#[test]
#[ignore = "zCore does not implement zx_sampler_create/start/stop/read syscalls (#715)"]
fn high_frequency() {}
