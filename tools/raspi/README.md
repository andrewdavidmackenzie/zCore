# Raspberry Pi 4 / Pi 400 Boot (UEFI)

The `raspi400` board boots via [pftf/RPi4](https://github.com/pftf/RPi4)
UEFI firmware: the Pi GPU loads `RPI_EFI.fd`, which provides a standard
UEFI environment; the zCore UEFI stub is installed as the default boot
application and loads the kernel from the SD card.

This is the only board configuration verified on real hardware
(see `docs/hw-test-log.md`).

## Quick Start

1. Format a microSD card with a single FAT32 partition
2. Mount the SD card (e.g., `/Volumes/boot` on macOS)
3. Run:
   ```bash
   make flash CONFIG=busybox.raspi400 SD=/Volumes/boot
   ```
   (or `make raspi400-uefi-sd SD=/Volumes/boot` for the same thing)

   `make flash` downloads the pftf firmware (cached under
   `target/pftf-firmware-*`), builds the UEFI stub and kernel, and
   writes the ESP layout to the card. For a custom firmware with the
   Zirconia boot logo, build it first with `make uefi-firmware`.

4. Eject the SD card and insert into the Pi
5. Connect a serial console (see below)
6. Power on

## Product choice

The board works with any product that supports it:

- `busybox.raspi400` — Zircon + Linux, busybox shell from the rootfs
  image written to the card as `/initrd.img` (default for `make flash`
  via this README; what the hardware test log verified)
- `petal.raspi400` — Zircon only, petal shell from the embedded ZBI
  (the board's `default-product`, used by `make build`)

## Serial Console

The Pi 400 has a GPIO header. Connect a USB-to-serial adapter:

| Pi GPIO | Signal | Adapter |
|---------|--------|---------|
| Pin 6   | GND    | GND    |
| Pin 8   | TX     | RX     |
| Pin 10  | RX     | TX     |

Open a serial terminal:
```bash
# macOS
screen /dev/tty.usbserial-* 115200

# Linux
screen /dev/ttyUSB0 115200
```

## Build Manually

```bash
make build CONFIG=petal.raspi400
```

The kernel ELF lands in `target/petal.raspi400/release/kernel`.
`tools/raspi/prepare-sd-uefi.sh` shows the full SD layout
(firmware, stub, kernel, initrd, DTB).
