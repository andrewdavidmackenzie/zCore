# petal -- minimal userspace for testing the zCore Zircon kernel
#
# Note: This build is currently non-functional. It requires either:
#   - Fuchsia sysroot libraries in prebuilt/zircon/{arch}/ (see #86)
#   - Or a switch to #![no_std] with raw Zircon syscalls (see #89)
#
# The ZBI packaging also requires the zbi CLI tool or the Rust ZBI
# construction from #121.

MODE ?= debug
ARCH ?= x86_64

zbi_out := target/petal.zbi
build_dir := target/$(ARCH)-unknown-fuchsia/$(MODE)
bootfs := $(build_dir)/bootfs
bins := $(patsubst src/bin/%.rs, $(bootfs)/bin/%, $(wildcard src/bin/*.rs))

ifeq ($(MODE), release)
  build_args += --release
endif

.PHONY: all build clean

all: build

build:
	cargo build $(build_args)

$(bootfs)/bin/%: $(build_dir)/%
	mkdir -p $(bootfs)/bin
	cp $^ $@

clean:
	cargo clean
