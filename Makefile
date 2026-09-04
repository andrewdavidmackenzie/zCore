# Makefile for top level of zCore

ARCH ?= aarch64
XTASK ?= 1

STRIP := $(ARCH)-linux-musl-strip
export PATH=$(shell printenv PATH):$(CURDIR)/ignored/target/$(ARCH)/$(ARCH)-linux-musl-cross/bin/

.PHONY: help build run test boot-test config config-macos update rootfs libc-test other-test image clippy check doc clean

# Build the rootfs image and kernel for the target architecture.
# cargo image: builds rootfs dir (busybox + musl libc) -> packs into SFS image
# cargo bin:   compiles the kernel ELF (for riscv64, also objcopy to .bin)
build:
	cargo image --arch $(ARCH)
	cargo bin -m virt-$(ARCH)

# Build (if needed) and run zCore interactively in QEMU.
# cargo qemu does: build rootfs image, build kernel, launch QEMU.
run:
	cargo qemu --arch $(ARCH)

# Build and run zCore in Zircon mode (userstart hello program).
# The kernel constructs a test ZBI in-memory -- no external ZBI file needed.
# Use LOG=info (or debug/trace/warn/error) to control log verbosity.
LOG ?= warn
zircon-run:
	cargo qemu --arch $(ARCH) --zircon --log $(LOG)

# Zircon boot smoke test: build in Zircon mode, start QEMU, wait for
# userstart hello message, verify clean shutdown.
zircon-boot-test:
	@echo "==> Zircon boot smoke test ($(ARCH))..."
	@tools/scripts/zircon-boot-test.sh $(ARCH)

# Run all tests: boot smoke test (must pass) then libc conformance (reporting only).
test: boot-test libc-test

# Boot smoke test: start QEMU, wait for the "/ # " shell prompt, exit.
# Proves: boot assembly, MMU, HAL, VirtIO, filesystem, ELF loader, and
# busybox shell all work end-to-end. Timeout is 60 seconds.
boot-test: build
	@echo "==> Boot smoke test ($(ARCH))..."
	@tools/scripts/boot-test.sh $(ARCH)

# Run musl libc-test functional tests. Reports pass/fail counts but does
# not fail the build — the pass rate is expected to improve over time as
# more syscalls are implemented (see issue #16).
# Depends on boot-test to ensure serialization under parallel make.
libc-test: boot-test
	@tools/scripts/libc-test.sh $(ARCH)

# configure build environment (platform toolchain)
config:
ifeq ($(shell uname -s),Darwin)
	$(MAKE) config-macos
endif

# install cross-compilation toolchain on macOS via Homebrew
config-macos:
	@echo "==> Installing musl cross-compiler toolchains (macOS)..."
	@brew tap FiloSottile/musl-cross 2>/dev/null || true
	brew install FiloSottile/musl-cross/musl-cross \
		--with-riscv64 --without-arm-hf
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
	@rm -rf rootfs/riscv && mkdir -p rootfs/riscv/bin
	@wget https://github.com/rcore-os/busybox-prebuilts/raw/master/busybox-1.30.1-riscv64/busybox -O rootfs/riscv/bin/busybox
	@ln -s busybox rootfs/riscv/bin/ls
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
	@rcore-fs-fuse zCore/riscv64.img rootfs/riscv zip
	@qemu-img resize -f raw zCore/riscv64.img +5M
endif

# Run clippy on all workspace crates.
# Step 1: kernel + OS crates via the custom bare-metal target.
#         Each package is listed explicitly so --no-deps can skip
#         shared crates (executor, region-alloc).
# Step 2: host-side tools (xtask, region-alloc) via native target.
clippy:
	@echo "==> Clippy: kernel crates ($(ARCH))..."
	cargo clippy \
		-p zcore -p kernel-hal -p linux-object -p linux-syscall \
		-p linux-loader -p zircon-object -p zircon-syscall -p zcore-drivers \
		--no-default-features --features linux \
		--target zCore/$(ARCH).json \
		-Z json-target-spec \
		-Z build-std=core,alloc \
		-Z build-std-features=compiler-builtins-mem \
		--no-deps -- --deny warnings
	@echo "==> Clippy: zircon-loader (requires USERSTART_ELF)..."
	@if [ -n "$(USERSTART_ELF)" ]; then \
		cargo clippy -p zircon-loader \
			--target zCore/$(ARCH).json \
			-Z json-target-spec \
			-Z build-std=core,alloc \
			-Z build-std-features=compiler-builtins-mem \
			--no-deps -- --deny warnings; \
	else \
		echo "  (skipped -- set USERSTART_ELF to enable)"; \
	fi
	@echo "==> Clippy: host tools..."
	cargo clippy -p xtask -p region-alloc -p zircon-abi \
		--no-deps -- --deny warnings
	@echo "==> Clippy: userspace programs ($(ARCH))..."
ifeq ($(ARCH), aarch64)
	cargo clippy -p petal -p userstart \
		--target aarch64-unknown-none-softfloat \
		--no-deps -- --deny warnings
else ifeq ($(ARCH), riscv64)
	cargo clippy -p petal -p userstart \
		--target riscv64gc-unknown-none-elf \
		--no-deps -- --deny warnings
else ifeq ($(ARCH), x86_64)
	cargo clippy -p petal -p userstart \
		--target x86_64-unknown-none \
		--no-deps -- --deny warnings
endif

# check code style
check:
	cargo check-style

# build and open project document
doc:
	cargo doc --open

# clean targets
clean:
	cargo clean
	rm -f  *.asm
	rm -rf rootfs
	rm -rf zCore/disk
	find zCore -maxdepth 1 -name "*.img" -delete
	find zCore -maxdepth 1 -name "*.bin" -delete

# delete targets, including those that are large and compile slowly
cleanup: clean
	rm -rf ignored/target

# delete everything, including origin files that are downloaded directly
clean-everything: clean
	rm -rf ignored

# rt-test:
# 	cd rootfs/x86_64 && git clone https://kernel.googlesource.com/pub/scm/linux/kernel/git/clrkwllms/rt-tests --depth 1
# 	cd rootfs/x86_64/rt-tests && make
# 	echo x86 gcc build rt-test,now need manual modificy.
