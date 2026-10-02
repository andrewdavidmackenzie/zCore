# Jolla C2 / UMS9230 Hardware Reference for zCore

All addresses extracted from the live device tree on the phone.

## CPUs

| Core | MPIDR | Type | Notes |
|------|-------|------|-------|
| cpu@0 | 0x000 | Cortex-A55 | LITTLE cluster |
| cpu@1 | 0x100 | Cortex-A55 | LITTLE cluster |
| cpu@2 | 0x200 | Cortex-A55 | LITTLE cluster |
| cpu@3 | 0x300 | Cortex-A55 | LITTLE cluster |
| cpu@4 | 0x400 | Cortex-A55 | LITTLE cluster |
| cpu@5 | 0x500 | Cortex-A55 | LITTLE cluster |
| cpu@6 | 0x600 | Cortex-A75 | big cluster |
| cpu@7 | 0x700 | Cortex-A75 | big cluster |

- Boot CPU: cpu@0 (Cortex-A55)
- SMP enable: PSCI (`arm,psci-0.2`, method `smc`)
- All cores support `cpu-idle-states`

## Memory

| Region | Address | Size | Notes |
|--------|---------|------|-------|
| RAM base | `0x80000000` | runtime-determined | LPDDR4X, 6 GB |
| Framebuffer | `0x9e000000` | `0x465000` (4.3 MiB) | nomap, pre-initialized |
| Ramoops | `0xfff80000` | `0x40000` (256 KiB) | pstore crash logs |
| Audio DSP | `0x94100000` | 32 MiB | nomap |
| Reserved | `0xb0000000` | 99 MiB | nomap |
| WCNSS SIPC | `0x87240000` | - | WiFi/BT shared memory |

## Framebuffer (simple-framebuffer, pre-initialized by bootloader)

```
Address:  0x9e000000
Width:    720 pixels
Height:   1600 pixels
Stride:   2880 bytes (720 * 4)
Format:   ARGB8888 (a8r8g8b8)
Size:     4,608,000 bytes (720 * 1600 * 4)
```

Pixel layout (each pixel = 4 bytes, little-endian):
```
byte 0: Blue
byte 1: Green
byte 2: Red
byte 3: Alpha (0xFF = opaque)
```

To write a white pixel at (x, y):
```
address = 0x9e000000 + y * 2880 + x * 4
write_u32(address, 0xFFFFFFFF)
```

## Watchdog (CRITICAL — must handle within 12 seconds of boot)

```
Address:    0x644e0000
Size:       0x1000
Compatible: sprd,ums9230-wdt / sprd,sp9860-wdt
Timeout:    12 seconds
IRQ:        SPI 79
```

Sprd watchdog register layout (from Linux driver `drivers/watchdog/sprd_wdt.c`):
```
0x00  SPRD_WDT_LOAD_LOW     — load value low 16 bits
0x04  SPRD_WDT_LOAD_HIGH    — load value high 16 bits
0x08  SPRD_WDT_CTRL         — control register
0x0C  SPRD_WDT_INT_CLR      — interrupt clear
0x10  SPRD_WDT_INT_RAW      — raw interrupt status
0x14  SPRD_WDT_INT_MSK      — masked interrupt status
0x18  SPRD_WDT_CNT_LOW      — counter low
0x1C  SPRD_WDT_CNT_HIGH     — counter high
0x20  SPRD_WDT_LOCK         — lock register
0x24  SPRD_WDT_IRQ_LOAD_LOW — IRQ load low
0x28  SPRD_WDT_IRQ_LOAD_HIGH— IRQ load high
```

Control bits (SPRD_WDT_CTRL @ 0x08):
```
bit 0: WDT_INT_EN    — interrupt enable
bit 1: WDT_CNT_EN   — counter enable (set = running)
bit 2: reserved
bit 3: WDT_RST_EN   — reset enable (set = reset on timeout)
```

To disable the watchdog:
```
write_u32(0x644e0020, 0x00E551)   // unlock: write magic to LOCK register
write_u32(0x644e0008, 0x00)       // clear CTRL — disable counter and reset
write_u32(0x644e0020, 0x000000)   // re-lock (optional)
```

Lock register magic: `0x00E551` ("SPRD" unlock key — verify against Linux source).

## Interrupt Controller (GICv3)

```
Distributor:    0x10000000  (size 0x20000)
Redistributor: 0x10040000  (size 0x100000)
Stride:         0x20000 per redistributor
Regions:        1
```

## Timer (ARM Generic Timer)

```
Compatible: arm,armv8-timer
IRQs (PPI):
  - Secure phys:  PPI 13, level
  - Non-secure phys: PPI 14, level  
  - Virtual:      PPI 11, level
  - Hypervisor:   PPI 10, level
```

## UART

| Instance | Address | IRQ | Notes |
|----------|---------|-----|-------|
| serial0 | `0x200a0000` | SPI 2 | |
| serial1 | `0x200b0000` | SPI 3 | **stdout, 115200n8** |
| serial2 | `0x200c0000` | SPI 4 | |

Compatible: `sprd,ums9230-uart` / `sprd,sc9836-uart`
Register size: 0x100

## Display

```
DPU:        0x31000000 (size 0x800)
Compatible: sprd,ums9230-dpu / sprd,sharkl3-dpu
IRQ:        SPI 27
```

## USB

```
DWC3:       (check DTS for exact address)
Mode:       peripheral (host mode partial)
```

## Storage

```
eMMC (mmc0): 0x201a0000
SD card:     appears as mmcblk1 in Linux
```

## Boot Entry Conditions

When U-Boot loads and jumps to the kernel:
- CPU is in EL2 or EL1 (non-secure)
- MMU is off
- D-cache may be on or off (U-Boot typically leaves it on)
- Interrupts are disabled
- x0 = DTB physical address (passed by U-Boot)
- PC = kernel entry point (start of Image)
- Framebuffer is live at 0x9e000000 — pixels written there appear immediately
- Watchdog is running — 12 second timeout
- UART1 at 0x200b0000 is initialized at 115200n8
