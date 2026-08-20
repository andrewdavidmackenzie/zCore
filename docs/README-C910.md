# zCore on RISC-V 64

## T-HEAD C910 Light Val Board

### Building the zCore Kernel Image

Compile the zCore kernel:

```
cd zCore/zCore
make build LINUX=1 MODE=release \
    ARCH=riscv64 PLATFORM=c910light
```

Create a U-Boot system image:

```
mkimage -A riscv -O linux -C none -T kernel \
    -a 0x200000 -e 0x200000 \
    -n "zCore for c910" \
    -d ../target/riscv64/release/zcore.bin \
    uImageC910
```

### Building the OpenSBI Image

```
git clone \
    https://github.com/elliott10/opensbi.git \
    -b thead_light-c910
cd opensbi
make PLATFORM=generic \
    CROSS_COMPILE=/path/to/toolchain/bin/\
riscv64-unknown-linux-gnu-
# Generates the required fw_dynamic.bin
```

Note: the original toolchain was compiled from the
official repository
https://gitee.com/thead-yocto/xuantie-yocto.git.
Other toolchains should work as replacements.

### Running via U-Boot

Place the compiled OpenSBI image `fw_dynamic.bin` and
system image `uImageC910` in the TFTP server
directory. Enter the C910 Light board's U-Boot command
line (with network configured):

```
ext4load mmc 0:2 $aon_ram_addr \
    light_aon_fpga.bin;
ext4load mmc 0:2 $dtb_addr ${fdt_file};
tftp $opensbi_addr fw_dynamic.bin;
tftp $kernel_addr uImageC910;
bootslave;
run finduuid;
run set_bootargs;
bootm $kernel_addr - $dtb_addr;
```

---

## C910 Light Val Board Porting Notes

### 1. Initial Analysis and Image Creation

![c910-light](img/c910-light.jpeg)

The C910 Light board is shown above. After obtaining
the board, review the official user manual to
understand the hardware components and cable
connections (power and serial ports).

User manual:
https://gitee.com/thead-yocto/documents/blob/master/en/user_guide/T-Head%20Yeying1520%20Yocto%20User%20Guide.pdf

With power connected and the serial port linked to
the host machine, four serial ports `/dev/ttyUSBX`
appear. The debug serial port is used by connecting
from the host and pressing any key during U-Boot
startup to enter command line mode.

From the U-Boot command line, connect via wired
network and use TFTP to load the OS image to boot.

```
# minicom -b 115200 -D /dev/ttyUSB2

U-Boot 2020.01 (Dec 14 2021)
CPU:   rv64imafdcvsu
Model: T-HEAD c910 light
DRAM:  1 GiB

C910 Light#
# setenv ipaddr <IP>
# setenv serverip <Server IP>
```

Initially, the FU740 board's zCore image format and
TFTP boot approach were tried. However, booting the
image produced an error:

```
Wrong Image Format for bootm command
ERROR: can't get kernel image!
```

Analysis showed that different U-Boot versions
recognize different image formats. The FU740 uses the
newer FIT image format (`.its`-based), while the C910
Light's U-Boot only supports old legacy images.
The fix:

```
mkimage -A riscv -O linux -C none -T kernel \
    -a 0x200000 -e 0x200000 \
    -n "zCore for c910" \
    -d ../target/riscv64/release/zcore.bin \
    uImageC910
```

### 2. Boot Initialization

From the board's existing Linux image, the load
address is `0x200000` and the entry point is
`0x200000`. The zCore linker script's `BASE_ADDRESS`
was set accordingly.

After compiling and loading via TFTP with `bootm`,
the kernel showed no output after jumping to the
entry point. Two possible causes: (1) the default
OpenSBI print call was incorrect, or (2) virtual
memory setup and jump failed.

To isolate the issue, zCore was first run with
physical memory only (modifying boot offset variables
in `zCore/src/platform/riscv/` to use `0x200000` as
the entry address). This eliminated issue (2),
leaving issue (1) -- the OpenSBI output problem.

To verify OpenSBI output, the most minimal zCore
image was tried, calling OpenSBI's
`SBI_CONSOLE_PUTCHAR` at the earliest Rust code stage.
Still no output -- a dead end.

### JTAG Debugging

The C910 Light board includes a JTAG debugging port.
Following the T-HEAD debugging documentation:
https://occ.t-head.cn/document?temp=linux&slug=t-head-debug-server-user-manual

![jtag](img/c910-jtag.jpg)

After connecting GDB to the JTAG Debug Server,
single-step CPU instruction debugging became possible.
Breakpoints sometimes failed until the zCore image
was loaded into memory.

During single-step debugging, the `ecall
SBI_CONSOLE_PUTCHAR` instruction produced no output.
This pointed to either no valid OpenSBI or a serial
driver problem.

### Fixing Serial Output

Direct serial output was implemented by referencing
U-Boot's serial code:

```rust
// T-HEAD C910 light
pub fn uart_put(c: u8) {
    let ptr = BADDR as *mut u32;
    unsafe {
        // LSR bit: THRE
        while ptr.add(5).read_volatile()
            & (1 << 5) == 0 {}
        // Transmitter empty, THR is 8-bit valid
        ptr.add(0).write_volatile(c as u32);
    }
}
pub fn uart_get() -> Option<u8> {
    let ptr = BADDR as *mut u32;
    unsafe {
        // Check LSR DR bit for data ready
        if ptr.add(5).read_volatile() & 0b1 == 0 {
            None
        } else {
            Some((ptr.add(0).read_volatile()
                & 0xff) as u8)
        }
    }
}
```

With this, zCore could produce output.

A device tree parsing failure occurred because the
DTB was loaded into memory that the zCore kernel
overlapped. Moving the DTB to a higher address offset
resolved the issue.

![c910 uart](img/c910-run-uart.png)

Later, documentation and source code from T-HEAD
support provided more information about the C910
Light board's boot chain:

```
U-Boot (runs in M-mode)
    |
    V
OpenSBI (loaded by U-Boot, jumps from M to S-mode)
    |
    V
Linux (loaded by U-Boot, runs in S-mode)
```

Therefore, loading only a zCore kernel via U-Boot
without OpenSBI means no SBI print is available --
explaining the lack of output.

The OpenSBI serial driver was missing support for the
C910 Light board's `snps,dw-apb-uart` type, falling
back to the generic `ns16550`. The fix:
https://github.com/elliott10/opensbi/commit/404951dd5b047873fa023545eafeb1fa2a9c5838

After the fix, OpenSBI output worked.

### 3. Virtual Memory Page Table Issues

Continuing execution, a new problem appeared: the
system stopped after page table switching
(`switch table`).

This was puzzling -- the kernel sections and memory
address spaces were mapped to a new page table using
the same parameters and process as other board types
(QEMU, D1, etc.). The hang point:

```
[  INFO] initialized kernel page table @ 0x5c18000
[ DEBUG] cpu 0 switch table 2ec000 -> 5c18000
```

JTAG debugging revealed that after `csrw satp`, the
program crashed. The page table itself was suspect.

Comparing with Linux's page table setup for RISC-V:

![c910 pagetable](img/c910-linux-pg.png)

Linux adds extra PTE flags for kernel memory
(`PAGE_KERNEL`): `CACHE`, `SHARE`, `BUF`. For device
memory (`PAGE_IOREMAP`): `SHARE`, `SO`.

These extended PTE attributes are documented in the
C910 chip manual, in bits `63:59`: `SO`, `CACHE`,
`BUF`, `SHARE`, `SEC`. These extensions exist in
both C910 and C906 CPUs.

![c910 pte flags](img/c910-pte-flags.png)

Adding the `CACHE`, `SHARE`, `BUF` bits to kernel
page table entries didn't help -- the problem
persisted after `switch table`.

The chip manual revealed a control bit in the
extended status register `MXSTATUS` called `MAEE`:

![c910 mxstatus](img/c910-mxstatus.png)

The `MAEE` bit controls whether extended MMU address
attributes (`SO`, `CACHE`, `BUF`, `SHARE`, `SEC`)
are enabled in page table entries.

- `MAEE = 0`: Extended attributes disabled
- `MAEE = 1`: Extended attributes enabled in PTEs

![c910 mxstatus](img/c910-maee.jpeg)

Setting `MAEE = 0` (disabling extended PTE
attributes) resolved the `switch table` hang. zCore
could continue to device tree node parsing, though
it then encountered high device address issues...

### 4. MAEE PTE Extension on CR1825 Board

During investigation of the CR1825 board's atomic
lock bug, a suggestion was that the `.bss` section
might not be properly cleared.

A similar issue had occurred when first porting zCore
to D1: spin atomic instructions deadlocked because
uninitialized atomic variables contained random
values due to an uncleared `.bss` section.

![c906 clear bss](img/c906-clear-bss.jpeg)

The `.bss` clearing was subsequently added. After
boot code refactoring, it was confirmed that `.bss`
clearing is executed.

Comparing C910 and C906 chip manuals showed no major
differences in attribute settings. After fixing the
C910 Light board's `switch table` hang, analysis of
the CR1825's OpenSBI, U-Boot, and Linux source code
revealed the same PTE extension pattern: kernel
memory configured with `CACHE`, `SHARE`, `BUF` bits,
which zCore had never set.

The hypothesis: the PTE extension bit issue was
strongly related to the CR1825's atomic lock problem,
since atomic operations require reading and writing
virtual memory values.

Testing with `MAEE = 0` on CR1825 confirmed that the
atomic lock problem was indeed caused by the T-HEAD
extended PTE attributes.

![c906 amo 1](img/c906-amo-1.jpg)
![c906 amo 2](img/c906-amo-2.jpg)

T-HEAD technical support confirmed: "The PTE
cacheable bit indicates that the page can be cached
by the C906 cache. If 0, it will not enter the cache.
The PTE bufferable bit controls whether the
bufferable signal is asserted during bus transfers."
"AMO instructions require cache support -- they
cannot be used on non-cached regions."

After resolving the CR1825 lock issue, progress was
smoother.

When `MAEE` is enabled, note that kernel image and
device address spaces require different PTE attribute
values. Per Linux source:
- Kernel memory (`PAGE_KERNEL`): `CACHE`, `SHARE`,
  `BUF`
- Device memory (`PAGE_IOREMAP`): `SHARE`, `SO`

### 5. High Device Address Issue

After building virtual memory and parsing device tree
nodes, high device addresses caused errors. For
example, the serial base address from the device tree
is `0xffe7014000` -- much larger than typical device
addresses (e.g., QEMU serial at `0x10000000`).

In physical address space these large addresses work
fine, but after adding the kernel virtual memory
offset (e.g., `0xffffffffe000200000`), the result
overflows the SV39 virtual address space.

The fix: when an address overflows, set the reserved
bits back to `1`:

```
paddr | (0x1ffffff << 39)
```

### 6. Running on C910 Light Board

zCore successfully boots into the busybox shell:

![c910 zcore](img/c910-zcore-run.png)
