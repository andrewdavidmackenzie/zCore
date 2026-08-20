# zCore Documentation Index

## Overview

zCore is a Rust reimplementation of the Zircon
microkernel (from Google's Fuchsia OS) that also
supports running Linux userspace programs. It targets
three CPU architectures (aarch64, riscv64, x86_64) and
can run both bare-metal and as a "library OS" on a host
system.

The project is organized as a Cargo workspace with 12
member crates plus 2 excluded standalone projects and
2 git submodules.

## Architecture and Design

| Document                                             | Description                                                                                 |
|------------------------------------------------------|---------------------------------------------------------------------------------------------|
| [architecture.md](architecture.md)                   | Project architecture: all crates, purposes, dependencies, structure, and usage status       |
| [boot-process.md](boot-process.md)                   | Boot sequence for each platform (aarch64, riscv64, x86_64, libos) and common post-boot flow |
| [hal-design.md](hal-design.md)                       | HAL macro-based dispatch, interface modules, KernelHandler callbacks, platform code split   |
| [dependency-tree.md](dependency-tree.md)             | Workspace crate dependency hierarchy, visual graph, feature-gated dependencies              |
| [external-dependencies.md](external-dependencies.md) | All external (non-workspace) crate dependencies classified by role                          |

## Guides

| Document                                     | Description                                                                                  |
|----------------------------------------------|----------------------------------------------------------------------------------------------|
| [libos.md](libos.md)                         | LibOS mode: how it works, running, testing, debugging, limitations                           |
| [build-artifacts.md](build-artifacts.md)     | Build outputs, generated files, cached artifacts, clean targets                              |
| [non-crate-folders.md](non-crate-folders.md) | Non-Rust directories: .cargo, .github, config, scripts, prebuilt, rootfs, tools              |
| [for-developers.md](for-developers.md)       | Developer conventions: toolchain strategy, code quality, dependency management, features/cfg |

## Board-Specific Deployment Guides

| Document                                     | Board                           | Architecture |
|----------------------------------------------|---------------------------------|--------------|
| [README-D1.md](README-D1.md)                 | Allwinner D1 (C906)             | riscv64      |
| [README-C910.md](README-C910.md)             | T-HEAD C910 Light               | riscv64      |
| [README-fu740.md](README-fu740.md)           | SiFive FU740 (HiFive Unmatched) | riscv64      |
| [README-visionfive.md](README-visionfive.md) | StarFive VisionFive             | riscv64      |

## Other Resources

| Resource                           | Description                     |
|------------------------------------|---------------------------------|
| [porting-rv64.md](porting-rv64.md) | RISC-V 64 porting log and notes |
| [structure.svg](structure.svg)     | Architecture diagram            |
