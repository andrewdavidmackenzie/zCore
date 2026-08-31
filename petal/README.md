# petal

A minimal, controlled userspace for testing the zCore Zircon kernel.

## What is petal?

In Fuchsia's architecture, "Zircon" is the kernel and everything above it is
userspace. The full Fuchsia userspace includes drivers, filesystems, networking,
the component framework, and package management -- all running as userspace
services.

**petal** is a small alternative to that full stack. It consists of simple test
programs that exercise Zircon syscalls directly, without requiring the Fuchsia
component framework or any other Fuchsia infrastructure. Think of it as the
Zircon-mode equivalent of the Linux-mode busybox rootfs.

The name comes from the Fuchsia flower -- a petal is a small part of the whole.

## How it works

1. petal programs are compiled as Fuchsia userspace binaries
2. They are packaged into a ZBI (Zircon Boot Image)
3. The zCore kernel boots and launches **userstart** (the first userspace
   process, our Rust reimplementation of Fuchsia's `userboot`)
4. userstart loads petal programs from the ZBI bootfs and runs them
5. Each program exercises specific Zircon kernel functionality via syscalls

## Current status

petal is not yet functional. The following work is needed:

- [#121](https://github.com/andrewdavidmackenzie/zCore/issues/121) --
  Implement userstart + vDSO + ZBI construction
- [#89](https://github.com/andrewdavidmackenzie/zCore/issues/89) --
  Create test programs and add Zircon boot test to CI

## Not just for petal

userstart is not petal-specific. It can launch any ZBI-based userspace,
including real Fuchsia binaries (see
[#122](https://github.com/andrewdavidmackenzie/zCore/issues/122) for Fuchsia
ABI compatibility work).
