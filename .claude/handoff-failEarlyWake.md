# Handoff: Pager.FailEarlyWake Intermittent Hang

## Symptom
- Passes alone 5/5 times
- Passes in Pager.* suite
- Hangs intermittently in full suite (~1 in 4 runs)
- Reproduces with just `Pager.FailAfterDetach:Pager.FailEarlyWake` (~1 in 5)
- When hung: all 8 CPUs idle in `wait_for_interrupt` — no tasks in any executor

## Data gathered
- GDB on ThinkPad confirms all CPUs idle (no lock contention, no spinning)
- Serial log shows FailEarlyWake's pager thread (1179) starts, then silence
- No FatalPageFault, no thread death, no errors after the test starts
- The second TestThread (CheckVmo) is never created or never logged

## What FailEarlyWake does
1. Creates 3-page VMO, starts background PageFaultHandler (supply_limit=0)
2. Starts TestThread doing `CheckVmo(0, 3)` (reads 3 pages via vmo_read)
3. Waits for thread to block on pager
4. Supplies page 0, waits for re-block
5. Fails page 1 → thread should wake with error

## What FailAfterDetach does (prior test)
1. Creates 11-page VMO
2. Starts TestThread doing `Commit(0, 11)` (VMO_OP_COMMIT)
3. Waits for page read request
4. Detaches VMO → commit thread should fail
5. Tries FailPages on detached VMO (should fail with INVALID_ARGS)
6. Cleanup: ~UserPager signals shutdown, ~TestThread joins threads

## Hypothesis
The prior test's cleanup (FailAfterDetach) doesn't fully complete
before FailEarlyWake starts. The background pager thread or the
commit thread from FailAfterDetach may still be running when
FailEarlyWake creates its pager. This could cause:
- Port packets from the old test to be delivered to the new test
- Thread wakers from the old test to interfere
- Outstanding requests from the old test to absorb new requests

## Next steps
1. Add warn! logging to `StartTaggedPageFaultHandler`, `TestThread::Start`,
   and `TestThread::WaitForBlocked` paths (in the kernel, not the C++ test)
   to see exactly which syscalls are made and when
2. Check if the main test thread is blocked on `port.wait()` — if so,
   no READ request was sent because the CheckVmo thread never faulted
3. Check if CheckVmo's vmo_read finds the pages already committed
   (stale pages from FailAfterDetach? — unlikely, different VMO)
4. Add scan_notifications logging to see if there ARE tasks but they're
   not being polled

## Test on ThinkPad
```bash
ssh thinkpad 'cd ~/RustroverProjects/zCore && make core-test-run T="Pager.FailAfterDetach:Pager.FailEarlyWake" CORE_TEST_TIMEOUT=30'
```
