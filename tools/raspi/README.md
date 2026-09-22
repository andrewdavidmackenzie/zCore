# Raspberry Pi 4 / Pi 400 Boot

## Quick Start

1. Format a microSD card with a single FAT32 partition
2. Mount the SD card (e.g., `/Volumes/boot` on macOS)
3. Run:
   ```bash
   make raspi400-sd SD=/Volumes/boot
   ```
4. Eject the SD card and insert into the Pi
5. Connect a serial console (see below)
6. Power on

## Serial Console

The Pi 400 has a GPIO header. Connect a USB-to-serial adapter:

| Pi GPIO | Signal | Adapter |
|---------|--------|---------|
| Pin 6   | GND    | GND     |
| Pin 8   | TX     | RX      |
| Pin 10  | RX     | TX      |

Open a serial terminal:
```bash
# macOS
screen /dev/tty.usbserial-* 115200

# Linux
screen /dev/ttyUSB0 115200
```

## What You'll See

The kernel boots and prints log messages to the serial console:
```
[INFO] DTB at 0x..., size=...
[INFO] DTB memory: base=0x0, size=0x...
[INFO] Drivers: UART=0xfe201000, GIC=0xff840000
...
```

The kernel boots in Zircon mode with the petal shell. You should see
the shell prompt after the boot log messages.

## Files on the SD Card

| File | Source | Purpose |
|------|--------|---------|
| `start4.elf` | Pi firmware | GPU firmware, loads kernel |
| `fixup4.dat` | Pi firmware | GPU memory fixup |
| `bcm2711-rpi-4-b.dtb` | Pi firmware | Device tree for Pi 4B/Pi 400 |
| `bcm2711-rpi-400.dtb` | Pi firmware | Device tree for Pi 400 |
| `config.txt` | zCore | Boot configuration |
| `kernel8.img` | zCore build | Kernel binary |
| `overlays/disable-bt.dtbo` | Pi firmware | Remap PL011 to GPIO |

## config.txt

Key settings:
- `arm_64bit=1` -- boot in AArch64 mode
- `enable_uart=1` -- enable UART output
- `dtoverlay=disable-bt` -- give PL011 UART to GPIO pins (not Bluetooth)
- `kernel=kernel8.img` -- kernel filename

## Build Manually

```bash
make raspi400-build
cp target/aarch64-raspi400/release/zcore.bin /path/to/sd/kernel8.img
```
