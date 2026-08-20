# zCore on RISC-V 64 for SiFive FU740

### Building the zCore FU740 System Image

First, build the riscv64 filesystem in the source
root directory. Then enter the `zCore` subdirectory
to compile the kernel, which generates the system
image `zcore-fu740.itb`.

The system image includes the FU740 board's device
tree. The DTB can be obtained from the board's Linux
`/boot` directory, or from the SiFive official image:
https://github.com/sifive/freedom-u-sdk/releases/download/2022.04.00/demo-coreip-cli-unmatched-2022.04.00.rootfs.wic.xz

```sh
make riscv-image
cd zCore
make build MODE=release LINUX=1 \
    ARCH=riscv64 PLATFORM=fu740
```

### Booting via U-Boot

First, set up a TFTP server. For example, on a Linux
server install `tftpd-hpa` (the TFTP directory is
typically `/srv/tftp/`).

Copy the compiled `zcore-fu740.itb` image to the
TFTP server directory.

Power on the FU740 board and enter the U-Boot command
line:

```
# Configure board IP and server IP
setenv ipaddr <IP>
setenv serverip <Server IP>

# Load the system image via TFTP
tftp 0xa0000000 zcore-fu740.itb

# Boot
bootm 0xa0000000
```

### FU740 Resources

HiFive Unmatched FU740 board documentation:
https://github.com/oscomp/DocUnmatchedU740/blob/main/unmatched.md
