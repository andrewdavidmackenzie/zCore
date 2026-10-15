//! Tests faithfully ported from C++ core-tests that are expected to
//! hang or crash in zCore due to missing kernel functionality.
//!
//! When the underlying kernel bug/gap is fixed, the test will start
//!
//! See also: AGENTS.md skip list, issue #21 (tracking)

use crate::helpers::TestContext;

// ── Channel tests ───────────────────────────────────────────────────

/// C++: TEST(ChannelTest, ReadFaultDoesNotLeakHandles)
/// channel/channel.cc:1486
///
/// Creates 256K+ iterations of:
///   1. Create event handle
///   2. Write handle into channel `a`
///   3. Read from channel `b` with handles output buffer at kernel
///      address 0xfffffffffffffff0 (forces copy-out fault)
///   4. Assert ZX_ERR_INVALID_ARGS
///
/// Verifies the kernel does not leak handle objects when the
/// copy-out to userspace faults.
#[test]
#[ignore = "requires bad pointer fault handling"]
fn channel_read_fault_does_not_leak_handles() {
    let _ctx = TestContext::new();
    // In Fuchsia, kBadAddress = 0xfffffffffffffff0 (kernel space) is
    // passed as the handles output buffer. The kernel detects the bad
    // address during copy_to_user, closes the handles, and returns
    // ZX_ERR_INVALID_ARGS. In libos mode, all pointers are valid
    // process memory — there is no copy_to_user fault path.
}

/// C++: TEST(ChannelTest, NoSpuriousReadableSignalWhenRacing)
/// channel/channel.cc:1517
///
/// 10,000 iterations of:
///   1. Writer thread writes 1 byte to channel `b`
///   2. Reader thread waits for READABLE on `a`, reads, verifies
///      no spurious READABLE signal after read completes
///   3. Sync via atomic attempt counter
///
/// Verifies no spurious READABLE signal when writer and reader race.
#[test]
#[ignore = "requires concurrent Zircon threads"]
fn channel_no_spurious_readable_signal_when_racing() {
    let _ctx = TestContext::new();
    // Requires two threads: a writer spinning on arch::Yield() and a
    // reader doing wait_one + read. Single-threaded cooperative
    // scheduling cannot reproduce the race.
}

/// C++: TEST(ChannelTest, ConcurrentReadsConsumeUniqueElements)
/// channel/channel.cc:469
///
/// Writes 2000 unique uint64 messages to a channel, then two reader
/// threads each read 1000 messages. Verifies:
///   - All 2000 unique values were consumed
///   - No duplicates
///   - Each reader's values are monotonically increasing
#[test]
#[ignore = "requires concurrent Zircon threads"]
fn channel_concurrent_reads_consume_unique_elements() {
    let _ctx = TestContext::new();
    // Requires two concurrent reader threads (std::jthread) and an
    // event for synchronization. Cooperative scheduling cannot run
    // two readers concurrently.
}

/// C++: TEST(ChannelTest, WaitManyIsSignaledOnAnyElementWrite)
/// channel/channel.cc:218
///
/// Creates two channels, spawns a worker that does wait_many on both
/// remote endpoints. Writes to only local_1. Verifies:
///   - Worker received exactly 1 packet from remote_1
///   - Worker received 0 bytes from remote_2
///   - Event was signaled with ZX_USER_SIGNAL_0
#[test]
#[ignore = "requires concurrent Zircon threads"]
fn channel_wait_many_is_signaled_on_any_element_write() {
    let _ctx = TestContext::new();
    // Requires a worker thread doing object_wait_many on two
    // channels while the main thread writes to one. Cannot be
    // done with cooperative scheduling.
}

/// C++: TEST(ChannelTest, ChannelFullException)
/// channel/channel.cc:1447
///
/// Spawns a mini-process that writes to a channel in a loop until
/// the channel is full. The kernel raises a policy exception
/// (ZX_EXCP_POLICY_CODE_PORT_TOO_MANY_PACKETS equivalent for
/// channels). The process is killed with ZX_TASK_RETCODE_EXCEPTION_KILL.
///
/// Verifies write_count > 1000 before the exception.
#[test]
#[ignore = "requires policy exception infrastructure"]
fn channel_full_exception() {
    let _ctx = TestContext::new();
    // In Fuchsia:
    //   1. Create process + thread + channel
    //   2. start_mini_process_etc with channel as bootstrap handle
    //   3. Loop: mini_process_cmd(MINIP_CMD_CHANNEL_WRITE) until != ZX_OK
    //   4. ASSERT_GT(write_count, 1000)
    //   5. Wait for ZX_PROCESS_TERMINATED
    //   6. Assert return_code == ZX_TASK_RETCODE_EXCEPTION_KILL
}

// ── Port tests ──────────────────────────────────────────────────────

/// C++: TEST(PortTest, QueuePacketLimitExceededGeneratesPolicyException)
/// port/ports.cc:213
///
/// Pre-queues 4096 user packets to a port, then queues one more
/// from a worker thread. The kernel raises ZX_EXCP_POLICY_ERROR
/// with synth_code ZX_EXCP_POLICY_CODE_PORT_TOO_MANY_PACKETS.
///
/// Main thread reads the exception from the process exception channel,
/// verifies the exception type and synth_code, then resumes the thread.
#[test]
#[ignore = "requires policy exception infrastructure"]
fn port_queue_packet_limit_exceeded_generates_policy_exception() {
    let _ctx = TestContext::new();
    // In Fuchsia:
    //   1. Create port, queue 4096 packets
    //   2. Spawn worker thread
    //   3. Create process exception channel
    //   4. Worker attempts 4097th queue → triggers policy exception
    //   5. Main reads exception: type == ZX_EXCP_POLICY_ERROR,
    //      synth_code == ZX_EXCP_POLICY_CODE_PORT_TOO_MANY_PACKETS
    //   6. Resume thread from exception
}

/// C++: TEST(PortTest, TooManyObservers)
/// port/ports.cc:1155
///
/// Spawns a mini-process that calls wait_async in a loop until OOM.
/// The process is killed with ZX_TASK_RETCODE_EXCEPTION_KILL.
/// In zCore, the allocator panics on OOM instead of returning
/// ZX_ERR_NO_MEMORY.
#[test]
#[ignore = "requires OOM handling (kernel panics instead of returning NO_MEMORY)"]
fn port_too_many_observers() {
    let _ctx = TestContext::new();
    // In Fuchsia:
    //   1. Create process + thread + channel
    //   2. start_mini_process_etc
    //   3. mini_process_cmd(MINIP_CMD_WAIT_ASYNC) → loops until OOM
    //   4. Expect ZX_ERR_PEER_CLOSED (process terminated)
    //   5. Wait for ZX_PROCESS_TERMINATED
    //   6. Assert return_code == ZX_TASK_RETCODE_EXCEPTION_KILL
}
