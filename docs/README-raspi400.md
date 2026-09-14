# Raspberry Pi 400 (BCM2711)

zCore runs on the Raspberry Pi 400 (and Pi 4B) in Zircon personality mode with the petal shell.

## Hardware

- **SoC**: Broadcom BCM2711, quad-core Cortex-A72 (ARMv8-A)
- **RAM**: 4 GB LPDDR4
- **UART**: PL011 (UART0) at 0xFE201000, accessible via GPIO header pins 14/15
- **Interrupt Controller**: GIC-400 at 0xFF840000
- **Boot**: Pi firmware loads kernel at 0x80000, enters at EL2

## Prerequisites

- Raspberry Pi 400 (or Pi 4 Model B)
- microSD card (any size, FAT32 formatted)
- USB-to-serial adapter (3.3V TTL, e.g. CP2102 or FTDI)
- Serial terminal program (`cu`, `screen`, or `minicom`)

### Serial wiring

Connect the USB-to-serial adapter to the Pi 400's GPIO header:

| Adapter pin | Pi 400 GPIO pin | Function |
|-------------|-----------------|----------|
| GND         | Pin 6 (GND)     | Ground   |
| RXD         | Pin 8 (GPIO 14) | UART TX  |
| TXD         | Pin 10 (GPIO 15)| UART RX  |

**Do NOT connect the adapter's VCC/3.3V pin** -- the Pi is powered via USB-C.

### Serial terminal

Connect at **9600 baud**, 8N1:

```bash
# Using cu (recommended on macOS)
cu -l /dev/tty.usbserial-* -s 9600

# Using screen
screen /dev/tty.usbserial-* 9600

# Using minicom
minicom -D /dev/tty.usbserial-* -b 9600
```

To exit `cu`: type `~.`
To exit `screen`: type `Ctrl-A \`

## Building

```bash
# Build the kernel for Pi 400
make raspi400-build
```

This produces `target/aarch64-raspi400/release/zcore.bin`.

## SD card setup

### Option 1: Automated

```bash
# Mount the SD card, then:
make raspi400-sd SD=/Volumes/BOOT
```

### Option 2: Manual

1. Format a microSD card as FAT32 (label: `BOOT`).

2. Download the Raspberry Pi firmware files from
   [raspberrypi/firmware](https://github.com/raspberrypi/firmware/tree/master/boot):
   - `start4.elf` -- GPU bootloader
   - `fixup4.dat` -- memory configuration for start4.elf
   - `bcm2711-rpi-4-b.dtb` -- device tree blob (Pi 4B)
   - `bcm2711-rpi-400.dtb` -- device tree blob (Pi 400)
   - `overlays/` directory (at minimum, `overlays/disable-bt.dtbo`)

3. Create `config.txt`:

```
arm_64bit=1
enable_uart=1
dtoverlay=disable-bt
disable_splash=1
core_freq=500
core_freq_min=500
kernel=kernel8.img
init_uart_baud=9600
enable_gic=1
```

4. Create `cmdline.txt`:

```
LOG=info ROOTPROC=/bin/sh
```

5. Copy the kernel:

```bash
cp target/aarch64-raspi400/release/zcore.bin /Volumes/BOOT/kernel8.img
```

### SD card files summary

| File | Source | Purpose |
|------|--------|---------|
| `start4.elf` | Pi firmware | GPU bootloader -- initializes hardware, loads DTB and kernel |
| `fixup4.dat` | Pi firmware | Memory mapping configuration for `start4.elf` |
| `bcm2711-rpi-4-b.dtb` | Pi firmware | Device tree blob for Pi 4B/Pi 400 hardware description |
| `bcm2711-rpi-400.dtb` | Pi firmware | Device tree blob for Pi 400 (firmware selects automatically) |
| `config.txt` | zCore | Firmware configuration: 64-bit mode, UART, GIC, baud rate |
| `cmdline.txt` | zCore | Kernel command line: log level, root process |
| `kernel8.img` | zCore build | The zCore kernel binary (loaded to 0x80000) |
| `overlays/` | Pi firmware | DTB overlays (e.g. `disable-bt.dtbo` to free PL011 UART) |

## Booting

1. Insert the SD card into the Pi 400.
2. Connect the USB-to-serial adapter.
3. Open the serial terminal (9600 baud).
4. Power on the Pi 400 (USB-C).
5. You should see:

```
zC
zCore on Raspberry Pi 400!
[  1.350099 INFO  ...] Boot options: BootOptions { ... }
[  1.864646 INFO  ...] Free physical memory: 0x488000..0x6400000 (95 MiB)
...
[  2.650082 INFO  ...] executor run!
```

## Boot sequence

1. **Pi firmware** (`start4.elf`) initializes GPU, DRAM, peripherals, UART at 9600 baud.
2. **Firmware** loads `kernel8.img` to physical address 0x80000, passes DTB pointer in x0.
3. **Boot assembly** (`boot_raspi400.s`):
   - Prints `zC` to UART (confirms CPU is alive)
   - Drops from EL2 to EL1 (configures HCR_EL2, CNTHCTL_EL2)
   - Sets up 2-level page tables (4x 1GB blocks: 3 normal + 1 device)
   - Enables MMU (without caches first, to avoid firmware's dirty D-cache)
   - Jumps to virtual address space, enables caches
   - Zeroes BSS, sets up stack, enters `rust_main`
4. **Rust entry** (`entry.rs`):
   - Initializes logging, memory allocator
   - Parses DTB for UART/GIC addresses, memory regions, bootargs
   - Initializes PL011 UART driver at 9600 baud (48MHz clock)
   - Initializes GIC-400 interrupt controller
   - Loads petal shell ZBI and starts userboot

## Known issues

- **Timer interrupts not working**: The GIC-400 timer PPI (IRQ 30) is configured
  as Group 0 (secure) by the Pi firmware. Non-secure EL1 cannot receive Group 0
  interrupts. This requires either a custom EL3 armstub to set GICD_IGROUPR, or
  switching to an alternative timer mechanism. The kernel boots and initializes
  correctly but the executor loop blocks at `wait_for_interrupt()` because no
  timer tick is delivered. See GitHub issue for tracking.

## config.txt reference

| Setting | Value | Purpose |
|---------|-------|---------|
| `arm_64bit=1` | Required | Boot in AArch64 mode |
| `enable_uart=1` | Required | Enable UART output |
| `dtoverlay=disable-bt` | Required | Route PL011 to GPIO pins (instead of Bluetooth) |
| `disable_splash=1` | Optional | Skip GPU rainbow splash screen |
| `core_freq=500` | Recommended | Fix core clock for stable UART baud |
| `core_freq_min=500` | Recommended | Prevent clock scaling |
| `kernel=kernel8.img` | Default | Kernel filename (default for AArch64) |
| `init_uart_baud=9600` | Required | Set UART baud rate to 9600 |
| `enable_gic=1` | Required | Enable GIC interrupt controller |
