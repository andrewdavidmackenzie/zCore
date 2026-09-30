# Hardware Test Log

Records of tests on real hardware with commit, build options, and results.

## x86_64 - ThinkPad P1 Gen 3 (Intel Core i7-10750H, 6c/12t)

| Date | Commit | Target | Cores | Branch | Result | Notes |
|------|--------|--------|-------|--------|--------|-------|
| 2026-09-27 | master (66c31a52) | x86-laptop | 1 | master | Hangs at SMP | Master has x2APIC bug - never gets past AP boot. Shell was NEVER tested on this HW. |
| 2026-09-26 | 82f5afbc | x86-laptop | 8 | x86_laptop_fix_295 | SMP boots, no shell | All 7 APs start, executor runs, no `/ #` prompt |
| 2026-09-26 | 82f5afbc | x86-laptop | 1 | x86_laptop_fix_295 | No shell | `executor run!` then nothing. Same as 8 cores. |

## aarch64 - Raspberry Pi 400 (BCM2711, 4c)

| Date | Commit | Target | Cores | Branch | Result | Notes |
|------|--------|--------|-------|--------|--------|-------|
| 2026-09-25 | aa429dbc | raspi400-uefi | 4 | master | Boots to shell | UEFI boot with Zirconia logo, busybox shell works |

## QEMU x86_64

| Date | Commit | Target | Cores | Branch | Result | Notes |
|------|--------|--------|-------|--------|--------|-------|
| 2026-09-26 | 82f5afbc | qemu-x86_64 | 1 | x86_laptop_fix_295 | PASS | Shell, echo, help all work |
| 2026-09-26 | 82f5afbc | qemu-x86_64 | 4 | x86_laptop_fix_295 | PASS | Shell works |
| 2026-09-26 | 82f5afbc | qemu-x86_64 | 5 | x86_laptop_fix_295 | PASS | Shell works |
| 2026-09-26 | 82f5afbc+STARTED fix | qemu-x86_64 | 8 | x86_laptop_fix_295 | PASS | Shell, echo, help verified |

## QEMU aarch64

| Date | Commit | Target | Cores | Branch | Result | Notes |
|------|--------|--------|-------|--------|--------|-------|
| 2026-09-26 | 82f5afbc | qemu-aarch64 | 4 | x86_laptop_fix_295 | PASS | boot-test passes |

## Key observations

- x86 real hardware (ThinkPad): boots kernel, all cores initialize, but busybox shell never appears. Same with 1 core or 8 cores.
- QEMU x86_64: all core counts work including 8 cores (with STARTED fix).
- The difference: QEMU uses UART for console, real HW uses fb-console.
- Need to test: was the shell ever working on real x86 HW with the x86-laptop target?
