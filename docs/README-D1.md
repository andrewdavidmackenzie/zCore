# zCore on RISC-V 64

## Allwinner D1 (C906) Development Board

### Building the zCore System Image

First, build the riscv64 filesystem in the source
root directory. Then enter the `zCore` subdirectory
to compile the kernel, which generates the system
image `zcore.bin`:

```sh
make riscv-image
cd zCore
make build LINUX=1 ARCH=riscv64 \
    PLATFORM=d1 MODE=release
```

### Flashing to the Development Board

Using the Allwinner D1 C906 board as an example.

Download and compile the `xfel` flashing tool:

```sh
git clone https://github.com/xboot/xfel.git
cd xfel
make
```

### Automatic Flash and Run

With `xfel` installed and the board in FEL mode (enter
FEL mode by running `reboot efex` in the board's Linux
system), run:

```sh
make run LINUX=1 ARCH=riscv64 \
    PLATFORM=d1 MODE=release
```

### (Optional) Manual Flash and Run

1. Download the D1 board's
   [OpenSBI](https://github.com/elliott10/opensbi)
   source and compile the image
   `build/platform/thead/c910/firmware/fw_payload.elf`:

    ```sh
    git clone \
        https://github.com/elliott10/opensbi \
        -b thead
    cd opensbi
    make PLATFORM=thead/c910 \
        CROSS_COMPILE=/path/to/toolchain/bin/\
    riscv64-unknown-linux-gnu- \
        SUNXI_CHIP=sun20iw1p1 \
        PLATFORM_RISCV_ISA=rv64gcxthead
    ```

    Or use the precompiled image:
    [prebuilt/firmware/riscv/d1_fw_payload.elf](../prebuilt/firmware/riscv/d1_fw_payload.elf)

2. Generate the combined firmware containing OpenSBI,
   DTB, and zCore:

    ```sh
    rust-objcopy \
        --binary-architecture=riscv64 \
        ../prebuilt/firmware/d1/fw_payload.elf \
        --strip-all -O binary ./zcore_d1.bin
    dd if=../target/riscv64/release/zcore.bin \
        of=zcore_d1.bin bs=512 seek=2048
    ```

3. Power on the D1 C906 board and enter FEL mode.
   Then use `xfel` to load the zCore image into DDR:

    ```
    sudo xfel ddr d1
    sudo xfel write 0x40000000 zcore_d1.bin
    sudo xfel exec 0x40000000
    ```

### Boot Output

After zCore boots successfully, OpenSBI loads the DTB
to high address `0x5ff00000`. Output looks like:

```
OpenSBI smartx-d1-tina-v1.0.1-release
   ____                    _____ ____ _____
  / __ \                  / ____|  _ \_   _|
 | |  | |_ __   ___ _ __ | (___ | |_) || |
 | |  | | '_ \ / _ \ '_ \ \___ \|  _ < | |
 | |__| | |_) |  __/ | | |____) | |_) || |_
  \____/| .__/ \___|_| |_|_____/|____/_____|
        | |
        |_|

Platform Name          : T-HEAD Xuantie Platform
Platform HART Features : RV64ACDFIMSUVX
Platform Max HARTs     : 1
Current Hart           : 0
Firmware Base          : 0x40000400
Firmware Size          : 75 KB
Runtime SBI Version    : 0.2

      ____
 ____/ ___|___  _ __ ___
|_  / |   / _ \| '__/ _ \
 / /| |__| (_) | | |  __/
/___|\____\___/|_|  \___|

Welcome to zCore rust_main(
    hartid: 0x0, device_tree_paddr: 0x44ddc)
Uart output testing
+++ Setting up UART interrupts +++
+++ Setting up PLIC +++
+++ setup interrupt +++
Exception::Breakpoint: A breakpoint set @...
/ #
/ # ls
bin  dev  tmp
/ # hello
Hello world from user mode program!
/ #
```
