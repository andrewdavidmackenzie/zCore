# Raspberry Pi 400 (BCM2711)

zCore runs on the Raspberry Pi 400 (and Pi 4B) in Zircon flavour mode.

## Current status

**Boot to executor**: Working. The kernel boots, initializes all hardware
(UART, GIC-400, timer), loads the userstart ELF, and enters the executor loop.

**Executor progress**: Not working on real hardware. The executor context
switch completes and timer interrupts fire inside the executor, but the
Zircon userstart process does not produce output. The same binary works
correctly on QEMU's `raspi4b` machine emulation, reaching the petal shell.
See [issue #259](https://github.com/andrewdavidmackenzie/zCore/issues/259)
for details and investigation notes.

## Hardware

- **SoC**: Broadcom BCM2711, quad-core Cortex-A72 (ARMv8-A)
- **RAM**: 4 GB LPDDR4 (kernel uses ~95 MiB starting at 0x488000)
- **UART**: PL011 (UART0) at 0xFE201000, accessible via GPIO header pins 14/15
- **Interrupt controller**: GIC-400 at 0xFF840000 (GICD at +0x1000, GICC at +0x2000)
- **Timer**: ARM Generic Timer, virtual timer (CNTV, PPI 11 = GIC IRQ 27)
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

Connect at **115200 baud**, 8N1:

```bash
# Using cu (recommended on macOS)
cu -l /dev/tty.usbserial-* -s 115200

# Using screen
screen /dev/tty.usbserial-* 115200

# Using minicom
minicom -D /dev/tty.usbserial-* -b 115200
```

To exit `cu`: type `~.`
To exit `screen`: type `Ctrl-A \`

## Building

```bash
# Build the kernel for Pi 400
make raspi400-build
```

This produces `target/aarch64-raspi400/release/zcore.bin`.

## Testing on QEMU

The Pi 400 kernel can also be tested on QEMU's `raspi4b` machine:

```bash
qemu-system-aarch64 \
  -machine raspi4b -m 2G -display none \
  -serial mon:stdio \
  -kernel target/aarch64-raspi400/release/zcore.bin
```

This reaches the petal shell and runs correctly.

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

3. Copy `tools/armstub/armstub8-gic.bin` from this repo (or build it with
   `aarch64-none-elf-gcc` from `tools/armstub/armstub8-gic.S`).

4. Create `config.txt`:

```
arm_64bit=1
enable_uart=1
dtoverlay=disable-bt
disable_splash=1
core_freq=500
core_freq_min=500
kernel=kernel8.img
init_uart_baud=115200
enable_gic=1
armstub=armstub8-gic.bin
```

5. Create `cmdline.txt`:

```
LOG=info ROOTPROC=/bin/sh
```

6. Copy the kernel:

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
| `armstub8-gic.bin` | This repo | EL3 stub that configures GIC-400 groups before kernel entry |
| `config.txt` | zCore | Firmware configuration: 64-bit mode, UART, GIC, baud rate |
| `cmdline.txt` | zCore | Kernel command line: log level, root process |
| `kernel8.img` | zCore build | The zCore kernel binary (loaded to 0x80000) |
| `overlays/` | Pi firmware | DTB overlays (e.g. `disable-bt.dtbo` to free PL011 UART) |

## Booting

1. Insert the SD card into the Pi 400.
2. Connect the USB-to-serial adapter.
3. Open the serial terminal (115200 baud).
4. Power on the Pi 400 (USB-C).
5. You should see:

```
zC
zCore on Raspberry Pi 400!
[  0.978 INFO  ...] Boot options: BootOptions { ... }
[  1.557 INFO  ...] Free physical memory: 0x488000..0x6400000 (95 MiB)
...
[  2.656 INFO  ...] timer: virtual timer enabled, CNTFRQ=53974656
[  2.795 INFO  ...] executor run!
```

## Boot sequence

1. **Pi firmware** (`start4.elf`) initializes GPU, DRAM, peripherals, UART at 115200 baud.
2. **Firmware** loads `armstub8-gic.bin` to physical address 0x0, writes kernel entry
   address to offset 0xFC and DTB pointer to offset 0xF8, then releases ARM cores.
3. **Armstub** (EL3): configures GIC-400 (all interrupts to Group 1 / Non-Secure),
   sets SCR_EL3 (NS=1), drops to Non-Secure EL2.
4. **Armstub** (EL2): reads entry/DTB from mailbox, branches to kernel at 0x80000.
6. **Boot assembly** (`boot_raspi400.s`):
   - Prints `zC` to UART (confirms CPU is alive)
   - Drops from EL2 to EL1 (configures HCR_EL2, CNTHCTL_EL2)
   - Sets up 2-level page tables (4x 1GB blocks: 3 normal + 1 device)
   - Enables MMU (without caches first, to avoid firmware's dirty D-cache)
   - Jumps to virtual address space, enables caches
   - Zeroes BSS, sets up stack, enters `rust_main`
7. **Rust entry** (`entry.rs`):
   - Initializes logging, memory allocator
   - Parses DTB for UART/GIC addresses, memory regions, bootargs
    - Initializes PL011 UART driver (keeps firmware baud rate, enables RX interrupts)
   - Initializes GIC-400 (Non-Secure Group 1 mode)
   - Enables virtual timer (CNTV, IRQ 27)
   - Loads petal shell ZBI and starts userboot

## Custom armstub

The Pi firmware's default armstub does not configure the GIC-400 interrupt
groups. Without this configuration, all interrupts remain in Group 0 (Secure)
and cannot be received by the Non-Secure EL1 kernel.

The `tools/armstub/armstub8-gic.S` in this repo is based on the upstream
[Raspberry Pi armstub](https://github.com/raspberrypi/tools/blob/master/armstubs/armstub8.S)
with one critical fix: `setup_gic` is called **before** `SCR_EL3.NS` is set
to 1, so the `GICD_IGROUPR` writes happen while the CPU is still in Secure
state. The upstream order (SCR first, then GIC) causes the writes to be
silently ignored on real GIC-400 hardware.

To rebuild:

```bash
aarch64-none-elf-gcc -DGIC=1 -DBCM2711=1 -nostdlib -nostartfiles \
  -Wl,--section-start=.text=0x0 \
  -o tools/armstub/armstub8-gic.elf tools/armstub/armstub8-gic.S
aarch64-none-elf-objcopy -O binary \
  tools/armstub/armstub8-gic.elf tools/armstub/armstub8-gic.bin
```

## Known issues

### Executor hang on real hardware (#259)

The kernel boots correctly and enters the executor loop, but the Zircon
userstart process does not make progress on real Pi 400 hardware. The same
binary works on QEMU `raspi4b`.

**What works:**
- Full boot to `executor run!` with UART output
- GIC-400 configured correctly (custom armstub sets IGROUPR to Group 1)
- Virtual timer (CNTV, IRQ 27) fires and is delivered as IRQ
- Timer IRQ handler runs inside the executor context
- Executor context switch (switch.S) completes successfully
- `timer_tick()` and `handle_timeout()` are called (matching riscv64/x86_64)

**What doesn't work:**
- The executor's task future (userstart) does not produce output
- `run_until_idle()` never returns to the main loop

**What was investigated and ruled out:**
- GIC Group 0 vs Group 1: resolved with custom armstub
- Physical timer (IRQ 30) vs virtual timer (IRQ 27): switched to virtual, no change
- FIQ vs IRQ delivery: Group 1 = IRQ, FIQ not involved
- DAIF masking: interrupts are enabled (`intr_on()` before executor)
- `handle_timeout()` deadlock: same code works on QEMU
- Multiple executor runtimes: reduced to 1, no change
- TTBR0 corruption in switch.S: skipped TTBR0=0 write, no change
- Edge vs level trigger: ICFGR shows level-triggered (correct)
- Stale executor build: clean rebuild confirmed

**Likely next steps:**
- GDB remote debugging via JTAG to inspect CPU state inside the executor
- Compare real hardware TLB/cache behavior with QEMU model
- Check if user-mode `eret` behaves differently on real Cortex-A72

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
| `init_uart_baud=115200` | Required | Set UART baud rate to 115200 |
| `enable_gic=1` | Required | Enable GIC interrupt controller |
| `armstub=armstub8-gic.bin` | Required | Load custom armstub with GIC Group 1 config |
