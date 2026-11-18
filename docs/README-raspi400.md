# Raspberry Pi 400 (BCM2711)

zCore runs on the Raspberry Pi 400 (and Pi 4B) via UEFI boot.

## Current status

**Boots to shell.** The UEFI boot path (pftf/RPi4 firmware + the zCore
UEFI stub) reaches a working busybox shell on real hardware — see
`docs/hw-test-log.md` (2026-09-25, `raspi400-uefi` entry, since merged
into the `raspi400` board).

The legacy raw/extlinux boot path (kernel8.img + custom armstub) was
removed when the `raspi400` and `raspi400-uefi` boards were merged; it
suffered from an executor hang on real hardware tracked in
[issue #259](https://github.com/andrewdavidmackenzie/zCore/issues/259)
(the same kernel worked under QEMU's `raspi4b` emulation). The
investigation notes and armstub sources are preserved in git history
and under `tools/armstub/`.

## Hardware

- **SoC**: Broadcom BCM2711, quad-core Cortex-A72 (ARMv8-A)
- **RAM**: 4 GB LPDDR4
- **UART**: PL011 (UART0) at 0xFE201000, accessible via GPIO header pins 14/15
- **Interrupt controller**: GIC-400 at 0xFF840000 (GICD at +0x1000, GICC at +0x2000)
- **Timer**: ARM Generic Timer, virtual timer (CNTV, PPI 11 = GIC IRQ 27)
- **Boot**: pftf/RPi4 UEFI firmware (EDK2) loaded by the Pi GPU boot chain

## SD card setup

The board's `flash-method` is `sd`, so `make flash` handles everything:
it downloads the pftf UEFI firmware (cached under
`target/pftf-firmware-*`), builds the UEFI stub and kernel, and writes
an ESP layout to the card.

```bash
# Format a microSD card with a single FAT32 partition, mount it, then:
make flash CONFIG=busybox.raspi400 SD=/Volumes/BOOT
```

Product choice:

- `busybox.raspi400` — Zircon + Linux, busybox shell (the configuration
  verified on hardware; the rootfs image is written to the card as
  `/initrd.img`)
- `petal.raspi400` — Zircon only, petal shell from the embedded ZBI
  (the board's `default-product`)

For a custom firmware build with the Zirconia boot logo instead of the
stock pftf release, run `make uefi-firmware` first.

See `tools/raspi/prepare-sd-uefi.sh` for the full SD layout
(firmware files, `EFI/BOOT/BOOTAA64.EFI` stub, `/kernel`,
`/initrd.img`, DTB) and `tools/raspi/README.md` for the quick start.

## Serial console

The Pi 400 has a GPIO header. Connect a USB-to-serial adapter
(3.3V TTL, e.g. CP2102 or FTDI):

| Pi GPIO | Signal | Adapter |
|---------|--------|---------|
| Pin 6   | GND    | GND     |
| Pin 8   | TX     | RX      |
| Pin 10  | RX     | TX      |

Open a serial terminal at 115200 baud:

```bash
# macOS
screen /dev/tty.usbserial-* 115200

# Linux
screen /dev/ttyUSB0 115200
```

## Boot sequence (UEFI)

1. **Pi GPU firmware** (`start4.elf` from the pftf release) initializes
   DRAM/peripherals and loads `RPI_EFI.fd` as the "kernel".
2. **RPI_EFI.fd** (EDK2) provides a standard UEFI environment with the
   Pi hardware described via DTB; it loads the default boot application.
3. **zCore UEFI stub** (`EFI/BOOT/BOOTAA64.EFI`, built from
   `tools/aarch64-uefi-stub` with the `board-raspi400` +
   `uefi-console` features) loads `/kernel` (linked at 0x80200000),
   `/initrd.img`, and the DTB, then jumps to the kernel entry with the
   UEFI memory map.
4. **Kernel** (`rust_main`) initializes logging, the memory allocator,
   PL011 UART, GIC-400, and the virtual timer, then starts the init
   process selected by the product (`ROOTPROC`).
