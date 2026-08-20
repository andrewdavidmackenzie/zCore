# Developer Guidelines (Draft)

This document describes development strategies for
zCore and reminds developers of important conventions.

> This is a draft. Feedback is welcome on GitHub
> [discussions](https://github.com/rcore-os/zCore/discussions/356).

## Contents

- [Toolchain Support Strategy](#toolchain-support-strategy)
  - Tracking toolchain update schedule
  - Unstable features used, and why
- [Code Quality](#code-quality)
  - How to use `#[deny(...)]` and `#[allow(...)]`
- [Dependency Management](#dependency-management)
  - When and how to introduce dependencies
- [Features and cfg](#features-and-cfg)
  - How to use conditional compilation

## Toolchain Support Strategy

Due to the [unstable features](#unstable-features-used)
described below, this project requires a nightly
toolchain. The repository records the tested toolchain
version in the Cargo-recognized
[toolchain configuration file](../rust-toolchain.toml).
Using `cargo` to build the project will automatically
install the correct toolchain.

Although the toolchain is nightly, it is updated in
sync with stable releases. Each time a new stable
version is released, the default toolchain is updated
to the nightly closest to the new stable fork point
(approximately every 6 weeks). Refer to the
[Rust Forge](https://forge.rust-lang.org/) for the
latest stable fork dates and use the corresponding
nightly.

> The expected update dates in this document are
> updated with each toolchain change.

### Unstable Features Used

#### [`doc_cfg`](https://doc.rust-lang.org/unstable-book/language-features/doc-cfg.html)

Used in: `zcore-drivers`, `kernel-hal`, `zcore-loader`

Marks platform availability information in generated
documentation. Improves documentation quality but does
not affect functionality. Can be removed if necessary.

#### [`naked_functions`](https://doc.rust-lang.org/unstable-book/language-features/naked-functions.html), [`asm_sym`](https://doc.rust-lang.org/unstable-book/language-features/asm-sym.html), [`asm_const`](https://doc.rust-lang.org/unstable-book/language-features/asm-const.html)

Used in: `zcore`

These three features support Rust naked functions.
Naked functions do not automatically insert stack
operations, making them suitable for pre-stack-setup
boot stages. Combined with `asm_const` (importing
Rust constants into inline assembly) and `asm_sym`
(importing Rust symbols into inline assembly), the
entire boot stage can be kept under Rust's protection
as much as possible (avoiding hardcoded constants or
exported global symbols).

Can be removed, but not recommended.

#### [`default_alloc_error_handler`](https://doc.rust-lang.org/unstable-book/language-features/default-alloc-error-handler.html)

Used in: `zcore`

Requests that `alloc` provide a default allocation
failure callback. Using both `no_std` and `alloc`
requires an allocation failure callback -- either the
default one or a custom one via the
[`alloc-error-handler`](https://doc.rust-lang.org/unstable-book/language-features/alloc-error-handler.html)
feature.

## Code Quality

Code quality is primarily maintained through
**clippy**. To support effective clippy usage, three
practices are followed:

1. **rust-analyzer integration**

   If using VS Code, rust-analyzer supports
   checkOnSave -- automatic checking on file save.
   Combined with auto-save, this provides continuous
   checking. Setting the check command to clippy
   enables continuous linting.

   Note: clippy can be slow on lower-end machines.

2. **`#![deny(warnings)]`**

   This option treats all warnings in the current
   module (and submodules) as errors, preventing code
   with warnings from compiling. This catches issues
   early. Clippy warnings are also treated as errors.

   Currently, all modules have this option enabled and
   compile cleanly. During development, it's fine to
   comment them out, but they must be restored before
   submitting a PR.

3. **GitHub Actions**

   Clippy checking is part of the CI workflow and
   runs on every commit. This proves to other
   developers that your code quality is maintained.
   Run `cargo check-style` locally to verify
   compliance -- it follows the same process as the
   CI workflow.

## Dependency Management

Dependencies, from most special to most general, fall
into four categories:

1. [Git submodules](#git-submodules)
2. [Personal repository cargo projects](#personal-repository-cargo-projects)
3. [Organization repository cargo projects](#organization-repository-cargo-projects)
4. [crates.io published crates](#cratesio-published-crates)

### Git Submodules

The most specialized form of dependency. Required for
projects that don't use Cargo (e.g., non-Rust
projects). Currently used for:

- **libc-test** -- musl libc test suite (C language)
- **rboot** -- UEFI bootloader (Rust, but a legacy
  submodule arrangement to be resolved)

### Personal Repository Cargo Projects

A specialized dependency form. Use only when a project
meets one of these criteria:

- Experimental, unstable, or may be abandoned
- Stable but unsuitable for crates.io publication
  and no organization willing to host it
- Forked from an organization project with a
  submitted but unmerged PR
- Permanently forked from an organization project
  for well-justified reasons

These situations should be actively resolved when
possible. Commit hashes must be pinned.

### Organization Repository Cargo Projects

The normal dependency form. Should be published to
crates.io when possible. Commit hashes must be pinned.

### crates.io Published Crates

The normal dependency form. Can be used freely.
Track the latest versions when possible. Document
reasons when a dependency cannot be updated.

## Features and cfg

zCore supports multiple platforms with different
hardware, making conditional compilation unavoidable.
However, improper use of compile-time options can
cause confusion or reduce test coverage. Follow these
guidelines for `#[cfg(...)]`:

1. For platform-only differences, use `target_arch`
2. For factors influenced by other conditions,
   consider dynamic detection (e.g., device tree)
3. When dynamic detection is not possible, use
   features -- but document the reason in the crate's
   entry point (`lib.rs` or `main.rs`)
4. For platform-specific features, add a constraint
   instead of using
   `all(target_arch = ..., feature = ...)`:

   ```rust
   #[cfg(all(
       feature = "sbi",
       not(target_arch = "riscv64")
   ))]
   compile_error!(
       "`sbi` is only available on RISC-V platforms"
   );
   ```

> Existing code does not fully meet these standards
> and will be gradually corrected.
