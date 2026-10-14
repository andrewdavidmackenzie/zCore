# Handover: Split core_tests to contain ONLY C++ core-test ports

## Goal

The `zircon-tests` crate has a `core_tests` submodule that should contain
**exactly** the 1:1 ports of the 1778 Fuchsia C++ `core-tests-standalone`
tests. Currently it contains ~2890 tests — 1967 C++ ports plus 923
zCore-original tests mixed into the same files.

**Task:** Extract the 923 non-C++ tests from `core_tests/` into top-level
modules, so `core_tests/` contains only faithful C++ ports.

## Branch

Start from `unignore_tests_668` branch (PR #679). Or start fresh from
master after #679 is merged.

## Current state

- `zCore/zircon-tests/src/core_tests/` — 101 `.rs` files + `mod.rs`
- 40 files are "pure" C++ ports (no splitting needed)
- 60 files are "mixed" (contain both C++ ports and zCore-original tests)
- C++ port tests are identified by `/// C++: TEST(...)` doc comments
- Tests WITHOUT this comment are zCore-original and must move out

## How to identify C++ port tests

Every C++ port test function has a doc comment immediately above the
`#[test]` attribute:

```rust
/// C++: TEST(ChannelTest, ReadBasic)
#[test]
fn read_basic() { ... }
```

or:

```rust
/// C++: VMO_VMAR_TEST(Pager, SinglePageTest)
#[test]
fn pager_single_page_test_vmo() { ... }
```

Tests WITHOUT `/// C++:` in the preceding doc comments are zCore-original.

## Files that need splitting (60 files, sorted by extra test count)

| File | C++ ports | Extra (move out) |
|------|-----------|-----------------|
| object_info.rs | 249 | 197 |
| pager.rs | 391 | 172 |
| thread.rs | 56 | 59 |
| vmo.rs | 71 | 53 |
| socket.rs | 42 | 30 |
| vmar.rs | 58 | 28 |
| process.rs | 28 | 23 |
| exception.rs | 2 | 22 |
| job.rs | 30 | 22 |
| channel.rs | 22 | 21 |
| property.rs | 20 | 15 |
| resource.rs | 36 | 15 |
| bti.rs | 13 | 14 |
| fifo.rs | 22 | 14 |
| handle.rs | 10 | 14 |
| object_wait.rs | 12 | 14 |
| timer.rs | 14 | 14 |
| vmo_clone.rs | 10 | 14 |
| stream.rs | 33 | 13 |
| syscall_handle.rs | 3 | 12 |
| port.rs | 22 | 11 |
| event.rs | 7 | 10 |
| port_stress.rs | 12 | 10 |
| profile.rs | 30 | 9 |
| event_pair.rs | 2 | 8 |
| memory_mapping.rs | 8 | 8 |
| vmo_slice.rs | 19 | 8 |
| interrupt.rs | 19 | 7 |
| clock.rs | 5 | 6 |
| syscall_port.rs | 17 | 6 |
| debuglog.rs | 9 | 5 |
| rseq.rs | 11 | 5 |
| syscall_signal.rs | 13 | 5 |
| syscall_transfer_data.rs | 1 | 5 |
| syscall_vmo.rs | 9 | 5 |
| bad_access.rs | 1 | 4 |
| futex.rs | 23 | 4 |
| syscall_channel.rs | 135 | 4 |
| c11_mutex.rs | 2 | 3 |
| c11_thread.rs | 3 | 3 |
| conditional_variable.rs | 0 | 3 |
| sync_mutex.rs | 2 | 3 |
| syscall_fifo.rs | 25 | 3 |
| bad_segsel.rs | 3 | 2 |
| completion_wrapper.rs | 4 | 2 |
| mutex_wrapper.rs | 2 | 2 |
| skipped.rs | 17 | 2 |
| sync_condition.rs | 0 | 2 |
| channel_internal.rs | 3 | 1 |
| cpp_thread.rs | 0 | 1 |
| cpu_features.rs | 1 | 1 |
| hotplug.rs | 0 | 1 |
| interrupt_crash.rs | 0 | 1 |
| pthread.rs | 5 | 1 |
| standalone_elf_load.rs | 0 | 1 |
| syscall_socket.rs | 42 | 1 |
| system_cpu.rs | 5 | 1 |
| version_test.rs | 3 | 1 |
| vmo_clone2.rs | 43 | 1 |
| vmo_zero.rs | 14 | 1 |

## Files with 0 C++ ports (move entirely out of core_tests)

These files have `/// C++:` in the grep but the script found 0 actual
test functions with the comment. They should move entirely to top-level:

- `conditional_variable.rs` (0 C++ + 3 extra)
- `cpp_thread.rs` (0 C++ + 1 extra)
- `hotplug.rs` (0 C++ + 1 extra)
- `interrupt_crash.rs` (0 C++ + 1 extra)
- `standalone_elf_load.rs` (0 C++ + 1 extra)
- `sync_condition.rs` (0 C++ + 2 extra)

Verify: check these files manually. The grep may have matched a comment
that is not directly above a `#[test]` attribute.

## Splitting procedure for each mixed file

For each file in the table above:

1. **Read the file** and identify every `#[test]` / `#[async_std::test]`
   function.

2. **Classify each test function:**
   - Has `/// C++:` in the 1-10 lines above `#[test]` → stays in core_tests
   - No `/// C++:` → moves to a new top-level module

3. **Identify shared helpers:**
   - Functions/constants/imports used by BOTH C++ port tests and extra tests
   - These should go in `helpers.rs` or be duplicated

4. **Create the new top-level module:**
   - Name it `{original_name}_extra.rs` or a descriptive name
   - Move the non-C++ test functions there
   - Add necessary `use` imports

5. **Update `core_tests/{original_name}.rs`:**
   - Remove the moved functions
   - Keep only C++ port tests and their helpers

6. **Update `lib.rs`:**
   - Add `mod {new_module_name};` to the zCore-original section

7. **Verify:** `cargo test -p zircon-tests --no-run` after each file

## Suggested naming for extracted modules

For files where the extra tests are closely related to the C++ tests
(same kernel object), use `{name}_extra.rs`:
- `core_tests/channel.rs` → extract to `channel_extra.rs`
- `core_tests/vmo.rs` → extract to `vmo_extra.rs`

For files where the extra tests are already in a `syscall_*` or `*_ops`
pattern, consider merging with existing top-level modules if they exist.

## Verification after all splitting

1. `cargo test -p zircon-tests --no-run` — compiles
2. `cargo test -p zircon-tests -- --list 2>&1 | grep '^core_tests::' | wc -l`
   — should be approximately 1967 (the VMO_VMAR_TEST duplicates push it
   above 1778)
3. `cargo test -p zircon-tests -- --list 2>&1 | wc -l` — should still be 3209
4. `cargo check-style` — EXIT 0
5. `cargo test -p zircon-object --features "libos,aspace-separate"` — 125/0/8

## Audit: verify 1:1 C++ mapping after split

After the split, run this audit to verify every C++ test has a Rust equivalent:

```bash
# Extract all C++ test names from the binary inventory
cat tests/fuchsia-core-tests-from-binary.txt | sort > /tmp/cpp_tests.txt

# Extract all C++ mappings from core_tests doc comments
grep -rh '/// C++:' zCore/zircon-tests/src/core_tests/*.rs | \
  sed 's/.*TEST(\|.*TEST_F(\|.*TEST_P(\|.*VMO_VMAR_TEST(//' | \
  sed 's/).*//' | sed 's/, /\./' | sort -u > /tmp/rust_mapped.txt

# Find C++ tests missing from Rust
comm -23 /tmp/cpp_tests.txt /tmp/rust_mapped.txt > /tmp/missing.txt
echo "Missing C++ tests: $(wc -l < /tmp/missing.txt)"
cat /tmp/missing.txt
```

Expected: ~10 missing tests (all requiring unimplemented infrastructure):
1. ChannelTest.ChannelFullException
2. ChannelTest.ConcurrentReadsConsumeUniqueElements
3. ChannelTest.NoSpuriousReadableSignalWhenRacing
4. ChannelTest.ReadFaultDoesNotLeakHandles
5. ChannelTest.WaitManyIsSignaledOnAnyElementWrite
6. VmoClone2TestCase.ResizeDisjointChild
7. VmoClone2TestCase.ResizeMultipleProgressive
8. VmoClone2TestCase.ResizeOffsetChild
9. PortTest.QueuePacketLimitExceededGeneratesPolicyException
10. PortTest.TooManyObservers

For each missing test, check if it is in `core_tests/skipped.rs` as a
stub. If not, add it as a stub with `#[ignore]` and the reason.

## Analysis script

A Python script at `/var/folders/df/v9sncll51gl8mcl8rrdvv2r80000gn/T/opencode/split_tests.py`
can be used to re-analyze after changes. Run:

```bash
python3 /var/folders/df/v9sncll51gl8mcl8rrdvv2r80000gn/T/opencode/split_tests.py
```

It outputs the count of C++ ports vs extra tests per file.

## Important notes

- **Do NOT change test logic.** This is purely organizational.
- **Run `cargo check-style` and `cargo test -p zircon-object --features
  "libos,aspace-separate"` before pushing.** The CI "Clippy" job runs
  the zircon-object tests and will SIGSEGV if aspace IDs leak.
- **The `helpers.rs` module stays at crate root.** All test files access
  it via `use crate::helpers::*`.
- **Some core_tests files have file-local helper functions** (not in
  `helpers.rs`). If both C++ port tests and extra tests use them, either
  move the helper to `helpers.rs` or duplicate it.
- **VMO_VMAR_TEST expansion:** The C++ macro `VMO_VMAR_TEST(Pager, Foo)`
  generates two C++ tests: `Pager.Foo` (vmo variant) and `Pager.Foo`
  (vmar variant). In Rust, these become two separate functions like
  `pager_foo_vmo()` and `pager_foo_vmar()`. Both have `/// C++:` comments
  and both stay in core_tests. This is why the Rust count (~1967) exceeds
  the C++ count (1778).
