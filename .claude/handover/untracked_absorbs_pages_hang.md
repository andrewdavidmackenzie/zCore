# PagerWriteback.UntrackedAbsorbsPages Hang Investigation

## Status
The last remaining PagerWriteback test. All other 58 PagerWriteback tests pass.

## Test Description
`UntrackedAbsorbsPages` (pager-writeback.cc:8021) creates an unbounded VMO
with `CreateUnboundedVmo(2 * page_size, ZX_VMO_TRAP_DIRTY)`, then:

1. Verifies no dirty ranges initially
2. Reads pages 2-9 via `vmo.read()` — expects zeros (beyond stream_size)
3. **TestThread** writes to pages 4, 5, 8 via three `vmo.write()` calls
4. Main thread receives 3 DIRTY requests, acks each with `DirtyPages`
5. `t.Wait()` — joins the TestThread
6. Queries dirty ranges (expects [4,6) and [8,9))
7. `set_stream_size(7*page)` — pages [2,7) become tracked dirty zeros
8. `set_stream_size(1*page)` — everything absorbed back

The similar test `UntrackedBeyondStreamSize` (same VMO creation, no TestThread
writes) passes.

## Hang Point
The test hangs at step 5: `t.Wait()` (thrd_join). The TestThread completed
all 3 `vmo.write` calls successfully but never finishes its C runtime exit.

## Data Gathered

### Serial log markers confirm:
- All 3 `pager_preflight_write` calls complete with `Ok(())`
- All 3 `sys_vmo_write` calls complete with `Ok(())`
- `zx_thread_exit` IS called (syscall `THREAD_EXIT` logged from thread 1150)
- `run_user` loop exits correctly for thread 1150
- `handle_exception(ThreadExiting)` dispatches and completes

### Critical finding:
The TestThread calls **plain `zx_thread_exit`** (Sys::THREAD_EXIT, syscall 7)
instead of the combined **`zx_futex_wake_handle_close_thread_exit`**
(Sys::FUTEX_WAKE_HANDLE_CLOSE_THREAD_EXIT).

The combined syscall is what the C library's `thrd_join` depends on — it
atomically:
1. Stores the new value to the futex word
2. Wakes the futex (unblocking `thrd_join`)
3. Closes the thread handle
4. Exits the thread

With plain `zx_thread_exit`, the futex is never woken, so `thrd_join` blocks
forever. The thread terminates (state → Dead, TASK_TERMINATED signaled), but
the joining thread is stuck on the futex, not on the thread signal.

### Why plain THREAD_EXIT instead of the combined syscall?
This is the key unanswered question. Normal C thread exit goes through musl's
`__pthread_exit` → `__unmapself` → `FUTEX_WAKE_HANDLE_CLOSE_THREAD_EXIT`.
The TestThread calling plain `THREAD_EXIT` suggests something disrupted the
normal exit path. Possible causes:

1. **Stack corruption**: The 3 `vmo.write` calls to pages beyond stream_size
   on a 1GB VMO might cause a stack overflow or corruption in the syscall
   handler (the `dirty_pages` array is 256KB = 262144 bytes).

2. **TLS corruption**: The `set_trap_dirty(262144)` + `set_unbounded(262144,2)`
   calls each allocate 256KB arrays. If these allocations cause the kernel heap
   to grow into a region that overlaps with user TLS, the thread's exit
   metadata could be corrupted.

3. **Signal/exception during exit**: Something triggers a non-standard exit
   path between the lambda returning and the C runtime's exit cleanup.

4. **Different C runtime behavior**: The TestThread might use a different
   thread creation path that doesn't set up the combined exit syscall.

### Tests that DO work with TestThread:
- `DirtyDuringWriteback` — TestThread writes 1 byte, joins successfully
- `DirtyRequestsOverlap` — TestThread writes 5 pages, joins successfully
- `LargeWriteBatched` — TestThread writes 2048 pages, joins successfully

All of these use regular (non-unbounded) VMOs. The difference is the
unbounded VMO with its 1GB capacity and 256KB `dirty_pages` allocation.

## Reproduction
```bash
make core-tests CORE_TESTS_FILTER='PagerWriteback.UntrackedAbsorbsPages' CORE_TESTS_TIMEOUT=30
```

Hangs with both `-smp 1` and `-smp 8`. Also hangs with reduced VMO capacity
(64 pages instead of 262144).

## Skip list entry
```
PagerWriteback.UntrackedAbsorbsPages
```

## Next steps
1. **Check if reducing VMO capacity fixes the issue**: The 64-page test also
   hung, which suggests the issue isn't the allocation size. But verify by
   checking if the thread exit path differs.

2. **Add markers in musl's thread exit**: Check if `__pthread_exit` or
   `__unmapself` is reached. If not, the thread is taking an abnormal exit
   path.

3. **Compare with working TestThread tests**: Run `DirtyDuringWriteback`
   with the same debug markers to see if it calls
   `FUTEX_WAKE_HANDLE_CLOSE_THREAD_EXIT` (expected) or `THREAD_EXIT`.

4. **Check if the TestThread's C function pointer is correct**: The lambda
   captures `[&]` — verify the captures don't include a dangling reference
   after the VMO operations.

5. **Try with a simpler unbounded VMO test**: Create an unbounded VMO,
   start a TestThread that does a single `vmo.write` beyond stream_size,
   and check if `t.Wait()` completes.

## Related files
- `zCore/zircon-syscall/src/lib.rs` — `pager_preflight_write`, VMO_WRITE handler
- `zCore/zircon-object/src/vm/vmo/mod.rs` — `update_stream_size_dirty_tracking`
- `zCore/zircon-object/src/signal/pager.rs` — unbounded VMO creation (cap=256K pages)
- `zCore/zircon-loader/src/zircon.rs` — `run_user` loop, thread exit handling
- `/tmp/pager-writeback.cc:8021` — C++ test source
