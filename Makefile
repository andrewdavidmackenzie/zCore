# Makefile for top level of zCore
#
# Build configurations are `<product>.<board>` pairs, mirroring
# Fuchsia's `fx set PRODUCT.BOARD`:
#   products: configs/products/  (busybox, petal, petal-linux, core-tests, zircon-hello)
#   boards:   configs/boards/    (qemu-aarch64, qemu-x86_64, ..., libos)
#
#   make list-products / make list-boards   — discover definitions
#   cargo zcore set <product>.<board>       — persist a default selection
#
# Most targets take CONFIG=<product>.<board> (default: busybox.qemu-aarch64).
# ARCH keys arch-specific operations (rootfs images, musl toolchains).

CONFIG ?= busybox.qemu-aarch64
XTASK ?= 1

# Split the selection into product and board parts.
PRODUCT := $(word 1,$(subst ., ,$(CONFIG)))
BOARD := $(word 2,$(subst ., ,$(CONFIG)))

# Architecture for arch-keyed operations (rootfs images, musl
# toolchains). Derived from the selected board's TOML unless explicitly
# overridden on the command line, so e.g. `make boot-test
# CONFIG=busybox.qemu-x86_64` cannot accidentally test with the aarch64
# default. The boot/busybox/libc test targets reject a conflicting
# explicit ARCH.
BOARD_ARCH := $(shell awk -F'"' '/^arch/ {print $$2}' configs/boards/$(BOARD).toml 2>/dev/null)
ifeq ($(origin ARCH),undefined)
ARCH := $(if $(BOARD_ARCH),$(BOARD_ARCH),aarch64)
endif

# Selection-derived artifact paths (keyed by the full selection).
KERNEL_DIR := target/$(CONFIG)/release
KERNEL := $(KERNEL_DIR)/kernel
KERNEL_BIN := $(KERNEL_DIR)/kernel.bin
BOOT_IMG := $(KERNEL_DIR)/boot.img

# Read core count from the board config (defaults to 1).
BOARD_CORES := $(shell grep '^cores' configs/boards/$(BOARD).toml 2>/dev/null | awk '{print $$NF}' || echo 1)
QEMU_CORES := $(BOARD_CORES)
X86_CORES := $(shell grep '^cores' configs/boards/qemu-x86_64.toml 2>/dev/null | awk '{print $$NF}' || echo 1)

# How the board's image is written to real hardware (from its TOML):
# "sd" or "usb". Absent for QEMU/libos boards.
FLASH_METHOD := $(shell grep '^flash-method' configs/boards/$(BOARD).toml 2>/dev/null | awk '{print $$NF}' | tr -d '"')

STRIP := $(ARCH)-linux-musl-strip
export PATH=$(shell printenv PATH):$(CURDIR)/.build-cache/target/$(ARCH)/$(ARCH)-linux-musl-cross/bin/


.PHONY: help build run test boot-test busybox-test config config-macos setup update rootfs libc-test other-test image clippy-all check doc docs clean \
	libos-build-linux libos-build-zircon libos-run-linux libos-run-zircon \
	list-products list-boards flash petal-test petal-tests \
	debug-qemu debug-lldb debug-x86-qemu debug-x86-gdb \
	pre-push pre-push-quick core-tests core-tests-build \
	boot-logo uefi-firmware

# List available products and boards (like fx list-products / list-boards).
list-products:
	@cargo zcore list-products
list-boards:
	@cargo zcore list-boards

# Build the rootfs image and kernel for a selection.
#   make build CONFIG=petal.qemu-aarch64
# QEMU boards also build the rootfs image selected by the product's
# rootfs-contents manifest; real-hardware boards build the kernel only
# (their boot media is prepared by `make flash`/`make image-uefi`).
HAS_QEMU := $(shell grep -q '^\[qemu\]' configs/boards/$(BOARD).toml 2>/dev/null && echo yes)
build:
ifneq ($(HAS_QEMU),yes)
	cargo bin -m $(CONFIG)
else
	cargo xtask rootfs-image -m $(CONFIG)
	cargo bin -m $(CONFIG)
endif

# Build (if needed) and run a selection interactively in QEMU.
#   make run CONFIG=busybox.qemu-aarch64   — busybox shell ("/ # ")
#   make run CONFIG=petal.qemu-aarch64     — petal shell
# Ctrl-A X to exit QEMU.
run:
	cargo qemu -m $(CONFIG)

# Backwards-compatible aliases for the old per-flavour run targets.
# LOG=info (or debug/trace/warn/error) controls log verbosity.
LOG ?= info
linux-run:
	cargo qemu -m busybox.$(BOARD) --log $(LOG)
# Zircon mode: petal shell (Zircon-only, no Linux).
zircon-run:
	cargo qemu -m petal.$(BOARD) --log $(LOG)
# Petal shell with Linux available.
petal-shell:
	cargo qemu -m petal-linux.$(BOARD) --log $(LOG)

# --- Remote Debugging via QEMU ---
# See docs/debugging.md for full instructions.
#
# aarch64: make debug-qemu / make debug-lldb
# x86_64:  make debug-x86-qemu / make debug-x86-gdb
#          make debug-x86-core-tests CORE_TESTS_FILTER='SuiteName.*'
#
# Step 1: Start QEMU paused, waiting for debugger on localhost:1234.
# Step 2: In another terminal, connect with the matching debug-* target.

# --- aarch64 debugging ---
debug-qemu: build
	@echo "==> Starting aarch64 QEMU in debug mode (paused, :1234)..."
	@echo "    Connect with: make debug-lldb"
	@echo "    Kernel symbols: $(KERNEL)"
	@ROOTFS_IMG=$$(cargo xtask rootfs-image -m $(CONFIG) --print-path); \
	qemu-system-aarch64 -m 2G -display none -no-reboot -nographic \
		-machine virt -cpu cortex-a72 -smp $(QEMU_CORES) \
		-kernel $(KERNEL_BIN) \
		-serial mon:stdio \
		-initrd "$$ROOTFS_IMG" \
		-S -gdb tcp:127.0.0.1:1234

LLDB ?= lldb
debug-lldb:
	@echo "==> Connecting lldb to QEMU on :1234..."
	@$(LLDB) \
		-o "file $(KERNEL)" \
		-o "gdb-remote 1234"

# --- x86_64 debugging ---
# Start x86_64 Zircon QEMU paused with GDB server on :1234.
# Defaults to the petal product on the x86_64 QEMU board; pass an
# explicit CONFIG=<x86_64 selection> to debug another one.
DEBUG_X86_CONFIG = $(if $(filter command line,$(origin CONFIG)),$(CONFIG),petal.qemu-x86_64)
debug-x86-qemu:
	$(MAKE) build CONFIG=$(DEBUG_X86_CONFIG) ARCH=x86_64
	@echo "==> Starting x86_64 Zircon QEMU in debug mode (paused, :1234)..."
	@echo "    Connect with: make debug-x86-gdb"
	@echo "    Kernel symbols: target/$(DEBUG_X86_CONFIG)/release/kernel"
	@. tools/scripts/find-ovmf.sh && OVMF=$$(find_ovmf) && \
	ROOTFS_IMG=$$(cargo xtask rootfs-image -m $(DEBUG_X86_CONFIG) --print-path); \
	RAMDISK_ARGS=""; \
	case "$$ROOTFS_IMG" in \
		*/petal-*) ;; \
		*) RAMDISK_ARGS="--ramdisk $$ROOTFS_IMG" ;; \
	esac; \
	tools/x86-bootimage/target/release/x86-bootimage \
		target/$(DEBUG_X86_CONFIG)/release/kernel \
		target/$(DEBUG_X86_CONFIG)/release/kernel-zircon.img $$RAMDISK_ARGS; \
	qemu-system-x86_64 -m 2G -display none -no-reboot -nographic \
		-machine q35 -smp $(X86_CORES) -cpu qemu64,+rdrand \
		-serial mon:stdio \
		-drive if=pflash,format=raw,readonly=on,file="$$OVMF" \
		-drive format=raw,file=target/$(DEBUG_X86_CONFIG)/release/kernel-zircon.img \
		-S -gdb tcp:127.0.0.1:1234

# Start x86_64 core-tests QEMU paused with GDB server on :1234.
# Build first with: make core-tests-build
# Use CORE_TESTS_FILTER to run a specific test suite.
debug-x86-core-tests: core-tests-build
	@echo "==> Starting x86_64 core-tests QEMU in debug mode (paused, :1234)..."
	@echo "    Connect with: make debug-x86-gdb"
	@echo "    Kernel symbols: target/core-tests.qemu-x86_64/release/kernel"
	@. tools/scripts/find-ovmf.sh && OVMF=$$(find_ovmf) && \
	qemu-system-x86_64 -m 4G -display none -no-reboot -nographic \
		-machine q35 -smp $(X86_CORES) \
		-cpu qemu64,+rdrand,+rdtscp,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
		-serial mon:stdio \
		-drive if=pflash,format=raw,readonly=on,file="$$OVMF" \
		-drive "format=raw,file=target/core-tests.qemu-x86_64/release/boot.img" \
		-S -gdb tcp:127.0.0.1:1234

# Connect GDB to a running x86_64 QEMU debug session.
# Uses the same selection default as debug-x86-qemu.
GDB ?= gdb
debug-x86-gdb:
	@echo "==> Connecting GDB to QEMU on :1234..."
	@$(GDB) target/$(DEBUG_X86_CONFIG)/release/kernel \
		-ex "target remote :1234"

# ── Rootfs image ────────────────────────────────────────────────────────
# Build the rootfs image for the current selection. The product's
# rootfs-contents manifest decides what goes in:
#   full       — busybox, musl libc, petal programs, demo binaries
#   petal      — petal programs only
#   core-tests — petal programs + prebuilt Fuchsia test binaries
image:
	cargo xtask rootfs-image -m $(CONFIG)

# Backwards-compatible alias (the old target was arch-keyed).
rootfs: image

# ── Interactive demos ───────────────────────────────────────────────────
# Each demo is just `make run` with a product selection. The product
# decides the kernel flavour (linux feature), ROOTPROC, and rootfs.
#
# Demo: boot into busybox shell (Linux init), can run /bin/zircon-hello
demo-busybox:
	cargo qemu -m busybox.$(BOARD) --log warn

# Demo: boot into petal shell (with Linux available)
demo-petal:
	cargo qemu -m petal-linux.$(BOARD) --log warn

# Demo: boot into petal shell (Zircon only, no Linux)
# /bin/linux-hello should return an error.
demo-zircon:
	cargo qemu -m petal.$(BOARD) --log warn

# Test: boot petal shell with Linux, run /bin/zircon-hello and
# /bin/linux-hello, verify both produce expected output.
demo-petal-test:
	tools/scripts/demo-petal-test.sh petal-linux.qemu-$(ARCH) $(ARCH)

# Test: boot petal shell with Linux, run /bin/hello.wasm via wasi-runner,
# verify output.
demo-wasi-test:
	tools/scripts/demo-wasi-test.sh petal-linux.qemu-$(ARCH) $(ARCH)

# ── UEFI boot demos (aarch64 only) ─────────────────────────────────────
# Same demos but booting via UEFI firmware instead of raw -kernel.
# Requires: mtools (mformat, mcopy, mmd), edk2-aarch64-code.fd

# Demo: UEFI petal shell (with Linux)
demo-petal-uefi:
	cargo qemu -m petal-linux.qemu-aarch64-uefi --log warn

# Demo: UEFI busybox shell
demo-busybox-uefi:
	cargo qemu -m busybox.qemu-aarch64-uefi --log warn

# Test: UEFI boot with petal shell, run /bin/zircon-hello
uefi-boot-test:
	tools/scripts/uefi-boot-test.sh

# ── Documentation ──────────────────────────────────────────────────────
# Build the mdbook documentation site into target/book/.
docs:
	mdbook build

# Alias for backwards compatibility.
book: docs

# Build the kernel for Raspberry Pi 400 in Zircon mode.
# Userstart, petal ZBI, features, and target spec all come from
# ── Real hardware: generic build/image/flash ────────────────────────────
# Boards declare their `flash-method` in configs/boards/<board>.toml:
#   sd  — write boot files into a mounted SD card FAT partition (SD=)
#   usb — write a raw UEFI disk image to a USB drive (USB=)
#
# Usage:
#   make build CONFIG=petal.raspi400
#   make image-uefi CONFIG=busybox.x86-laptop OUTPUT=/tmp/zcore.img
#   make flash CONFIG=petal.raspi400 SD=/Volumes/boot
#   make flash CONFIG=busybox.x86-laptop USB=/dev/disk5
#
# The QEMU raspi4b emulation of the Pi 400 has no [qemu] section in the
# board config; run it directly:
#   qemu-system-aarch64 -machine raspi4b -m 2G -display none -nographic \
#     -serial mon:stdio -kernel target/petal.raspi400/release/kernel.bin

# Write the current selection to real hardware media.
# Dispatches on the board's flash-method; builds first.
flash: build
ifeq ($(FLASH_METHOD),sd)
	@tools/flash-sd.sh "$(SD)" "$(CONFIG)"
else ifeq ($(FLASH_METHOD),usb)
	$(MAKE) image-uefi CONFIG=$(CONFIG) ARCH=$(ARCH)
	$(write-usb)
else
	$(error board '$(BOARD)' does not declare a flash-method in configs/boards/$(BOARD).toml)
endif

# Backwards-compatible aliases for the old per-board targets.
# The raspi400 board is UEFI-only (the raspi400-uefi board was merged
# into it); both old SD targets now flash via the UEFI preparation,
# which builds the kernel for the selection it is given.
raspi400-build:
	cargo bin -m petal.raspi400
raspi400-sd:
	$(MAKE) flash CONFIG=petal.raspi400 SD=$(SD)
raspi400-uefi-sd:
	$(MAKE) flash CONFIG=busybox.raspi400 SD=$(SD)
jollac2-build:
	cargo bin -m petal.jolla-c2
jollac2-sd: jollac2-build
ifndef DISK
	$(error DISK is required. Usage: make jollac2-sd DISK=/dev/diskN)
endif
	@tools/jolla-c2/make-sdcard-image.sh "$(DISK)" "petal.jolla-c2"

# ── UEFI boot logo and firmware ─────────────────────────────────────────

# Regenerate the UEFI boot logo BMP from the SVG source.
# Requires: rsvg-convert (librsvg), magick (ImageMagick 7)
boot-logo:
	@tools/scripts/generate-boot-logo.sh

# Build custom pftf/RPi4 UEFI firmware with Zirconia boot logo.
# Requires: Docker (macOS) or gcc-aarch64-linux-gnu (Linux).
# The first build clones pftf/RPi4 (~1 GB) and takes ~5-10 minutes.
uefi-firmware:
	@tools/scripts/build-uefi-firmware.sh

# ── x86_64 QEMU ─────────────────────────────────────────────────────────
# x86_64 always boots via UEFI (OVMF) in QEMU, same as real hardware.
# Use the generic targets:
#   make build CONFIG=busybox.qemu-x86_64    — Linux flavour + busybox
#   make run   CONFIG=petal.qemu-x86_64      — Zircon-only + petal shell
#   make run   CONFIG=busybox.qemu-x86_64
x86-linux-build:
	$(MAKE) build CONFIG=busybox.qemu-x86_64 ARCH=x86_64
x86-zircon-build:
	$(MAKE) build CONFIG=petal.qemu-x86_64 ARCH=x86_64
x86-linux-run:
	$(MAKE) run CONFIG=busybox.qemu-x86_64 ARCH=x86_64
x86-zircon-run:
	$(MAKE) run CONFIG=petal.qemu-x86_64 ARCH=x86_64

# Create a UEFI-bootable disk image for x86_64 real hardware.
# Uses the x86-laptop board (no PCI, no UART -- uses framebuffer console).
# The image can be written to a USB drive with `make flash USB=`.
# The ramdisk decision follows the product's rootfs-contents manifest
# (visible in the image path printed by `rootfs-image --print-path`):
# full/core-tests images are embedded as the ramdisk; petal-manifest
# products boot their userspace from the embedded ZBI (no ramdisk),
# like the old MODE=zircon image.
# Usage: make image-uefi CONFIG=busybox.x86-laptop OUTPUT=/tmp/zcore-uefi.img
#        make image-uefi CONFIG=petal.x86-laptop OUTPUT=/tmp/zcore-uefi.img
OUTPUT ?= target/$(CONFIG)/release/kernel-uefi.img
image-uefi:
ifeq ($(BOARD),x86-laptop)
	cargo xtask rootfs-image -m $(CONFIG)
	cargo zcore-build -m $(CONFIG)
	@ROOTFS_IMG=$$(cargo xtask rootfs-image -m $(CONFIG) --print-path); \
	RAMDISK=auto; \
	case "$$ROOTFS_IMG" in \
		*/petal-*) RAMDISK=none ;; \
	esac; \
	KERNEL_ELF=$(KERNEL) ROOTFS_IMG="$$ROOTFS_IMG" \
		tools/scripts/x86-uefi-image.sh $(OUTPUT) $$RAMDISK
else
	$(error image-uefi supports x86-laptop boards (got CONFIG=$(CONFIG)))
endif

# Backwards-compatible alias.
x86-uefi-image:
	$(MAKE) image-uefi CONFIG=$(if $(filter zircon,$(MODE)),petal,busybox).x86-laptop ARCH=x86_64

# Create a PXE TFTP directory for x86_64 network boot.
# Usage: make x86-pxe
# Then run dnsmasq: sudo dnsmasq --no-daemon -C target/busybox.x86-laptop/pxe/dnsmasq.conf
PXE_DIR ?= target/busybox.x86-laptop/pxe/tftpboot
PXE_SUBNET ?= 192.168.1
x86-pxe:
	cargo xtask rootfs-image -m busybox.x86-laptop
	cargo zcore-build -m busybox.x86-laptop
	@cargo build --release --manifest-path tools/x86-bootimage/Cargo.toml
	@rm -rf $(PXE_DIR)/zcore
	@mkdir -p $(PXE_DIR)/zcore
	@tools/x86-bootimage/target/release/x86-bootimage \
		target/busybox.x86-laptop/release/kernel \
		$(PXE_DIR)/zcore \
		--ramdisk target/rootfs/images/full-x86_64.img \
		--pxe
	@mkdir -p target/busybox.x86-laptop/pxe
	@echo "port=0" > target/busybox.x86-laptop/pxe/dnsmasq.conf
	@echo "dhcp-range=$(PXE_SUBNET).0,proxy" >> target/busybox.x86-laptop/pxe/dnsmasq.conf
	@echo "dhcp-no-override" >> target/busybox.x86-laptop/pxe/dnsmasq.conf
	@echo "enable-tftp" >> target/busybox.x86-laptop/pxe/dnsmasq.conf
	@echo "tftp-root=$(abspath $(PXE_DIR))" >> target/busybox.x86-laptop/pxe/dnsmasq.conf
	@echo "dhcp-match=set:efi-x86_64,option:client-arch,7" >> target/busybox.x86-laptop/pxe/dnsmasq.conf
	@echo "dhcp-match=set:efi-x86_64,option:client-arch,9" >> target/busybox.x86-laptop/pxe/dnsmasq.conf
	@echo "dhcp-boot=tag:efi-x86_64,zcore/bootloader" >> target/busybox.x86-laptop/pxe/dnsmasq.conf
	@echo "log-dhcp" >> target/busybox.x86-laptop/pxe/dnsmasq.conf
	@echo ""
	@echo "==> PXE TFTP directory ready at $(PXE_DIR)"
	@echo "==> Start PXE server:"
	@echo "    sudo /opt/homebrew/opt/dnsmasq/sbin/dnsmasq --no-daemon -C target/busybox.x86-laptop/pxe/dnsmasq.conf"
	@echo "==> On laptop: F12 -> PXE Network Boot"

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

# Usage: make x86-uefi-usb-linux USB=/dev/disk5
x86-uefi-usb-linux:
	$(MAKE) flash CONFIG=busybox.x86-laptop ARCH=x86_64 USB=$(USB)

# Usage: make x86-uefi-usb-zircon USB=/dev/disk5
x86-uefi-usb-zircon:
	$(MAKE) flash CONFIG=petal.x86-laptop ARCH=x86_64 USB=$(USB)

# Zircon boot smoke test: build in Zircon mode, start QEMU, wait for
# userstart hello message, verify clean shutdown.
zircon-boot-test:
	@echo "==> Zircon boot smoke test ($(ARCH))..."
	@tools/scripts/zircon-boot-test.sh zircon-hello.qemu-$(ARCH) $(ARCH)

zircon-rootfs-test:
	@echo "==> Zircon rootfs boot test ($(ARCH))..."
	@tools/scripts/zircon-rootfs-test.sh zircon-hello.qemu-$(ARCH) $(ARCH)

# ── Fuchsia core-tests-standalone ──────────────────────────────────────
# Run core-tests-standalone on x86_64 in QEMU. This target handles the
# full build chain: userstart, petal ZBI, kernel (Zircon-only, no linux),
# rootfs with core-tests-standalone, boot image, and QEMU launch.
#
# The kernel is built WITHOUT the linux feature to avoid interference
# between Linux init_spawn and the Zircon rootfs boot path.
#
# Usage:
#   make core-tests                    # full run (15 min timeout)
#   make core-tests TIMEOUT=300        # custom timeout in seconds
#   make core-tests FILTER='PortTest.*' # run specific test suite
#
# Results are written to /tmp/qemu-core-tests.log.
# No default timeout — rely on CI job timeout-minutes.
# Override with: make core-tests CORE_TESTS_TIMEOUT=900
CORE_TESTS_TIMEOUT ?= 0
CORE_TESTS_LOG ?= /tmp/qemu-core-tests.log
CORE_TESTS_SKIP_FILE ?= tests/core-tests-skip.txt

# Build the skip filter from the skip-list file unless the caller
# supplied an explicit CORE_TESTS_FILTER.  The file uses one gtest
# pattern per line; comments (#) and blank lines are ignored.
ifndef CORE_TESTS_FILTER
  ifneq ($(wildcard $(CORE_TESTS_SKIP_FILE)),)
    CORE_TESTS_FILTER := $(shell sed '/^[[:space:]]*\#/d; /^[[:space:]]*$$/d' $(CORE_TESTS_SKIP_FILE) \
                            | paste -s -d ':' - | sed 's/^/-/')
  endif
endif
CORE_TESTS_FILTER ?=

# ── Core-tests build pipeline ────────────────────────────────────────
# Split into granular targets so Make skips unchanged steps.
# Use 'make core-tests-build' to build everything, or individual
# targets for faster iteration:
#   make core-tests-kernel   — rebuild kernel only
#   make core-tests-rootfs   — rebuild rootfs image only
#   make core-tests-bootimg  — rebuild boot image only

USERSTART_ELF := $(CURDIR)/target/userstart/x86_64-unknown-none/release/userstart
KERNEL_ELF    := target/core-tests.qemu-x86_64/release/kernel
ROOTFS_IMG    := target/rootfs/images/core-tests-x86_64.img
BOOT_IMG      := target/core-tests.qemu-x86_64/release/boot.img
EXTRA_ARGS    := target/rootfs/core-tests/x86_64/etc/extra-args
BOOTIMAGE_BIN := tools/x86-bootimage/target/release/x86-bootimage

# Step 1+2: Build userstart + kernel (cargo handles caching).
# The core-tests product (configs/products/core-tests.toml) provides the
# Zircon-only flavour, debugging-syscalls cmdline, and ROOTPROC.
.PHONY: core-tests-kernel
core-tests-kernel:
	@test -f prebuilt/zircon-test/x86_64/core-tests-standalone || \
		{ echo "ERROR: prebuilt/zircon-test/x86_64/core-tests-standalone not found"; exit 1; }
	@CARGO_ENCODED_RUSTFLAGS="-Crelocation-model=pic" \
		cargo build -p userstart --target x86_64-unknown-none \
		--release --target-dir target/userstart -Z build-std=core,alloc
	@USERSTART_ELF="$(USERSTART_ELF)" \
		cargo zcore-build -m core-tests.qemu-x86_64

# Step 3: Write extra-args filter file + build rootfs SFS image.
# Always compare the desired filter against the baked-in extra-args.
# If they differ (or if the rootfs image is missing), rebuild.
.PHONY: core-tests-rootfs
core-tests-rootfs:
	@mkdir -p target/rootfs/core-tests/x86_64/etc
	@REBUILD=0; \
	CURRENT=$$(cat $(EXTRA_ARGS) 2>/dev/null || true); \
	if [ -n "$(strip $(CORE_TESTS_FILTER))" ]; then \
		DESIRED="--gtest_filter=$(CORE_TESTS_FILTER)"; \
		if [ "$$DESIRED" != "$$CURRENT" ]; then \
			printf '%s\n' "$$DESIRED" > $(EXTRA_ARGS); \
			REBUILD=1; \
		fi; \
	else \
		if [ -n "$$CURRENT" ]; then \
			rm -f $(EXTRA_ARGS); \
			REBUILD=1; \
		fi; \
	fi; \
	if [ ! -f $(ROOTFS_IMG) ] || [ $$REBUILD -eq 1 ]; then \
		rm -f $(ROOTFS_IMG); \
		cargo xtask zircon-rootfs --arch x86_64 --manifest core-tests; \
	fi

# Step 4: Create UEFI boot image from kernel + rootfs.
.PHONY: core-tests-bootimg
core-tests-bootimg: core-tests-kernel core-tests-rootfs
	@cargo build --release --manifest-path tools/x86-bootimage/Cargo.toml
	@$(BOOTIMAGE_BIN) $(KERNEL_ELF) $(BOOT_IMG) --ramdisk $(ROOTFS_IMG)

# All steps together (backwards compatible).
core-tests-build: core-tests-bootimg

# Run a single test (or filter pattern) quickly.
# Usage: make core-test-run T='Pager.FailEarlyWake'
#        make core-test-run T='Pager.*' CORE_TEST_TIMEOUT=120
#        make core-test-run T='Pager.FailEarlyWake:Pager.ResizeCommitTest'
# Only rebuilds rootfs + boot image (not kernel).
# Set CORE_TEST_TIMEOUT (seconds, default 60) for longer suites.
.PHONY: core-test-run
core-test-run:
ifndef T
	$(error Usage: make core-test-run T=TestName [CORE_TEST_TIMEOUT=60])
endif
	@# Kill any stale QEMU using our boot image
	@pkill -f 'qemu-system-x86_64.*boot\.img' 2>/dev/null; sleep 1 || true
	@# Write the filter first: the manifest builder preserves etc/
	@# (it only clears bin/ and lib/), so extra-args survives the
	@# rootfs staging below.
	@mkdir -p target/rootfs/core-tests/x86_64/etc
	@printf '%s\n' '--gtest_filter=$(T)' > $(EXTRA_ARGS)
	@rm -f $(ROOTFS_IMG) $(BOOT_IMG)
	@cargo xtask zircon-rootfs --arch x86_64 --manifest core-tests > /dev/null 2>&1
	@$(BOOTIMAGE_BIN) $(KERNEL_ELF) $(BOOT_IMG) --ramdisk $(ROOTFS_IMG) > /dev/null 2>&1
	@echo "==> Filter: $(T)  Timeout: $${CORE_TEST_TIMEOUT:-60}s"
	@. tools/scripts/find-ovmf.sh && OVMF=$$(find_ovmf) && \
	qemu-system-x86_64 -m 4G -display none -no-reboot -nographic \
		-machine q35 -smp $(X86_CORES) \
		-cpu qemu64,+rdrand,+rdtscp,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
		-serial mon:stdio \
		-drive if=pflash,format=raw,readonly=on,file="$$OVMF" \
		-drive "format=raw,file=$(BOOT_IMG)" \
		> $(CORE_TESTS_LOG) 2>&1 & \
	PID=$$!; \
	TIMEOUT=$${CORE_TEST_TIMEOUT:-60}; \
	ELAPSED=0; LAST_P=0; \
	while true; do \
		if ! kill -0 $$PID 2>/dev/null; then break; fi; \
		if [ $$ELAPSED -ge $$TIMEOUT ]; then break; fi; \
		sleep 5; ELAPSED=$$((ELAPSED + 5)); \
		CUR_P=$$(sed 's/\x1b\[[0-9;]*m//g' $(CORE_TESTS_LOG) 2>/dev/null \
			| grep -c '\[       OK \]' || echo 0); \
		if [ "$$CUR_P" != "$$LAST_P" ]; then \
			printf '\r    %s passed (%ds)...' "$$CUR_P" "$$ELAPSED"; \
			LAST_P=$$CUR_P; \
		fi; \
	done; \
	printf '\n'; \
	kill $$PID 2>/dev/null; wait $$PID 2>/dev/null; \
	CLEAN=$(CORE_TESTS_LOG).clean; \
	tr -d '\000' < $(CORE_TESTS_LOG) | sed 's/\x1b\[[0-9;]*m//g' > $$CLEAN; \
	P=$$(grep -c '\[       OK \]' $$CLEAN || true); \
	F=$$(grep -c '\[  FAILED  \]' $$CLEAN || true); \
	R=$$(grep '\[==========\]' $$CLEAN | tail -1 | sed -n 's/.*\] \([0-9]*\) test.*/\1/p'); \
	echo "--- Results ($$ELAPSED s) ---"; \
	echo "Passed: $$P  Failed: $$F  Ran: $${R:-$$P}"; \
	if [ "$$F" -gt 0 ] 2>/dev/null; then \
		echo "Failed tests:"; \
		grep '\[  FAILED  \]' $$CLEAN | sed 's/.*\] /  /'; \
	fi; \
	LAST_RUN=$$(grep '\[ RUN      \]' $$CLEAN | tail -1 | sed 's/.*\] //'); \
	LAST_OK=$$(grep '\[       OK \]' $$CLEAN | tail -1 | sed 's/.*\] //' | sed 's/ (.*//'); \
	if [ "$$P" -eq 0 ] 2>/dev/null && [ "$$F" -eq 0 ] 2>/dev/null; then \
		echo "HANG: no tests completed (last started: $$LAST_RUN)"; exit 1; \
	elif [ -n "$$R" ] && [ "$$P" -eq "$$R" ] 2>/dev/null && [ "$$F" -eq 0 ] 2>/dev/null; then \
		echo "OK: all $$R tests passed"; \
	elif [ "$$F" -gt 0 ] 2>/dev/null; then \
		exit 1; \
	elif [ -z "$$R" ]; then \
		if [ -n "$$LAST_RUN" ] && [ "$$LAST_RUN" != "$$LAST_OK" ]; then \
			echo "HANG: stuck on $$LAST_RUN (after $$LAST_OK)"; exit 1; \
		else \
			echo "OK: $$P tests passed (QEMU exited before summary)"; \
		fi; \
	else \
		echo "INCOMPLETE: $$P/$$R passed, hung on: $$LAST_RUN"; exit 1; \
	fi

# Build and run the full core-tests suite.
core-tests: core-tests-build
	@pkill -f 'qemu-system-x86_64.*boot\.img' 2>/dev/null; sleep 1 || true
	@echo "==> Running QEMU (timeout=$(CORE_TESTS_TIMEOUT)s, log=$(CORE_TESTS_LOG))..."
	@. tools/scripts/find-ovmf.sh && OVMF=$$(find_ovmf) && \
	qemu-system-x86_64 -m 4G -display none -no-reboot -nographic \
		-machine q35 -smp $(X86_CORES) \
		-cpu qemu64,+rdrand,+rdtscp,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
		-serial mon:stdio \
		-drive if=pflash,format=raw,readonly=on,file="$$OVMF" \
		-drive "format=raw,file=target/core-tests.qemu-x86_64/release/boot.img" \
		> $(CORE_TESTS_LOG) 2>&1 & \
	PID=$$!; \
	ELAPSED=0; \
	TIMEOUT=$(CORE_TESTS_TIMEOUT); \
	LAST_P=0; \
	while true; do \
		if ! kill -0 $$PID 2>/dev/null; then break; fi; \
		if [ "$$TIMEOUT" -gt 0 ] 2>/dev/null && [ $$ELAPSED -ge $$TIMEOUT ]; then break; fi; \
		sleep 5; ELAPSED=$$((ELAPSED + 5)); \
		CUR_P=$$(grep -c '\[       OK \]' $(CORE_TESTS_LOG) 2>/dev/null || echo 0); \
		if [ "$$CUR_P" != "$$LAST_P" ]; then \
			printf '\r    %s tests passed (%ds)...' "$$CUR_P" "$$ELAPSED"; \
			LAST_P=$$CUR_P; \
		fi; \
	done; \
	printf '\n'; \
	kill $$PID 2>/dev/null; wait $$PID 2>/dev/null; \
	CLEAN=$(CORE_TESTS_LOG).clean; \
	tr -d '\000' < $(CORE_TESTS_LOG) | sed 's/\x1b\[[0-9;]*m//g' > $$CLEAN; \
	echo "=== Core-tests results ($$ELAPSED s) ==="; \
	P=$$(grep -c '\[       OK \]' $$CLEAN || true); \
	F=$$(grep -c '\[  FAILED  \]' $$CLEAN || true); \
	R=$$(grep '\[==========\]' $$CLEAN | tail -1 | sed -n 's/.*\] \([0-9]*\) test.*/\1/p'); \
	if [ -z "$$R" ]; then R=$$(($$P + $$F)); PARTIAL=" (no summary — partial run)"; else PARTIAL=""; fi; \
	S=$$(sed '/^[[:space:]]*\#/d; /^[[:space:]]*$$/d' $(CORE_TESTS_SKIP_FILE) 2>/dev/null | wc -l | tr -d ' '); \
	T=$$(($$R + $$S)); \
	echo "Passed: $$P  Failed: $$F  Ran: $$R/$$T  Skipped: $$S$$PARTIAL"; \
	grep '\[  FAILED  \]' $$CLEAN || true; \
	HUNG=$$(grep '\[ RUN      \]' $$CLEAN | tail -1 | sed 's/.*\] //'); \
	LAST_OK=$$(grep '\[       OK \]' $$CLEAN | tail -1 | sed 's/.*\] //' | sed 's/ (.*//'); \
	echo "Last completed: $$LAST_OK"; \
	echo "Last started:   $$HUNG"; \
	tail -5 $$CLEAN; \
	echo "=== Kernel diagnostics ([excp]/[ud]/scan) ==="; \
	grep -E '\[excp\]|\[ud\]|scan: cpu=' $$CLEAN | tail -30 || echo "(none)"; \
	if [ "$$F" -gt 0 ] 2>/dev/null; then echo "FAIL: $$F test(s) failed"; exit 1; fi; \
	if [ -z "$$R" ] || [ "$$R" -eq 0 ] 2>/dev/null; then echo "FAIL: no tests ran"; exit 1; fi; \
	if [ -n "$$PARTIAL" ]; then \
		echo "FAIL: incomplete run ($$P passed of $$T expected) — hung on: $$HUNG (after: $$LAST_OK)"; exit 1; \
	fi

# Run all tests: boot smoke, libc conformance, and the petal
# user-mode test suite (single QEMU boot).
test: boot-test libc-test petal-tests

# ── Bare-metal Rust zircon-tests ─────────────────────────────────────
# Build and run the Rust zircon-tests suite inside QEMU on bare-metal.
# The `rust-tests` product (configs/products/rust-tests.toml) adds the
# `baremetal-test` feature: the kernel runs the suite instead of an
# init process. Drivers and target spec come from the board.
# Usage: make rust-tests RUST_TESTS_ARCH=aarch64
RUST_TESTS_ARCH ?= aarch64
RUST_TESTS_CONFIG = rust-tests.qemu-$(RUST_TESTS_ARCH)

.PHONY: rust-tests-build rust-tests
rust-tests-build:
	@echo "==> Building kernel with baremetal-test for $(RUST_TESTS_ARCH)..."
	cargo bin -m $(RUST_TESTS_CONFIG)
ifeq ($(RUST_TESTS_ARCH),x86_64)
	@cargo build --release --manifest-path tools/x86-bootimage/Cargo.toml
	@tools/x86-bootimage/target/release/x86-bootimage \
		target/$(RUST_TESTS_CONFIG)/release/kernel \
		target/$(RUST_TESTS_CONFIG)/release/boot.img
endif

rust-tests: rust-tests-build
	@tools/scripts/rust-tests-qemu.sh $(RUST_TESTS_ARCH) $(RUST_TESTS_CONFIG)

# ── Petal user-mode tests ──────────────────────────────────────────────
# Run a single petal user-mode test binary in QEMU (the 66 syscall-layer
# tests ported from zircon-tests; see configs/products + issue #706).
# Usage: make petal-test BIN=counter-test [ARCH=aarch64|x86_64]
petal-test:
ifndef BIN
	$(error BIN is required. Usage: make petal-test BIN=counter-test [ARCH=aarch64])
endif
	@tools/scripts/petal-test.sh $(ARCH) $(BIN)

# Run the FULL petal user-mode test suite in a single QEMU boot
# (all 65 binaries, one kernel, one rootfs). This is the CI entry
# point — the petal equivalent of `make rust-tests`.
# Usage: make petal-tests [ARCH=aarch64|x86_64]
petal-tests:
	@tools/scripts/petal-tests.sh $(ARCH)

# Boot smoke test: start QEMU, wait for the "/ # " shell prompt, exit.
# Proves: boot assembly, MMU, HAL, VirtIO, filesystem, ELF loader, and
# busybox shell all work end-to-end. Timeout is 60 seconds.
# ARCH is derived from the selected board; a conflicting explicit ARCH
# is rejected before building (busybox-test and libc-test inherit this).
.PHONY: check-arch
check-arch:
	@if [ -n "$(BOARD_ARCH)" ] && [ "$(BOARD_ARCH)" != "host" ] && [ "$(ARCH)" != "$(BOARD_ARCH)" ]; then \
		echo "ERROR: ARCH=$(ARCH) conflicts with board '$(BOARD)' (arch $(BOARD_ARCH)) — omit ARCH, it is derived from CONFIG"; \
		exit 1; \
	fi

boot-test: check-arch build
	@echo "==> Boot smoke test ($(CONFIG), $(ARCH))..."
	@tools/scripts/boot-test.sh $(CONFIG) $(ARCH)

# SMP testing is now integrated into boot-test and other CI targets
# via the cores count in configs/boards/<board>.toml.

# Run busybox applet tests: echo, ls, cat, pipes, etc.
# Verifies that common busybox commands work end-to-end in QEMU.
# Depends on boot-test to ensure serialization under parallel make.
busybox-test: boot-test
	@echo "==> Busybox applet test ($(CONFIG))..."
	@tools/scripts/busybox-test.sh $(CONFIG) $(ARCH)

# Run musl libc-test functional tests. Reports pass/fail counts but does
# not fail the build — the pass rate is expected to improve over time as
# more syscalls are implemented (see issue #16).
# Depends on boot-test to ensure serialization under parallel make.
libc-test: boot-test
	@tools/scripts/libc-test.sh $(CONFIG) $(ARCH)

# LibOS mode: runs zCore as a host process (no QEMU needed).
# Requires x86_64 host (Linux or macOS) or aarch64 Linux.
# Two personalities: Linux (busybox shell) and Zircon (petal programs).

# Build libos in Linux mode (busybox product)
libos-build-linux:
	cargo zcore-build -m busybox.libos

# Build libos in Zircon mode (petal product; builds userstart + petal
# automatically)
libos-build-zircon:
	cargo zcore-build -m petal.libos

# Run libos in Linux mode with busybox shell
libos-run-linux:
	cargo linux-libos --args "/bin/busybox?sh"

# Run libos in Zircon mode (known broken -- see #281)
libos-run-zircon:
	cargo zcore-build -m petal.libos
	./target/petal.libos/release/kernel

# ── Environment setup ──────────────────────────────────────────────────
# Install all host dependencies needed to build and test zCore:
# QEMU emulators, UEFI firmware (OVMF + aarch64), mtools, e2fsprogs,
# rustup cross-targets, and (on macOS) the musl cross-toolchains.
# Run once after cloning. See tools/scripts/setup.sh for details.
setup:
	@tools/scripts/setup.sh

# Backwards-compatible alias (CI and old docs use `make config`).
config: setup

# install cross-compilation toolchain on macOS via Homebrew
# (invoked automatically by `make setup` on Darwin)
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

# put rootfs for linux mode (full manifest, arch-keyed)
rootfs:
ifeq ($(XTASK), 1)
	cargo rootfs --arch $(ARCH)
endif

# put other tests into rootfsw
other-test:
	cargo other-test --arch $(ARCH)

# Pre-push check: mirrors all CI jobs locally.
# Run this before every push to avoid CI failures.
# Fast pre-push: clippy + fmt + builds (no QEMU tests).
# Catches ~90% of CI failures in ~3 minutes.
pre-push-quick:
	@echo "==> [1/10] Clippy (all selections + personalities)..."
	cargo check-style
	@echo "==> [2/10] Workspace build..."
	cargo build
	@echo "==> [3/10] Unit tests..."
	cargo test --no-fail-fast
	@echo "==> [4/10] zircon-object host tests (libos + aspace-separate)..."
	cargo test -p zircon-object --features libos,aspace-separate --no-fail-fast
	@echo "==> [5/10] hal-impl host tests (libos)..."
	cargo test -p hal-impl --features libos --no-fail-fast
	@echo "==> [6/10] Zircon syscall tests (libos)..."
	cargo test -p zircon-tests --no-fail-fast -- --test-threads=1
	@echo "==> [7/10] Bare-metal aarch64..."
	$(MAKE) build CONFIG=busybox.qemu-aarch64 ARCH=aarch64
	@echo "==> [8/10] Bare-metal x86_64..."
	$(MAKE) build CONFIG=busybox.qemu-x86_64 ARCH=x86_64
	@echo "==> [9/10] Hardware targets..."
	cargo bin -m petal.raspi400
	cargo bin -m petal.x86-laptop
	@echo "==> [10/10] Done."
	@echo "==> Quick pre-push checks passed."

# Full pre-push: everything CI checks (slow — includes QEMU boot tests).
# Requires: qemu-system-aarch64, qemu-system-x86_64.
pre-push:
	@echo "==> [1/21] Clippy (all selections + personalities)..."
	cargo check-style
	@echo "==> [2/21] Workspace build..."
	cargo build
	@echo "==> [3/21] Unit tests..."
	cargo test --no-fail-fast
	@echo "==> [4/21] zircon-object host tests (libos + aspace-separate)..."
	cargo test -p zircon-object --features libos,aspace-separate --no-fail-fast
	@echo "==> [5/21] hal-impl host tests (libos)..."
	cargo test -p hal-impl --features libos --no-fail-fast
	@echo "==> [6/21] Zircon syscall tests (libos)..."
	cargo test -p zircon-tests --no-fail-fast -- --test-threads=1
	@echo "==> [7/21] Bare-metal aarch64 (build + rootfs)..."
	$(MAKE) build CONFIG=busybox.qemu-aarch64 ARCH=aarch64
	@echo "==> [8/21] Boot smoke test (aarch64)..."
	$(MAKE) boot-test CONFIG=busybox.qemu-aarch64 ARCH=aarch64
	@echo "==> [9/21] Bare-metal riscv64..."
	cargo zcore-build -m busybox.qemu-riscv64
	@echo "==> [9.5/21] Boot smoke test (riscv64)..."
	tools/scripts/riscv64-boot-test.sh busybox.qemu-riscv64
	@echo "==> [10/21] Bare-metal x86_64 (build + rootfs)..."
	$(MAKE) build CONFIG=busybox.qemu-x86_64 ARCH=x86_64
	@echo "==> [11/21] Boot smoke test (x86_64)..."
	$(MAKE) boot-test CONFIG=busybox.qemu-x86_64 ARCH=x86_64
	@echo "==> [12/21] Bare-metal x86_64 zircon..."
	cargo bin -m petal.qemu-x86_64
	@echo "==> [13/21] Dual flavour (build-only)..."
	cargo bin -m busybox.qemu-aarch64
	@echo "==> [14/21] LibOS (Linux + Zircon)..."
	cargo zcore-build -m busybox.libos
	cargo zcore-build -m petal.libos
	@echo "==> [15/21] Hardware targets (build-only)..."
	cargo bin -m petal.raspi400
	cargo bin -m petal.x86-laptop
	@echo "==> [16/21] Zircon boot test (aarch64)..."
	$(MAKE) zircon-boot-test ARCH=aarch64
	@tools/scripts/zircon-rootfs-test.sh zircon-hello.qemu-aarch64 aarch64
	@echo "==> [17/21] Zircon boot test (x86_64)..."
	$(MAKE) zircon-boot-test ARCH=x86_64
	@echo "==> [18/21] Libc tests (aarch64 + x86_64)..."
	$(MAKE) libc-test CONFIG=busybox.qemu-aarch64 ARCH=aarch64
	$(MAKE) libc-test CONFIG=busybox.qemu-x86_64 ARCH=x86_64
	@echo "==> [19/21] Check all feature combinations..."
	$(MAKE) check-all-features
	@echo "==> [20/21] Petal user-mode test suite (aarch64 + x86_64)..."
	$(MAKE) petal-tests ARCH=aarch64
	$(MAKE) petal-tests ARCH=x86_64
	@echo "==> [21/21] Done."
	@echo "==> All pre-push checks passed."

# Run clippy for all selections (catches cross-platform issues).
# Features come from configs/ via xtask.
clippy-all:
	cargo check-style

# check code style
check:
	cargo check-style

# Audit 1:1 correspondence between Rust core_tests and C++ core-tests
audit-tests:
	cargo audit-tests

# Check all feature combinations on host-buildable crates.
# Requires: cargo install cargo-all-features
check-all-features:
	cargo all-features check

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

# Clone and build the Fuchsia project in ../fuchsia for testing alongside zCore.
# Requires: x86-64 Linux host with curl, git, unzip installed.
# Cross-compiles Fuchsia for aarch64 (qemu-arm64 board).
FUCHSIA_DIR := $(CURDIR)/../fuchsia
build-fuchsia:
	@echo "==> Checking prerequisites..."
	@which curl >/dev/null 2>&1 || { echo "ERROR: curl not found"; exit 1; }
	@which git >/dev/null 2>&1 || { echo "ERROR: git not found"; exit 1; }
	@which unzip >/dev/null 2>&1 || { echo "ERROR: unzip not found"; exit 1; }
	@if [ "$$(uname -s)" != "Linux" ]; then \
		echo "ERROR: Fuchsia can only be built on Linux (detected: $$(uname -s))"; \
		exit 1; \
	fi
	@if [ "$$(uname -m)" != "x86_64" ]; then \
		echo "ERROR: Fuchsia build requires x86_64 host (detected: $$(uname -m))"; \
		exit 1; \
	fi
	@if [ ! -d "$(FUCHSIA_DIR)/fuchsia/.jiri_root" ]; then \
		echo "==> Cloning Fuchsia source into $(FUCHSIA_DIR)..."; \
		mkdir -p "$(FUCHSIA_DIR)"; \
		cd "$(FUCHSIA_DIR)" && \
		test -d fuchsia || git clone https://fuchsia.googlesource.com/fuchsia && \
		cd fuchsia && \
		bash scripts/bootstrap; \
	else \
		echo "==> Fuchsia source already exists at $(FUCHSIA_DIR)/fuchsia, skipping clone."; \
	fi
	@echo "==> Configuring Fuchsia build...(fx set workbench_eng.x64)"
	@export PATH="$(FUCHSIA_DIR)/fuchsia/fuchsia/.jiri_root/bin:$PATH"
	@jiri init -analytics-opt=false "$(FUCHSIA_DIR)/fuchsia/fuchsia"
	@cd "$(FUCHSIA_DIR)/fuchsia/fuchsia" && \
		source scripts/fx-env.sh && \
		.jiri_root/bin/fx set workbench_eng.x64
	@echo "==> Building Fuchsia..."
	@cd "$(FUCHSIA_DIR)/fuchsia/fuchsia" && \
		source scripts/fx-env.sh && \
		.jiri_root/bin/fx build
	@echo "==> Fuchsia build complete at $(FUCHSIA_DIR)/fuchsia"
