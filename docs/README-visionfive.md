# Running zCore on VisionFive

## I. VisionFive Board Setup

> **NOTE:** Host machine operations described here
> were tested on Ubuntu 20.04.

### 1. Deploy Required Services on the Host Machine

```bash
# Deploy DHCP server
# 1. Install dhcp3-server
sudo apt install isc-dhcp-server

# 2. Set DHCP interface to your physical NIC
vim /etc/default/isc-dhcp-server
# ----------------------------------------
# INTERFACESv4="enp2s0"
# INTERFACESv6=""
# ----------------------------------------

# 3. Configure static IP on the interface
vim /etc/netplan/01-network-manager-all.yaml
# -------------------------------------------
# network:
#   version: 2
#   renderer: NetworkManager
#   ethernets:
#     enp2s0:
#       addresses: [192.168.10.10/24]
#       gateway4: 192.168.10.1
#       nameservers:
#         addresses: [114.114.114.114,8.8.8.8]
#       dhcp4: no
# -------------------------------------------
sudo netplan apply

# 4. Configure DHCP settings
vim /etc/dhcp/dhcpd.conf
# -------------------------------------------
# subnet 192.168.10.0 netmask 255.255.255.0 {
#   range 192.168.10.120 192.168.10.128;
#   option routers 192.168.10.10;
#   option subnet-mask 255.255.255.0;
#   option broadcast-address 192.168.10.255;
#   option domain-name-servers 192.168.10.10;
# }
# -------------------------------------------

# 5. Start DHCP service
systemctl restart isc-dhcp-server

# 6. Check DHCP service status
systemctl status isc-dhcp-server

# Deploy TFTP server
# 1. Install tftpd-hpa
sudo apt install tftpd-hpa

# 2. Configure TFTP
# -------------------------------------------
# TFTP_USERNAME="tftp"
# TFTP_DIRECTORY="/root/work/tftpboot"
# TFTP_ADDRESS=":69"
# TFTP_OPTIONS="--secure"
# -------------------------------------------

# 3. Start TFTP service
systemctl restart tftpd-hpa

# 4. Check TFTP service status
systemctl status tftpd-hpa
```

### 2. Connect Host and VisionFive Board

![avatar](visionfive.jpg)

```bash
# Round black cable: host power
# Yellow ethernet: board DHCP network
# Blue ethernet: host internet (optional)
# White USB: board power supply (5V/3A)
# Black USB: serial console
```

Open a serial terminal (minicom) on the host. After
powering on the board, it enters U-Boot command mode
after a few seconds:

```bash
Welcome to minicom 2.7.1
Port /dev/ttyUSB0

VisionFive #

# Use `help` to see available commands
# Use `printenv` to view environment variables

# 1. Get IP address via DHCP
# (may auto-download from server; Ctrl+C to stop)
dhcp

# 2. Set TFTP client address (this board)
setenv ipaddr 192.168.10.121

# 3. Set TFTP server address (host machine)
setenv serverip 192.168.10.10

# 4. Test network connectivity (optional)
ping 192.168.10.10

# 5. Save configuration to flash
saveenv
```

### 3. Create ITB Format Image

```bash
# 1. Download starfive_fb.h,
#    starfive_vic7100_clk.dtsi,
#    starfive_vic7100_evb.dts
#    from the VisionFive official repository

# 2. Compile DTB
cpp -nostdinc -I include -undef \
    -x assembler-with-cpp \
    starfive_vic7100_evb.dts starfive.dts.0
dtc -o starfive.dtb starfive.dts.0
# Precompiled starfive.dtb is in prebuilt/

# 3. Compress kernel image
gzip -9 -cvf z.bin > z.bin.gz

# 4. Create ITB file
mkimage -f zcore-starfive.its z.itb
# zcore-starfive.its is the same as
# firmware/riscv/starfive_fdt.its

# 5. Copy to TFTP server directory
cp z.itb /path/to/tftpboot
```

The `zcore-starfive.its` content:

```dts
/dts-v1/;
/ {
    description = "U-Boot uImage source file "
                  "for zCore-visionfive";
    #address-cells = <1>;
    images {
        kernel {
            description = "Linux kernel";
            data = /incbin/("./z.bin.gz");
            type = "kernel";
            arch = "riscv";
            os = "linux";
            compression = "gzip";
            load = <0x80200000>;
            entry = <0x80200000>;
        };
        fdt {
            description = "FDT blob";
            data = /incbin/("./starfive.dtb");
            type = "flat_dt";
            arch = "riscv";
            compression = "none";
        };
    };
    configurations {
        default = "conf";
        conf {
            description = "Boot with FDT";
            kernel = "kernel";
            fdt = "fdt";
        };
    };
};
```

### 4. Upload and Boot

```bash
VisionFive # tftpboot 0xc0000000 z.itb
TFTP from server 192.168.10.10;
    our IP address is 192.168.10.121
Filename 'z.itb'.
Load address: 0xc0000000
Loading: ########## 1.5 MiB
done
VisionFive # bootm 0xc0000000
## Loading kernel from FIT Image at c0000000
   Using 'conf' configuration
   Uncompressing Kernel Image
   Loading Device Tree to 0xff6e1000

Starting kernel ...

boot page table launched,
    sstatus = 0x8000000200046000
kernel (physical):
    0000000080200000..0000000085ce70b8
kernel (remapped):
    ffffffc080200000..ffffffc085ce70b8
device tree:
    00000000ff6e1000..00000000ff6e6000

hart0 is booting...
hart1 is the primary hart.
/ #
```

## II. VisionFive Porting Issues

### 1. Flash Address

Use `tftpboot 0xc0000000 z.itb` -- the `0xc0000000`
address must leave enough space for the decompressed
kernel.

### 2. Memory Mapping

The VisionFive U-Boot sets kernel and device tree load
locations, passing the device tree address via the
`a1` register. The device tree is placed beyond 1 GiB,
but zCore originally only supported 1 GiB, causing
illegal memory errors. The memory mapping layout was
modified. See commits:

- [commit1](https://github.com/rcore-os/zCore/commit/227956d5df401c8f8f2fa746f8aa911d3530637f)
- [commit2](https://github.com/rcore-os/zCore/commit/16772b9363d02945863008c6a4639ad1cb37eed4)

### 3. Interrupt Handling Bug

Interrupts must be handled by the corresponding core.
Fix:
[commit](https://github.com/rcore-os/zCore/commit/55b3145442f0f70c01527c20e87988d26c01a39b)

### 4. Serial Driver

The VisionFive board supports the `16550` driver.
Only code adaptation was needed. See:
[commit](https://github.com/rcore-os/zCore/commit/55b3145442f0f70c01527c20e87988d26c01a39b)

### 5. Unregistered Interrupt Handling

The board reports interrupt `131` from core 1, which
is not in the device tree and thus not registered in
the kernel. The interrupt priority was lowered to mask
it. See:
[commit](https://github.com/rcore-os/zCore/commit/55b3145442f0f70c01527c20e87988d26c01a39b)
