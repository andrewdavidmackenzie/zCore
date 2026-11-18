//! petal test-runner — runs all petal user-mode test binaries in one boot.
//!
//! The single-boot CI runner for the petal user-mode test suite (issue
//! #706): instead of booting QEMU once per test binary, this program is
//! the init process and `zx_debug_exec`s each test binary from the
//! rootfs sequentially. Results are printed to the debug console with
//! START/END markers around each test:
//!
//! ```text
//! === petal-test-runner: START counter-test ===
//!   counter_test: create_initial PASS
//!   ...
//! === petal-test-runner: END counter-test ===
//! ...
//! === petal-test-runner: DONE ===
//! ```
//!
//! Per-test pass/fail is reported by the tests themselves (`PASS` /
//! `FAIL: ...` lines, same output the per-binary `petal-test.sh`
//! parses). This runner reports exec errors (binary missing, exec
//! failed) and hangs are detectable as a START without a matching END.
//!
//! The test list must match the `[[bin]]` entries in `petal/Cargo.toml`
//! (minus `hello`, `shell`, and this runner); a stale entry shows up as
//! an EXEC-FAIL NOT_FOUND in the results.

#![no_std]
#![no_main]

extern crate alloc;
extern crate petal;

use zx::sys::*;

const TESTS: &[&str] = &[
    "alloc-test",
    "bad-access-test",
    "bad-syscall-test",
    "channel-call-test",
    "channel-internal-test",
    "channel-test",
    "clock-ops-test",
    "clock-test",
    "counter-test",
    "cprng-test",
    "cpu-mask-profile-test",
    "debuglog-test",
    "debug-test",
    "event-pair-test",
    "exception-extra-test",
    "exceptions-arch-test",
    "exception-test",
    "fifo-test",
    "futex-test",
    "handle-wait-test",
    "hotplug-test",
    "iovec-test",
    "job-test",
    "memory-priority-profile-test",
    "mini-process-test",
    "object-info-extra-test",
    "object-info-test",
    "object-wait-extra-test",
    "object-wait-test",
    "port-test",
    "process-mem-test",
    "process-shared-test",
    "process-test",
    "profile-test",
    "socket-test",
    "sync-completion-test",
    "syscall-channel-extra-test",
    "syscall-channel-test",
    "syscall-fifo-extra-test",
    "syscall-fifo-test",
    "syscall-generation-test",
    "syscall-handle-extra-test",
    "syscall-handle-test",
    "syscall-object-test",
    "syscall-port-extra-test",
    "syscall-port-test",
    "syscall-signal-extra-test",
    "syscall-socket-extra-test",
    "syscall-socket-test",
    "syscall-transfer-data-extra-test",
    "syscall-transfer-data-test",
    "syscall-vmo-extra-test",
    "syscall-vmo-test",
    "system-cpu-test",
    "thread-extra-test",
    "thread-sampler-test",
    "thread-test",
    "timer-extra-test",
    "timer-test",
    "user-exceptions-test",
    "vdso-test",
    "vmo-discardable-test",
    "vmo-extra-test",
    "vmo-test",
    "wake-reports-test",
];

fn write(s: &[u8]) {
    zx::debug_write(s);
}

#[no_mangle]
pub fn main() {
    write(b"=== petal-test-runner: starting (");
    write(alloc::format!("{}", TESTS.len()).as_bytes());
    write(b" tests) ===\n");

    let mut exec_failures = 0;
    for (i, name) in TESTS.iter().enumerate() {
        write(b"\n=== petal-test-runner: START [");
        write(alloc::format!("{}/{}", i + 1, TESTS.len()).as_bytes());
        write(b"] ");
        write(name.as_bytes());
        write(b" ===\n");

        let path = alloc::format!("/bin/{}", name);
        let status = zx::sys::debug_exec(&path);

        if status != ZX_OK {
            write(b"=== petal-test-runner: EXEC-FAIL ");
            write(name.as_bytes());
            write(b" status=");
            write(alloc::format!("{}", status).as_bytes());
            write(b" ===\n");
            exec_failures += 1;
        } else {
            write(b"=== petal-test-runner: END ");
            write(name.as_bytes());
            write(b" ===\n");
        }
    }

    write(b"\n=== petal-test-runner: DONE (exec failures: ");
    write(alloc::format!("{}", exec_failures).as_bytes());
    write(b") ===\n");
}
