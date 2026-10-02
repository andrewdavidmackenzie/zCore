# Jolla C2 / Reeder S19 Max Pro S — Boot & Custom Kernel Guide

## Hardware

- **SoC**: Unisoc UMS9230 (Tiger T606 / T7200) — ARM Cortex-A75 x2 + Cortex-A55 x6
- **GPU**: Mali-G57 MP1
- **RAM**: 6 GB LPDDR4X
- **Storage**: 128 GB eMMC
- **Display**: 6.58" 1080x2408 (IPS LCD)
- **USB**: USB-C, USB 2.0 (DWC3 controller, peripheral mode confirmed working; host mode partial)
- **Original device**: Reeder S19 Max Pro S (Turkish OEM), rebranded by Jolla as the Jolla C2
- **Modem**: Unisoc integrated LTE modem (separate ARM core running proprietary firmware)

## Boot Chain

```
Boot ROM → FDL1 → FDL2/SPL → U-Boot (lk.bin) → Kernel
```

The Unisoc UMS9230 SoC boots through a multi-stage chain:

1. **Boot ROM** (mask ROM in SoC) — has a USB download protocol (entered via **Vol Up + Power**)
2. **FDL1** (First Download Loader) — loaded into SRAM at `0x65000800`, sets up DRAM
3. **FDL2/SPL** (Second Download Loader) — loaded into DRAM at `0x9efffe00`, provides partition access
4. **U-Boot / lk.bin** — the Android bootloader (or our custom U-Boot fork as secondary)
5. **Linux Kernel**

The Boot ROM USB download protocol lets you flash any partition, including the bootloader,
using the `spd_dump` tool with a CVE-2022-38694 exploit.

## USB Modes and Detection

### Normal boot (Sailfish OS running) — CONFIRMED
The phone appears as a USB composite device using Google's Android USB VID:
- **Vendor ID**: `0x18D1` (Google — standard Android USB gadget)
- **Product ID**: `0x0A02`
- **Manufacturer string**: `Reeder`
- **Product string**: `Jolla C2`
- **Serial**: `21210112221773`
- **USB Speed**: USB 2.0 High Speed (480 Mbps)
- **bcdDevice**: `0x0504` (firmware version 5.04)
- **bDeviceClass**: 0 (composite — interfaces define function)
- **Location**: via USB 2.0 Hub chain
- ADB does not detect it by default (USB developer mode may need to be enabled on the phone)
- No USB serial `/dev/tty*` device is created in normal boot mode on macOS
- Check with: `system_profiler SPUSBDataType` or `ioreg -p IOUSB -l | grep -A 40 "Jolla C2"`

### Unisoc Download Mode (Vol Up + Power)
- **Vendor ID**: `0x1782` (Spreadtrum)
- **Product ID**: `0x4d00` (download mode)
- The phone displays a console message about recovery mode
- On macOS, requires no special driver; on Linux appears as `/dev/ttyUSB0` or similar
- This is the mode used by `spd_dump` for low-level flashing

### fastboot mode
- If U-Boot is installed and configured, it can expose a fastboot interface
- Standard Android fastboot protocol

### Recovery mode
- Vol Up + Power shows console message allowing recovery boot
- Cancel to boot normally

## Flashing via USB with spd_dump

The `spd_dump` tool communicates with the Unisoc Boot ROM download protocol.
It requires FDL1 and FDL2 firmware blobs to bootstrap the flash process.

### Backup the boot partition (do this first!)
```bash
./spd_dump exec_addr 0x65015f08 \
  fdl fdl1-dl.bin 0x65000800 \
  fdl fdl2-dl.bin 0x9efffe00 \
  exec r boot_a reset
```

### Flash a new boot image
```bash
./spd_dump exec_addr 0x65015f08 \
  fdl fdl1-dl.bin 0x65000800 \
  fdl fdl2-dl.bin 0x9efffe00 \
  exec w boot_a .../u-boot/boot.img reset
```

Key addresses:
- `0x65015f08` — exec address (Boot ROM exploit entry)
- `0x65000800` — FDL1 load address (SRAM)
- `0x9efffe00` — FDL2 load address (DRAM)

## Building Your Own Kernel

### Prerequisites
- Cross-compiler: `aarch64-linux-gnu-` toolchain
- The kernel build repo: https://git.abscue.de/linux-mainlining/sailfish-os/jolla-c2-kernel-build

### Clone and configure
```bash
git clone --recursive https://git.abscue.de/linux-mainlining/sailfish-os/jolla-c2-kernel-build.git
cd jolla-c2-kernel-build
git submodule update --init

mkdir linux/_out
cp linux.config linux/_out/.config

mkdir u-boot/_out
cp u-boot.config u-boot/_out/.config
```

### Build Linux kernel
```bash
cd linux
make O=_out ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- -j8
cd ..
```

### Build U-Boot (secondary bootloader)
```bash
cd u-boot
make O=_out CROSS_COMPILE=aarch64-linux-gnu- -j8
../make-bootimage.sh    # produces boot.img
cd ..
```

The U-Boot defconfig for this board is `reeder_s19mps_defconfig`.

### Install Linux to SD card
For Sailfish OS (SD card at /dev/mmcblk0):
```bash
cd linux
# First install without modules (touchscreen won't work yet):
../linux-install.sh --no-modules /dev/mmcblk0p1 _out
# Boot the device, then re-insert SD card and install with modules:
../linux-install.sh /dev/mmcblk0p1 _out
```

### Manual installation (non-Sailfish)
Create ext4 boot + system partitions on SD card, then:
```bash
# Install kernel and DTBs to boot partition
cd linux
sudo make O=_out ARCH=arm64 zinstall dtbs_install INSTALL_PATH=/path/to/mounted/boot

# Create extlinux config
mkdir -p /path/to/mounted/boot/extlinux
# (see linux-install.sh for extlinux.conf template)

# Install kernel modules to system partition
sudo make O=_out ARCH=arm64 modules_install INSTALL_MOD_PATH=/path/to/mounted/system

# Optionally install Debian
sudo debootstrap --arch=arm64 trixie /path/to/mounted/system
```

## U-Boot Boot Configuration

U-Boot reads `extlinux.conf` for boot configuration. Two partition layouts supported:

1. **Single partition** (e.g. SD card): one ext4 partition, kernel at `/boot/extlinux/extlinux.conf`
2. **Separate /boot** (e.g. Sailfish LVM): boot partition root has `/extlinux/extlinux.conf`

## Framebuffer Console Output

The display controller is a Unisoc DPU (Display Processing Unit). For kernel console output:

- The stock Android bootloader initializes the display hardware before chainloading U-Boot
- The framebuffer is reserved in memory at `0x9e000000` (4500 KiB region: `0x9e000000..0x9e464fff`)
- There is a ramoops region at `0xfff80000..0xfffbffff` (256 KiB) for crash logs
- The mainline kernel has working display support via the DRM/KMS subsystem
- For early console: use `console=tty0` in kernel command line to get fbcon output
- The DPU driver in the mainline fork handles the 1080x2408 panel
- U-Boot also has display support and shows its own console output

### Enabling fbcon in kernel config
Ensure these are enabled in your kernel config:
```
CONFIG_FRAMEBUFFER_CONSOLE=y
CONFIG_VT=y
CONFIG_VT_CONSOLE=y
CONFIG_FB=y
CONFIG_DRM_FBDEV_EMULATION=y
```

Add to kernel command line: `console=tty0` (or `console=tty0 console=ttyS0,115200` for dual output)

## Current Mainline Kernel Status (as of Dec 2025 / kernel ~7.1)

### Working
- Display (DRM/KMS), GPU (Mali-G57 via Panfrost)
- Touchscreen
- USB peripheral mode, USB Power Delivery
- Battery charging and fuel gauge
- WiFi (client mode, custom driver)
- Bluetooth (A2DP works)
- Mobile data (first SIM)
- Voice calls (VoLTE supported)
- Audio (speakers, microphone, earpiece, headphones)
- GPS
- Sensors
- Camera (main + wide-angle, via libcamera with software ISP; autofocus working)
- Suspend/resume
- Display blanking
- Reboot
- OTA updates (via OBS: https://build.sailfishos.org/project/show/home:affe_null:c2-mainlining)

### Not yet working / known issues
- Dual SIM (only first SIM slot)
- Jack detection (headphones require manual pactl switch)
- USB host mode (works with keyboard/mouse, broken with storage)
- WiFi hotspot / AP mode
- Bluetooth handsfree calling
- Hardware video acceleration
- Camera app integration (no native camera app yet, `qcam` works)
- WiFi occasionally fails on boot (related to GNSS processor boot order)

## Key Source Repositories

| Resource                                                             | URL                                                                                                                                                                                                                      |
|----------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Mainline kernel fork** (UMS9230, ~7.1)                             | [codeberg.org/ums9230-mainline/linux](https://codeberg.org/ums9230-mainline/linux)                                                                                                                                       |
| **U-Boot fork** (secondary bootloader)                               | [codeberg.org/ums9230-mainline/u-boot](https://codeberg.org/ums9230-mainline/u-boot)                                                                                                                                     |
| **Kernel build repo** (configs + scripts)                            | [git.abscue.de/linux-mainlining/sailfish-os/jolla-c2-kernel-build](https://git.abscue.de/linux-mainlining/sailfish-os/jolla-c2-kernel-build)                                                                             |
| **Sailfish OS local build scripts**                                  | [git.abscue.de/linux-mainlining/sailfish-os/localbuild](https://git.abscue.de/linux-mainlining/sailfish-os/localbuild)                                                                                                   |
| **Jolla official kernel** (downstream, 5.10)                         | [github.com/mer-hybris/s19mps-kernel](https://github.com/mer-hybris/s19mps-kernel)                                                                                                                                       |
| **Jolla bootloader source**                                          | [github.com/mer-hybris/s19mps-droid-hal-img-bootloader-c2](https://github.com/mer-hybris/s19mps-droid-hal-img-bootloader-c2)                                                                                             |
| **Flashing guide** (Unisoc flash mode + spd_dump)                    | [sailfishos.wiki — Flashing the Jolla C2](https://sailfishos.wiki/books/jolla-c2/page/flashing-the-jolla-c2-using-the-unisoc-flash-mode)                                                                                 |
| **Mainline kernel forum thread** (progress, issues, SD-card boot)    | [forum.sailfishos.org](https://forum.sailfishos.org/t/mainline-linux-kernel-for-the-jolla-c2/21382)                                                                                                                      |
| **postmarketOS port**                                                | [wiki.postmarketos.org/wiki/Jolla_C2_(jolla-c2)](https://wiki.postmarketos.org/wiki/Jolla_C2_(jolla-c2))                                                                                                                 |
| **FOSDEM 2026 talk + slides** (boot chain, U-Boot chainloading, DTB) | [fosdem.org — Running mainline Linux on the Jolla C2](https://fosdem.org/2026/schedule/event/9KYVGM-jolla-c2/) · [slides (PDF)](https://fosdem.org/2026/events/attachments/9KYVGM-jolla-c2/slides/267073/c2_egt5x46.pdf) |
| **FOSDEM 2026 video recording**                                      | [AV1/WebM](https://video.fosdem.org/2026/ub4132/9KYVGM-jolla-c2.av1.webm) · [MP4](https://video.fosdem.org/2026/ub4132/9KYVGM-jolla-c2.mp4)                                                                            |
| **Prebuilt Sailfish OS image** (mainline kernel, SD-card boot)       | [storage.abscue.de — latest image](https://storage.abscue.de/private/zImage/Sailfish_OS-s19mps-latest.zip)                                                                                                               |
| **OTA update packages** (OBS)                                        | [build.sailfishos.org](https://build.sailfishos.org/project/show/home:affe_null:c2-mainlining)                                                                                                                           |

## Quick-Start: Boot Custom Kernel from SD Card (safest path)

1. Download the prebuilt image or build your own kernel (see above)
2. Insert SD card into the Jolla C2
3. If using the prebuilt image, unzip and run `./install-on-device.sh` from the phone's shell
4. The device boots from SD card automatically if U-Boot is flashed to `boot_a`
5. To return to stock Sailfish OS: remove the SD card and reboot

## Architecture Notes

### The U-Boot Chainloading Trick
The stock Android bootloader (SPL/lk) does essential hardware initialization:
- Configures DRAM timings
- Initializes the display panel
- Sets up eMMC/SD storage
- Loads and verifies the next stage

Instead of replacing this entirely (risky, and the init code is proprietary),
the mainline U-Boot is flashed as the *boot image* that the stock bootloader chainloads.
U-Boot then provides its own device tree (for the Reeder S19 Max Pro S / Jolla C2),
which avoids the fragmented Android dtbo overlay system.

### Device Tree
The device tree for the hardware is at:
`arch/arm64/boot/dts/sprd/ums9230-reeder-s19mps.dts` in the mainline kernel fork.

### Firmware Blobs
WiFi, Bluetooth, and GNSS firmware are proprietary Unisoc blobs:
- WiFi: `wcnmodem.bin` (loaded via remoteproc)
- GNSS: loaded via remoteproc (boot order matters — if GNSS boots before WiFi, connectivity crashes)
- Packaged in the `firmware-jolla-c2` postmarketOS package
- Sourced from the `device-bsp-s19mps` repo

### Memory Map (from dmesg)
```
0x9e000000..0x9e464fff  — framebuffer region (4500 KiB, nomap)
0xfff80000..0xfffbffff  — ramoops (256 KiB, pstore crash logs)
0x94100000..0x96100000  — audio firmware region (32 MiB, nomap)
0xb0000000..0xb6100000  — reserved region (99 MiB, nomap)
0x87240000              — SIPC shared memory (wcn)
0x88000000              — SIPC shared memory (remoteproc0)
```

## Booting a Custom (non-Linux) Kernel (e.g. zCore)

The boot chain for running your own kernel on the Jolla C2 is:

```
Stock SPL (in flash) → U-Boot 2nd (in boot_a) → extlinux.conf → YOUR KERNEL (on SD card)
```

### What U-Boot expects

The prebuilt `u-boot-2nd.img` is an **Android boot image** (`ANDROID!` magic header) containing
U-Boot as the "kernel" payload. It gets flashed to `boot_a` and `boot_b` partitions. The stock
SPL/lk bootloader chainloads it.

Once U-Boot runs, it reads `/extlinux/extlinux.conf` (or `/boot/extlinux/extlinux.conf`) from
the first partition of the SD card. This is a standard syslinux/extlinux boot config that points
to a kernel binary and optional DTB/initrd.

### extlinux.conf format

```
DEFAULT ice-fishing
TIMEOUT 30

LABEL ice-fishing
    KERNEL /vmlinuz
    FDT /dtbs/sprd/ums9230-reeder-s19mps.dtb
    APPEND console=tty0 root=/dev/mmcblk1p2 rw
```

For a bare-metal kernel (no Linux, no initrd), you'd use:
```
DEFAULT zcore
TIMEOUT 30

LABEL zcore
    KERNEL /zcore.bin
    FDT /dtbs/sprd/ums9230-reeder-s19mps.dtb
```

### Kernel binary format

U-Boot on aarch64 expects one of:
1. **Linux ARM64 Image** — flat binary with the standard ARM64 Linux header at offset 0
   (magic `ARM\x64` at offset 0x38). U-Boot detects this and boots it with `booti`.
2. **FIT image** — Flattened Image Tree wrapping kernel + DTB + optional initrd.
3. **Raw binary** — if loaded at the correct address, U-Boot can `go` to it.

The simplest approach for zCore: produce a flat aarch64 binary with the ARM64 Image header.
The header is 64 bytes:
```
offset 0x00: code0          — branch to kernel entry (or NOP)
offset 0x04: code1          — reserved
offset 0x08: text_offset    — image load offset from start of RAM (usually 0x0)
offset 0x10: image_size     — effective image size (can be 0)
offset 0x18: flags          — kernel flags
offset 0x20: res2           — reserved
offset 0x28: res3           — reserved
offset 0x30: res4           — reserved  
offset 0x38: magic          — 0x644d5241 ("ARM\x64")
offset 0x3c: res5           — reserved (PE header offset for EFI, or 0)
```

### Prebuilt artifacts available (in `prebuilt/`)

| File | Size | Purpose |
|------|------|---------|
| `u-boot-2nd.img` | 464 KB | **U-Boot as Android boot image** — flash to `boot_a` and `boot_b` |
| `u-boot-spl-16k-ufs-sign.bin` | 62 KB | SPL for UFS — flash to UFS LUN partitions |
| `fdl1-sign.bin` | 59 KB | FDL1 blob for `spd_dump` (loads into SRAM) |
| `lk.bin` | 1.0 MB | Stock Android bootloader (reference only) |
| `flash-sdcard.sh` | Script | Partitions SD card: 600MB boot (ext4) + rest for rootfs |
| `install-on-device.sh` | Script | Flashes boot partitions from the phone itself |

### SD Card Layout

The flash-sdcard.sh creates a GPT table with:
- **Partition 1** (`sdcard-boot`): 600 MB, ext4, type `0FC63DAF-8483-4772-8E79-3D69D8477DE4` (Linux filesystem)
  - Contains: kernel binary, DTB files, extlinux/extlinux.conf
  - For zCore: just your kernel binary + DTB + extlinux.conf
- **Partition 2** (`sdcard-sailfish`): remaining space, for rootfs (not needed for bare-metal zCore)

For a minimal zCore boot, you only need partition 1.

### Flashing procedure (from the phone itself)

The install-on-device.sh script shows that flashing is done **from the phone**:
```bash
# These are the partitions that get flashed:
dd if=u-boot-2nd.img of=/dev/disk/by-partlabel/boot_a bs=4096
dd if=u-boot-2nd.img of=/dev/disk/by-partlabel/boot_b bs=4096
dd if=u-boot-spl-16k-ufs-sign.bin of=/dev/disk/by-path/platform-20200000.ufs-scsi-0:0:0:1 bs=4096
dd if=u-boot-spl-16k-ufs-sign.bin of=/dev/disk/by-path/platform-20200000.ufs-scsi-0:0:0:2 bs=4096
```

Alternative: use `spd_dump` from the host in Unisoc download mode (Vol Up + Power).

### Display / Framebuffer for zCore

The stock bootloader initializes the display before chainloading U-Boot. The framebuffer is at
physical address `0x9e000000` (4500 KiB, 1080x2408 pixels). Your kernel can write directly to
this address to display output without needing a display driver — the panel is already initialized
and scanning from this buffer.

Framebuffer format (from DTB `chosen/framebuffer` node):
- **Format**: `a8r8g8b8` (ARGB8888, 4 bytes per pixel)
- **Width**: 720 (0x2d0)
- **Height**: 1600 (0x640)
- **Stride**: 2880 bytes/row (0xb40 = 720 * 4)
- **Physical address**: `0x9e000000`
- **Size**: 0x465000 (4,542,464 bytes = ~4.3 MiB) = 720 * 1600 * 4 = 4,608,000 — fits
- **Note**: status is "disabled" in DTB but the bootloader has already initialized the panel

### Hardware Watchdog

- **Address**: `0x644e0000` (size 0x1000)
- **Compatible**: `sprd,ums9230-wdt`
- **Timeout**: 12 seconds — **must disable or pet within 12s or hardware resets**
- This is the cause of the reboot loop seen with mainline Sailfish OS

### UART (serial console)

- **UART0**: `0x200a0000` (serial0)
- **UART1**: `0x200b0000` (serial1) — **stdout-path, 115200n8**
- **UART2**: `0x200c0000` (serial2)
- Compatible: `sprd,ums9230-uart`

### DPU (Display Processing Unit)

- **Address**: `0x31000000` (size 0x800)
- Compatible: `sprd,ums9230-dpu`

### Memory

- **RAM base**: `0x80000000`

### Boot chain CONFIRMED WORKING (2026-10-02)

Successfully tested the full chain:
```
Stock SPL → U-Boot (flashed to boot_a/boot_b) → extlinux.conf (SD card) → mainline kernel → Sailfish OS
```
- U-Boot console output visible on display
- Kernel boots from SD card
- Flashing was done from the phone itself using `dd` (no spd_dump needed):
  ```bash
  # From Sailfish OS terminal with devel-su:
  mount -t ext4 /dev/mmcblk1p1 /mnt
  dd if=/dev/disk/by-partlabel/boot_a of=/mnt/boot_a_backup.bin bs=4096   # BACKUP FIRST
  dd if=/mnt/u-boot-2nd.img of=/dev/disk/by-partlabel/boot_a bs=4096
  dd if=/mnt/u-boot-2nd.img of=/dev/disk/by-partlabel/boot_b bs=4096
  ```
- SD card appears as `/dev/mmcblk1` on the phone (p1=boot, p2=sailfish)
- Known issue: phone reboots after Sailfish OS starts (known mainline kernel bug)

### Minimal zCore boot checklist

1. [ ] Build zCore as a flat aarch64 binary with ARM64 Image header
2. [x] Get the device tree blob (DTB) for ums9230-reeder-s19mps — already on SD card boot partition
3. [x] Format SD card with boot partition — done, working
4. [ ] Replace /Image on SD card with zCore binary, update extlinux.conf
5. [x] Flash `u-boot-2nd.img` to `boot_a` and `boot_b` — done
6. [ ] Insert SD card, power on — U-Boot reads extlinux.conf, loads and jumps to zCore
7. [ ] zCore writes to framebuffer at `0x9e000000` for console output

## Developer: Affe Null (Otto Pfluger)
- Mastodon: https://mt.abscue.de/@affe_null
- GitLab: https://git.abscue.de/linux-mainlining
- The mainline port is being daily-driven as of late 2025
