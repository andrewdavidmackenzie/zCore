# Fuchsia Baseline (Issue #523 Phase A)

Before attempting to boot Fuchsia userspace on zCore, we run and verify the
**real** Fuchsia kernel + userspace in QEMU from a local Fuchsia checkout.
This validates the boot recipe, gives us a reference serial log, and defines
the success markers that the zCore port will later be measured against.

## Quick start

```bash
make fuchsia-check   # doctor: verify tree/build, print artifact inventory
make fuchsia-run     # interactive: serial console on stdio (Ctrl-A x quits)
make fuchsia-test    # automated: headless boot + userspace marker checks
```

`make fuchsia-test` boots the bringup image and passes when these markers
appear **in order** in the serial log (saved to `target/fuchsia/serial.log`):

| # | Marker | Meaning |
|---|--------|---------|
| 1 | `welcome to Zircon` | Fuchsia kernel booted |
| 2 | `console: starting up` | component_manager started the console |
| 3 | `launching /boot/bin/sh` | serial shell (real userspace!) |
| 4 | `Bootup completed` | driver_manager finished binding drivers |

With KVM this takes ~8 seconds.

## The Fuchsia checkout

The scripts expect a jiri-managed Fuchsia checkout with a built product:

- `FUCHSIA_ROOT` — the jiri root (default `../fuchsia/fuchsia/fuchsia`,
  matching the layout created by `make build-fuchsia`).
- `FUCHSIA_OUT` — the build directory (default
  `$FUCHSIA_ROOT/out/bringup.x64-balanced`).

The bringup product is the right baseline: headless, boots entirely from
bootfs (no FVM/blobfs/package resolver), serial console shell enabled
(`console.shell=true`). To (re)build it:

```bash
cd $FUCHSIA_ROOT
fx set bringup.x64
fx build
```

The checkout should stay **pinned** — record the revision in any change that
updates artifacts (see `PROVENANCE.md` for the zCore-side import, Phase B).
Verify the current pin with `make fuchsia-check`.

## Boot recipe

The QEMU invocation mirrors Fuchsia's own `zircon/scripts/run-zircon` x64
defaults (see that script for the authoritative flags):

- `-machine q35,smbios-entry-point-type=32` — q35 with 32-bit SMBIOS entry
- `-fw_cfg name=etc/sercon-port,string=0` — silence SeaBIOS's terminal reset
- `-device isa-debug-exit,iobase=0xf4,iosize=0x04` — guest-requested exit
- KVM: `-enable-kvm -cpu host,migratable=no,+invtsc`, else
  `-cpu Skylake-Client,-check` (TCG, ~8x slower; the test timeout scales)
- `-kernel kernel.phys32/linux-x86-boot-shim.bin` — boot shim implementing
  the Linux boot protocol (from the build's `images.json` `qemu-kernel` entry)
- `-initrd fuchsia.zbi` — the complete boot image: kernel + zstd-compressed
  bootfs with `bin/component_manager`, `bin/sh`, drivers, `lib/ld.so.1`, …
- `-append "kernel.serial=legacy kernel.entropy-mixin=<random> kernel.halt-on-panic=true"`

Artifact paths (ZBI, boot shim) are resolved from `$FUCHSIA_OUT/images.json`
so they survive product/board output-layout changes.

## What is being verified

The bringup boot chain on real Fuchsia:

```
boot shim → physboot → Zircon kernel → userboot → component_manager
          → driver_manager/drivers (e1000, ahci, i8042, sysmem) → netsvc
          → console-launcher → /boot/bin/sh on serial
```

Reaching marker 4 proves the entire userspace stack works: the vDSO,
dynamic linking (`ld.so.1` + loader service), component_manager's async
runtime, FIDL channels between bootstrap components, and driver binding.

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `FUCHSIA_ROOT` | `../fuchsia/fuchsia/fuchsia` | jiri checkout root |
| `FUCHSIA_OUT` | `$FUCHSIA_ROOT/out/bringup.x64-balanced` | build output dir |
| `FUCHSIA_QEMU_BIN` | `qemu-system-x86_64` from PATH | QEMU binary |
| `FUCHSIA_MEMORY` | `8192` | guest memory (MB) |
| `FUCHSIA_SMP` | `4` | guest CPUs |

## Relationship to the zCore plan

This is [Phase A of issue #523](https://github.com/andrewdavidmackenzie/zCore/issues/523):
establish the baseline and verification methodology on real Fuchsia.
Phase B imports the same artifacts (`userboot.elf`, `vdso.elf`,
`fuchsia.zbi`) into `prebuilt/fuchsia/x86_64/`; Phase C boots Fuchsia's
real userboot on zCore with the same marker-based verification.
