# Makefile for top level of zCore

ARCH ?= aarch64
XTASK ?= 1

STRIP := $(ARCH)-linux-musl-strip
export PATH=$(shell printenv PATH):$(CURDIR)/.build-cache/target/$(ARCH)/$(ARCH)-linux-musl-cross/bin/

.PHONY: help build linux-run zircon-run test boot-test busybox-test config config-macos update rootfs libc-test other-test image clippy-all check doc clean \
	libos-build-linux libos-build-zircon libos-run-linux libos-run-zircon \
	petal-shell raspi400-build raspi400-run raspi400-sd \
	x86-linux-build x86-linux-run x86-zircon-build x86-zircon-run x86-uefi-image x86-uefi-usb-linux x86-uefi-usb-zircon \
	debug-qemu debug-gdb \
	pre-push

# Build the rootfs image and kernel for the target architecture.
# cargo image: builds rootfs dir (busybox + musl libc) -> packs into SFS image
# cargo bin:   compiles the kernel ELF (for riscv64, also objcopy to .bin)
build:
	cargo image --arch $(ARCH)
	ZCORE_CMDLINE="LOG=$(LOG) ROOTPROC=/bin/busybox?sh" cargo bin -m qemu-$(ARCH)

# Build (if needed) and run zCore in Linux mode interactively in QEMU.
# cargo qemu does: build rootfs image, build kernel, launch QEMU.
linux-run:
	cargo qemu -m qemu-$(ARCH)

# --- Remote Debugging via QEMU ---
# Step 1: Start QEMU paused, waiting for debugger on localhost:1234.
# Step 2: In another terminal, connect with: make debug-lldb
#         Or from RustRover: Run > Attach to Process > Remote GDB, host=localhost port=1234
debug-qemu: build
	@echo "==> Starting QEMU in debug mode (paused, waiting for debugger on :1234)..."
	@echo "    Connect with: make debug-lldb"
	@echo "    Or RustRover: Run > Attach to Process > Remote GDB Server, localhost:1234"
	@echo "    Kernel symbols: target/qemu-$(ARCH)/release/kernel"
	@qemu-system-aarch64 -m 2G -display none -no-reboot -nographic \
		-machine virt -cpu cortex-a72 \
		-kernel target/qemu-aarch64/release/kernel.bin \
		-serial mon:stdio \
		-initrd target/qemu-aarch64/release/aarch64-linux.img \
		-S -s

# Connect lldb to a running QEMU debug session.
# Uses the system lldb (or set LLDB to RustRover's bundled version).
LLDB ?= lldb
debug-lldb:
	@echo "==> Connecting lldb to QEMU on :1234..."
	@$(LLDB) \
		-o "file target/qemu-aarch64/release/kernel" \
		-o "gdb-remote 1234"

# Build and run zCore in Zircon mode (userstart hello program).
# Use LOG=info (or debug/trace/warn/error) to control log verbosity.
LOG ?= info
zircon-run:
	cargo qemu -m qemu-$(ARCH) --personality zircon --log $(LOG)

# Run the petal shell interactively in QEMU.
# Boots zCore in Zircon mode with the shell as the init program.
# Type 'help', 'echo hello', 'version', 'exit'. Ctrl-A X to kill QEMU.
petal-shell:
	ZCORE_CMDLINE="LOG=$(LOG)" cargo qemu -m qemu-$(ARCH) --personality zircon --log $(LOG)

# Build the kernel for Raspberry Pi 400 in Zircon mode.
# Userstart, petal ZBI, features, and target spec all come from
# targets/raspi400.toml via xtask.
raspi400-build:
	@echo "==> Building zCore kernel for Raspberry Pi 400..."
	ZCORE_CMDLINE="LOG=$(LOG)" cargo bin -m raspi400

# Build and run zCore on QEMU raspi400 interactively with petal shell.
# Ctrl-A X to exit QEMU.
raspi400-run: raspi400-build
	@echo "==> Starting zCore on QEMU raspi4b (Ctrl-A X to exit)..."
	@qemu-system-aarch64 -machine raspi4b -m 2G \
		-display none -no-reboot -nographic \
		-serial mon:stdio \
		-kernel target/raspi400/release/kernel.bin

# Prepare an SD card for Raspberry Pi 4 / Pi 400.
# Usage: make raspi400-sd SD=/Volumes/boot
#   SD= is the mount point of the SD card's FAT32 partition.
raspi400-sd: raspi400-build
	@tools/raspi/prepare-sd.sh $(SD)
ifeq ($(shell uname),Darwin)
	@echo "==> Ejecting SD card..."
	@disk=$$(diskutil info "$(SD)" 2>/dev/null | grep "Part of Whole" | awk '{print $$NF}'); \
	 if [ -n "$$disk" ]; then diskutil eject "/dev/$$disk"; \
	 else echo "Warning: could not determine disk for $(SD)"; fi
endif

# Build x86_64 kernel in Linux mode.
x86-linux-build:
	@echo "==> Building zCore kernel (Linux, x86_64)..."
	ZCORE_CMDLINE="LOG=$(LOG) ROOTPROC=/bin/busybox?sh" cargo zcore-build -m qemu-x86_64

# Build x86_64 kernel in Zircon mode with petal shell.
x86-zircon-build:
	@echo "==> Building zCore kernel (Zircon, x86_64)..."
	ZCORE_CMDLINE="LOG=$(LOG) ROOTPROC=/bin/shell" cargo zcore-build -m qemu-x86_64 --personality zircon

# Build and run x86_64 Linux in QEMU.
# Ctrl-A X to exit QEMU.
x86-linux-run: x86-linux-build
	cargo qemu -m qemu-x86_64 --log $(LOG)

# Build and run x86_64 Zircon with petal shell in QEMU.
# Ctrl-A X to exit QEMU.
x86-zircon-run: x86-zircon-build
	@tools/x86-bootimage/target/release/x86-bootimage \
		target/qemu-x86_64/release/kernel \
		target/qemu-x86_64/release/kernel-zircon.img
	@echo "==> Starting x86_64 Zircon (Ctrl-A X to exit QEMU)..."
	@. tools/scripts/find-ovmf.sh && OVMF=$$(find_ovmf) && \
	qemu-system-x86_64 -m 2G -display none -no-reboot -nographic \
		-machine q35 -cpu qemu64,+fsgsbase,+rdrand \
		-serial mon:stdio \
		-drive if=pflash,format=raw,readonly=on,file="$$OVMF" \
		-drive format=raw,file=target/qemu-x86_64/release/kernel-zircon.img

# Create a UEFI-bootable disk image for x86_64 real hardware.
# The image can be written to a USB drive with dd.
# Usage: make x86-uefi-image OUTPUT=/tmp/zcore-uefi.img
#        make x86-uefi-image OUTPUT=/tmp/zcore-uefi.img MODE=zircon
MODE ?= linux
OUTPUT ?= target/qemu-x86_64/release/kernel-uefi.img
x86-uefi-image:
ifeq ($(MODE),zircon)
	$(MAKE) x86-zircon-build
	@tools/scripts/x86-uefi-image.sh $(OUTPUT) none
else
	$(MAKE) build ARCH=x86_64
	@tools/scripts/x86-uefi-image.sh $(OUTPUT)
endif

# Write a UEFI boot image to a USB drive.
# WARNING: This erases all data on the USB drive!
USB ?= /dev/disk5
define write-usb
	@echo "==> Writing UEFI image to $(USB)..."
	diskutil unmountDisk $(USB)
	sudo dd if=$(OUTPUT) of=$$(echo $(USB) | sed 's|/dev/disk|/dev/rdisk|') bs=1m
	sync
	diskutil eject $(USB)
	@echo "==> Done. Insert USB into target machine and boot from UEFI."
endef

# Usage: make x86-uefi-usb-linux USB=/dev/disk5 [LOG=info]
x86-uefi-usb-linux:
	$(MAKE) x86-uefi-image MODE=linux
	$(write-usb)

# Usage: make x86-uefi-usb-zircon USB=/dev/disk5 [LOG=debug]
x86-uefi-usb-zircon:
	$(MAKE) x86-uefi-image MODE=zircon
	$(write-usb)

# Zircon boot smoke test: build in Zircon mode, start QEMU, wait for
# userstart hello message, verify clean shutdown.
zircon-boot-test:
	@echo "==> Zircon boot smoke test ($(ARCH))..."
	@tools/scripts/zircon-boot-test.sh $(ARCH)

zircon-rootfs-test:
	@echo "==> Zircon rootfs boot test ($(ARCH))..."
	@tools/scripts/zircon-rootfs-test.sh $(ARCH)

# Run all tests: boot smoke test (must pass) then libc conformance (reporting only).
test: boot-test libc-test

# Boot smoke test: start QEMU, wait for the "/ # " shell prompt, exit.
# Proves: boot assembly, MMU, HAL, VirtIO, filesystem, ELF loader, and
# busybox shell all work end-to-end. Timeout is 60 seconds.
boot-test: build
	@echo "==> Boot smoke test ($(ARCH))..."
	@tools/scripts/boot-test.sh $(ARCH)

# SMP smoke test: boot with -smp 4, verify all cores initialize.
# Only for aarch64 (other arches don't have SMP yet).
smp-test: build
	@echo "==> SMP smoke test ($(ARCH))..."
	@tools/scripts/smp-test.sh $(ARCH)

# Run busybox applet tests: echo, ls, cat, pipes, etc.
# Verifies that common busybox commands work end-to-end in QEMU.
# Depends on boot-test to ensure serialization under parallel make.
busybox-test: boot-test
	@echo "==> Busybox applet test ($(ARCH))..."
	@tools/scripts/busybox-test.sh $(ARCH)

# Run musl libc-test functional tests. Reports pass/fail counts but does
# not fail the build — the pass rate is expected to improve over time as
# more syscalls are implemented (see issue #16).
# Depends on boot-test to ensure serialization under parallel make.
libc-test: boot-test
	@tools/scripts/libc-test.sh $(ARCH)

# LibOS mode: runs zCore as a host process (no QEMU needed).
# Requires x86_64 host (Linux or macOS) or aarch64 Linux.
# Two personalities: Linux (busybox shell) and Zircon (petal programs).

# Build libos in Linux mode (default personality)
libos-build-linux:
	ZCORE_CMDLINE="LOG=$(LOG)" cargo zcore-build -m libos

# Build libos in Zircon mode (builds userstart + petal automatically)
libos-build-zircon:
	ZCORE_CMDLINE="LOG=$(LOG)" cargo zcore-build -m libos --personality zircon

# Run libos in Linux mode with busybox shell
libos-run-linux:
	cargo linux-libos --args "/bin/busybox?sh"

# Run libos in Zircon mode (known broken -- see #281)
libos-run-zircon:
	ZCORE_CMDLINE="LOG=$(LOG)" cargo zcore-build -m libos --personality zircon
	./target/release/kernel

# configure build environment (platform toolchain)
config:
ifeq ($(shell uname -s),Darwin)
	$(MAKE) config-macos
endif

config-linux:
	sudo apt update
	sudo apt install qemu-system qemu-kvm libvirt-daemon-system libvirt-clients bridge-utils virt-manager
	sudo systemctl enable --now libvirtd

# install cross-compilation toolchain on macOS via Homebrew
config-macos:
	@echo "==> Installing musl cross-compiler toolchains (macOS)..."
	@brew tap FiloSottile/musl-cross 2>/dev/null || true
	@# If musl-cross is already installed but missing x86_64 (e.g. from a
	@# previous install with --without-x86_64), reinstall to pick it up.
	@# 'brew install' on an already-installed formula does not apply new options.
	@if brew list musl-cross >/dev/null 2>&1 && \
	    ! "$$(brew --prefix musl-cross)/libexec/bin/x86_64-linux-musl-gcc" --version >/dev/null 2>&1; then \
		echo "==> Reinstalling musl-cross to add x86_64 support..."; \
		brew reinstall FiloSottile/musl-cross/musl-cross \
			--with-aarch64 --with-riscv64 --without-arm-hf; \
	else \
		brew install FiloSottile/musl-cross/musl-cross \
			--with-riscv64 --without-arm-hf; \
	fi
	@echo "==> Verifying cross-compilers..."
	aarch64-linux-musl-gcc --version
	riscv64-linux-musl-gcc --version
	x86_64-linux-musl-gcc --version
	@echo "==> Installing Linux kernel headers into musl-cross sysroots..."
	@MUSL_PREFIX=$$(brew --prefix musl-cross)/libexec; \
	KERNEL_SHA256=c1923b6bd166e6dd07be860c15f59e8273aaa8692bc2a1fce1d31b826b9b3fbe; \
	for arch_pair in "aarch64:arm64" "riscv64:riscv" "x86_64:x86"; do \
		MUSL_ARCH=$${arch_pair%%:*}; \
		KERN_ARCH=$${arch_pair##*:}; \
		SYSROOT="$$MUSL_PREFIX/$$MUSL_ARCH-linux-musl"; \
		if [ ! -d "$$SYSROOT/include/linux" ]; then \
			echo "  Installing kernel headers for $$MUSL_ARCH..."; \
			cd /tmp && \
			if [ ! -d linux-4.19.88 ]; then \
				curl -sL -o linux-4.19.88.tar.xz \
					https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.19.88.tar.xz; \
				echo "$$KERNEL_SHA256  linux-4.19.88.tar.xz" | shasum -a 256 -c - || \
					{ echo "ERROR: kernel tarball checksum mismatch"; rm -f linux-4.19.88.tar.xz; exit 1; }; \
				tar xJf linux-4.19.88.tar.xz linux-4.19.88/include linux-4.19.88/arch \
					linux-4.19.88/scripts linux-4.19.88/Makefile 2>/dev/null; \
				rm -f linux-4.19.88.tar.xz; \
			fi; \
			cd linux-4.19.88 && \
			PATH="/opt/homebrew/opt/gnu-sed/libexec/gnubin:$$PATH" \
				make ARCH=$$KERN_ARCH INSTALL_HDR_PATH="$$SYSROOT" headers_install; \
		else \
			echo "  $$MUSL_ARCH kernel headers already installed, skipping."; \
		fi; \
	done; \
	rm -rf /tmp/linux-4.19.88
	@echo "==> Installing stub headers for musl-cross sysroots..."
	@MUSL_PREFIX=$$(brew --prefix musl-cross)/libexec; \
	for MUSL_ARCH in aarch64 riscv64; do \
		SYSROOT="$$MUSL_PREFIX/$$MUSL_ARCH-linux-musl"; \
		if [ ! -f "$$SYSROOT/include/linux/compiler.h" ]; then \
			printf '%s\n' \
				'#ifndef _LINUX_COMPILER_H' \
				'#define _LINUX_COMPILER_H' \
				'#define __user' \
				'#define __force' \
				'#define __iomem' \
				'#endif' \
				> "$$SYSROOT/include/linux/compiler.h"; \
			echo "  Created $$MUSL_ARCH linux/compiler.h stub"; \
		fi; \
		if [ ! -f "$$SYSROOT/include/scsi/sg.h" ]; then \
			printf '%s\n' \
				'#ifndef _SCSI_SG_H' \
				'#define _SCSI_SG_H' \
				'#include <stdint.h>' \
				'#define SG_DXFER_NONE (-1)' \
				'#define SG_DXFER_TO_DEV (-2)' \
				'#define SG_DXFER_FROM_DEV (-3)' \
				'#define SG_IO 0x2285' \
				'#define SG_GET_VERSION_NUM 0x2282' \
				'typedef struct sg_io_hdr {' \
				'    int interface_id;' \
				'    int dxfer_direction;' \
				'    unsigned char cmd_len;' \
				'    unsigned char mx_sb_len;' \
				'    unsigned short iovec_count;' \
				'    unsigned int dxfer_len;' \
				'    void *dxferp;' \
				'    unsigned char *cmdp;' \
				'    unsigned char *sbp;' \
				'    unsigned int timeout;' \
				'    unsigned int flags;' \
				'    int pack_id;' \
				'    void *usr_ptr;' \
				'    unsigned char status;' \
				'    unsigned char masked_status;' \
				'    unsigned char msg_status;' \
				'    unsigned char sb_len_wr;' \
				'    unsigned short host_status;' \
				'    unsigned short driver_status;' \
				'    int resid;' \
				'    unsigned int duration;' \
				'    unsigned int info;' \
				'} sg_io_hdr_t;' \
				'#endif' \
				> "$$SYSROOT/include/scsi/sg.h"; \
			echo "  Created $$MUSL_ARCH scsi/sg.h stub"; \
		fi; \
		if [ ! -f "$$SYSROOT/include/scsi/scsi.h" ]; then \
			printf '%s\n' \
				'#ifndef _SCSI_SCSI_H' \
				'#define _SCSI_SCSI_H' \
				'#define SCSI_IOCTL_SEND_COMMAND 1' \
				'#define SCSI_IOCTL_DOORLOCK 0x5380' \
				'#define SCSI_IOCTL_DOORUNLOCK 0x5381' \
				'#define ALLOW_MEDIUM_REMOVAL 0x1e' \
				'#define START_STOP 0x1b' \
				'#endif' \
				> "$$SYSROOT/include/scsi/scsi.h"; \
			echo "  Created $$MUSL_ARCH scsi/scsi.h stub"; \
		fi; \
		if [ ! -f "$$SYSROOT/include/scsi/scsi_ioctl.h" ]; then \
			printf '%s\n' \
				'#ifndef _SCSI_SCSI_IOCTL_H' \
				'#define _SCSI_SCSI_IOCTL_H' \
				'#define SCSI_IOCTL_GET_IDLUN 0x5382' \
				'#define SCSI_IOCTL_GET_BUS_NUMBER 0x5386' \
				'#endif' \
				> "$$SYSROOT/include/scsi/scsi_ioctl.h"; \
			echo "  Created $$MUSL_ARCH scsi/scsi_ioctl.h stub"; \
		fi; \
	done

# print top level help
help:
	cargo xtask

# update toolchain and dependencies
update:
	cargo update-all

# put rootfs for linux mode
rootfs:
ifeq ($(XTASK), 1)
	cargo rootfs --arch $(ARCH)
else ifeq ($(ARCH), riscv64)
	@rm -rf target/rootfs/riscv && mkdir -p target/rootfs/riscv/bin
	@wget https://github.com/rcore-os/busybox-prebuilts/raw/master/busybox-1.30.1-riscv64/busybox -O target/rootfs/riscv/bin/busybox
	@ln -s busybox target/rootfs/riscv/bin/ls
endif

# put other tests into rootfs
other-test:
	cargo other-test --arch $(ARCH)

# build image from rootfs
image:
ifeq ($(XTASK), 1)
	cargo image --arch $(ARCH)
else ifeq ($(ARCH), riscv64)
	@echo building riscv.img
	@mkdir -p target/qemu-riscv64/release
	@rcore-fs-fuse target/qemu-riscv64/release/riscv64-linux.img target/rootfs/riscv zip
	@qemu-img resize -f raw target/qemu-riscv64/release/riscv64-linux.img +5M
endif

# Pre-push check: mirrors all CI jobs locally.
# Run this before every push to avoid CI failures.
# Requires: qemu-system-aarch64, qemu-system-x86_64.
pre-push:
	@echo "==> [1/11] Format check..."
	cargo fmt --all -- --check
	@echo "==> [2/11] Workspace build..."
	cargo build
	@echo "==> [3/11] Unit tests..."
	cargo test --no-fail-fast
	@echo "==> [4/11] Bare-metal aarch64 (build + rootfs)..."
	$(MAKE) build ARCH=aarch64
	@echo "==> [5/11] Boot smoke test (aarch64)..."
	$(MAKE) boot-test ARCH=aarch64
	@echo "==> [6/11] Bare-metal riscv64..."
	cargo zcore-build -m qemu-riscv64
	@echo "==> [7/11] Bare-metal x86_64 (build + rootfs)..."
	$(MAKE) build ARCH=x86_64
	@echo "==> [8/11] Boot smoke test (x86_64)..."
	$(MAKE) boot-test ARCH=x86_64
	@echo "==> [9/11] LibOS (Linux + Zircon)..."
	ZCORE_CMDLINE="LOG=info" cargo zcore-build -m libos
	ZCORE_CMDLINE="LOG=info" cargo zcore-build -m libos --personality zircon
	@echo "==> [10/11] Zircon boot test (aarch64)..."
	$(MAKE) zircon-boot-test ARCH=aarch64
	@tools/scripts/zircon-rootfs-test.sh aarch64
	@echo "==> [11/11] Libc tests (aarch64 + x86_64)..."
	$(MAKE) libc-test ARCH=aarch64
	$(MAKE) libc-test ARCH=x86_64
	@echo "==> All pre-push checks passed."

# Run clippy for all architectures (catches cross-platform issues).
# Features come from targets/qemu-<arch>.toml via xtask.
clippy-all:
	cargo check-style

# check code style
check:
	cargo check-style

# build and open project document
doc:
	cargo doc --open

# clean targets
# `cargo clean` removes target/ which includes rootfs, images, and kernel binaries.
clean:
	cargo clean
	rm -f  *.asm
	# Legacy locations (can be removed once all local checkouts are migrated)
	rm -rf rootfs
	rm -rf zCore/disk
	find zCore -maxdepth 1 -name "*.img" -delete
	find zCore -maxdepth 1 -name "*.bin" -delete

# delete targets, including those that are large and compile slowly
cleanup: clean
	rm -rf .build-cache/target
	rm -rf ignored/target

# delete everything, including origin files that are downloaded directly
clean-everything: clean
	rm -rf .build-cache
	rm -rf ignored

# rt-test:
# 	cd rootfs/x86_64 && git clone https://kernel.googlesource.com/pub/scm/linux/kernel/git/clrkwllms/rt-tests --depth 1
# 	cd rootfs/x86_64/rt-tests && make
# 	echo x86 gcc build rt-test,now need manual modificy.
