//! Build petal userspace programs and package them into a ZBI.

use crate::arch::Arch;
use crate::PROJECT_DIR;
use os_xtask_utils::dir;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Copy `src` to `dest` only if `src` is newer than `dest` (or `dest`
/// doesn't exist). This preserves destination mtimes when the source
/// hasn't changed, allowing downstream staleness checks to skip work.
fn copy_if_newer(src: &Path, dest: &Path) -> bool {
    let dominated = dest.is_file()
        && src
            .metadata()
            .and_then(|s| {
                dest.metadata()
                    .and_then(|d| Ok(s.modified()? <= d.modified()?))
            })
            .unwrap_or(false);
    if dominated {
        return false;
    }
    std::fs::copy(src, dest).unwrap_or_else(|e| {
        panic!(
            "failed to copy {} to {}: {}",
            src.display(),
            dest.display(),
            e
        )
    });
    true
}

/// ELF OS/ABI value for zCore Zircon flavour binaries.
///
/// Set in `e_ident[EI_OSABI]` (byte 7) of petal ELF binaries at build time.
/// The kernel's `execve` checks this byte to determine whether to use
/// Linux or Zircon syscall dispatch for the new process.
///
/// Value 0xFC is in the OS-specific range (64-255) of the ELF spec.
pub const ELFOSABI_ZIRCON: u8 = 0xFC;

/// Target triple for each architecture when building petal programs.
fn petal_target(arch: Arch) -> &'static str {
    match arch {
        Arch::Aarch64 => "aarch64-unknown-none-softfloat",
        Arch::Riscv64 => "riscv64gc-unknown-none-elf",
        Arch::X86_64 => "x86_64-unknown-none",
    }
}

/// Build multiple petal programs for the given architecture in one
/// cargo invocation, then patch each ELF's OS/ABI byte. Returns the
/// output directory containing the built binaries.
pub fn build_petal_batch(arch: Arch, names: &[String]) {
    if names.is_empty() {
        return;
    }
    let target = petal_target(arch);
    println!(
        "Building {} petal programs for {} (target: {})",
        names.len(),
        arch.name(),
        target
    );

    let target_dir = PROJECT_DIR.join("target/petal");
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--release"])
        .args(["-p", "petal"])
        .arg("--target")
        .arg(target)
        .arg("--target-dir")
        .arg(&target_dir);
    for name in names {
        cmd.arg("--bin").arg(name);
    }
    cmd.args(["-Z", "build-std=core,alloc"])
        .args(["-Z", "build-std-features=compiler-builtins-mem"])
        .env("RUSTFLAGS", "-C relocation-model=static");
    let status = cmd
        .status()
        .expect("failed to run cargo build for petal");
    if !status.success() {
        panic!("petal batch build failed");
    }

    for name in names {
        let elf_path = target_dir.join(target).join("release").join(name);
        set_elf_osabi(&elf_path, ELFOSABI_ZIRCON);
    }
}

/// Path to the petal output directory for a given architecture.
fn petal_output_dir(arch: Arch) -> PathBuf {
    PROJECT_DIR.join("target").join("petal").join(arch.name())
}

/// Build a petal program for the given architecture.
/// Returns the path to the compiled ELF binary.
pub fn build_petal(arch: Arch, bin_name: &str) -> PathBuf {
    let target = petal_target(arch);
    println!(
        "Building petal '{}' for {} (target: {})",
        bin_name,
        arch.name(),
        target
    );

    let target_dir = PROJECT_DIR.join("target/petal");
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .args(["-p", "petal"])
        .arg("--target")
        .arg(target)
        .arg("--target-dir")
        .arg(&target_dir)
        .arg("--bin")
        .arg(bin_name)
        .args(["-Z", "build-std=core,alloc"])
        .args(["-Z", "build-std-features=compiler-builtins-mem"])
        .env("RUSTFLAGS", "-C relocation-model=static")
        .status()
        .expect("failed to run cargo build for petal");

    if !status.success() {
        panic!("petal build failed");
    }

    let elf_path = target_dir.join(target).join("release").join(bin_name);
    set_elf_osabi(&elf_path, ELFOSABI_ZIRCON);
    elf_path
}

/// ELF e_ident field indices.
const EI_OSABI: u64 = 7;

/// ELF magic number.
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// Patch the ELF OS/ABI byte (`e_ident[EI_OSABI]`) in an ELF binary.
fn set_elf_osabi(path: &std::path::Path, osabi: u8) {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap_or_else(|e| panic!("failed to open ELF {}: {}", path.display(), e));
    let mut header = [0u8; 8];
    file.read_exact(&mut header)
        .unwrap_or_else(|e| panic!("failed to read ELF header: {}", e));
    assert!(
        header[0..4] == ELF_MAGIC,
        "Not an ELF file: {}",
        path.display()
    );
    file.seek(SeekFrom::Start(EI_OSABI)).unwrap();
    file.write_all(&[osabi]).unwrap();
}

/// Strip an ELF binary to a flat binary using objcopy.
/// Returns the path to the flat binary.
/// NOTE: No longer used for ZBI packaging (full ELF packaged instead, see #241).
/// Kept for potential future use by other tools.
#[allow(dead_code)]
fn strip_to_flat_binary(elf_path: &std::path::Path, arch: Arch, bin_name: &str) -> PathBuf {
    let out_dir = petal_output_dir(arch);
    std::fs::create_dir_all(&out_dir).unwrap();
    let flat_path = out_dir.join(format!("{}.bin", bin_name));

    println!(
        "Stripping {} -> {}",
        elf_path.display(),
        flat_path.display()
    );

    // Use rust-objcopy (from cargo-binutils) or llvm-objcopy
    let objcopy = find_objcopy();
    let status = Command::new(&objcopy)
        .args(["-O", "binary"])
        .arg(elf_path)
        .arg(&flat_path)
        .status()
        .unwrap_or_else(|e| panic!("Failed to run {}: {}", objcopy, e));

    if !status.success() {
        panic!("objcopy failed");
    }

    flat_path
}

/// Find an objcopy tool, checking PATH and the Rust toolchain's llvm-tools.
#[allow(dead_code)]
fn find_objcopy() -> String {
    // Try common names in PATH
    for name in ["rust-objcopy", "llvm-objcopy"] {
        if Command::new(name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            return name.to_string();
        }
    }

    // Try to find llvm-objcopy in the Rust toolchain (installed by llvm-tools-preview)
    if let Ok(output) = Command::new("rustc").args(["--print", "sysroot"]).output() {
        let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let lib_dir = PathBuf::from(&sysroot).join("lib/rustlib");
        if let Ok(entries) = std::fs::read_dir(&lib_dir) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("bin/llvm-objcopy");
                if candidate.exists() {
                    let path = candidate.to_string_lossy().to_string();
                    println!("Found objcopy: {}", path);
                    return path;
                }
            }
        }
    }

    panic!(
        "No objcopy found. Install llvm-tools-preview: \
         rustup component add llvm-tools-preview"
    );
}

/// Build the userstart binary for the given architecture.
/// Returns the path to the compiled ELF binary.
pub fn build_userstart(arch: Arch) -> PathBuf {
    let target = petal_target(arch);
    println!(
        "Building userstart for {} (target: {})",
        arch.name(),
        target
    );

    let target_dir = PROJECT_DIR.join("target/userstart");
    // Use PIC code generation so the static-PIE linker can emit
    // proper relocations for all absolute addresses. Without this,
    // aarch64's default relocation-model=static generates ABS64
    // relocations in .rodata that the PIE linker rejects.
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .args(["-p", "userstart"])
        .arg("--target")
        .arg(target)
        .arg("--target-dir")
        .arg(&target_dir)
        .args(["-Z", "build-std=core,alloc"])
        .env("CARGO_ENCODED_RUSTFLAGS", "-Crelocation-model=pic")
        .status()
        .expect("failed to run cargo build for userstart");

    if !status.success() {
        panic!("userstart build failed");
    }

    let elf_path = target_dir.join(target).join("release").join("userstart");
    set_elf_osabi(&elf_path, ELFOSABI_ZIRCON);
    elf_path
}

/// Copy petal hello binary into the Linux rootfs so Zircon binaries
/// can be tested from the Linux busybox shell.
/// Copy demo binaries into the Linux rootfs for cross-flavour testing.
/// Adds both Zircon (petal hello) and Linux (linux-hello) binaries.
pub fn copy_petal_to_linux_rootfs(arch: Arch) {
    let hello = build_petal(arch, "hello");
    let linux_bin = PROJECT_DIR
        .join("target")
        .join("rootfs")
        .join("linux")
        .join(arch.name())
        .join("bin");
    if linux_bin.is_dir() {
        // Copy Zircon hello (tagged with ELFOSABI_ZIRCON)
        let dest = linux_bin.join("zircon-hello");
        std::fs::copy(&hello, &dest).unwrap_or_else(|e| {
            panic!(
                "failed to copy petal hello to Linux rootfs: {} -> {}: {}",
                hello.display(),
                dest.display(),
                e
            )
        });
        println!("Copied petal hello -> {}", dest.display());

        // Build linux-hello (Rust, statically linked with musl)
        let linux_target = match arch {
            Arch::Aarch64 => "aarch64-unknown-linux-musl",
            Arch::X86_64 => "x86_64-unknown-linux-musl",
            Arch::Riscv64 => "riscv64gc-unknown-linux-musl",
        };
        let musl_cross = arch.linux_musl_cross();
        let linker = musl_cross
            .join("bin")
            .join(format!("{}-linux-musl-gcc", arch.name()));
        let linker_env = format!(
            "CARGO_TARGET_{}_LINKER",
            linux_target.to_uppercase().replace('-', "_")
        );
        let status = Command::new("cargo")
            .args(["build", "--release"])
            .arg("--manifest-path")
            .arg(PROJECT_DIR.join("tools/linux-hello/Cargo.toml"))
            .arg("--target")
            .arg(linux_target)
            .env(&linker_env, &linker)
            .status();
        match status {
            Ok(s) if s.success() => {
                let built = PROJECT_DIR
                    .join("tools/linux-hello/target")
                    .join(linux_target)
                    .join("release")
                    .join("linux-hello");
                let dest = linux_bin.join("linux-hello");
                std::fs::copy(&built, &dest)
                    .unwrap_or_else(|e| panic!("failed to copy linux-hello: {}", e));
                println!("Built linux-hello -> {}", dest.display());
            }
            _ => println!(
                "WARNING: failed to build linux-hello (target {} may not be installed)",
                linux_target
            ),
        }

        // Also copy petal shell for Zircon-init demo
        let shell = build_petal(arch, "shell");
        let dest = linux_bin.join("shell");
        std::fs::copy(&shell, &dest)
            .unwrap_or_else(|e| panic!("failed to copy petal shell: {}", e));
        println!("Copied petal shell -> {}", dest.display());

        // Build wasi-runner (Linux musl-static binary)
        let wasi_status = Command::new("cargo")
            .args(["build", "--release"])
            .arg("--manifest-path")
            .arg(PROJECT_DIR.join("tools/wasi-runner/Cargo.toml"))
            .arg("--target")
            .arg(linux_target)
            .env(&linker_env, &linker)
            .status();
        match wasi_status {
            Ok(s) if s.success() => {
                let built = PROJECT_DIR
                    .join("tools/wasi-runner/target")
                    .join(linux_target)
                    .join("release")
                    .join("wasi-runner");
                let dest = linux_bin.join("wasi-runner");
                std::fs::copy(&built, &dest)
                    .unwrap_or_else(|e| panic!("failed to copy wasi-runner: {}", e));
                println!("Built wasi-runner -> {}", dest.display());
            }
            _ => println!(
                "WARNING: failed to build wasi-runner (target {} may not be installed)",
                linux_target
            ),
        }

        // Build wasi-hello (wasm32-wasip1 target)
        let wasi_hello_status = Command::new("cargo")
            .args(["build", "--release"])
            .arg("--manifest-path")
            .arg(PROJECT_DIR.join("tools/wasi-hello/Cargo.toml"))
            .arg("--target")
            .arg("wasm32-wasip1")
            .status();
        match wasi_hello_status {
            Ok(s) if s.success() => {
                let built = PROJECT_DIR
                    .join("tools/wasi-hello/target/wasm32-wasip1/release/wasi-hello.wasm");
                let dest = linux_bin.join("hello.wasm");
                std::fs::copy(&built, &dest)
                    .unwrap_or_else(|e| panic!("failed to copy hello.wasm: {}", e));
                println!("Built hello.wasm -> {}", dest.display());
            }
            _ => println!(
                "WARNING: failed to build wasi-hello (wasm32-wasip1 target may not be installed)"
            ),
        }
    }
}

/// All petal program binary names, parsed from `petal/Cargo.toml`
/// (`[[bin]]` entries). Used to build the Zircon rootfs and to decide
/// which program the embedded petal ZBI carries.
pub fn petal_bin_names() -> Vec<String> {
    #[derive(serde_derive::Deserialize)]
    struct Bin {
        name: String,
    }
    #[derive(serde_derive::Deserialize)]
    struct Manifest {
        #[serde(default)]
        bin: Vec<Bin>,
    }
    let path = PROJECT_DIR.join("petal/Cargo.toml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    let manifest: Manifest = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", path.display(), e));
    let mut names: Vec<String> = manifest.bin.into_iter().map(|b| b.name).collect();
    names.sort();
    names
}

/// Build all petal programs and create a Zircon rootfs directory.
/// The directory layout mirrors the Linux rootfs: `bin/hello`,
/// `bin/channel-test`, `bin/vmo-test`.
///
/// `fuchsia` controls whether the prebuilt Fuchsia test binaries and
/// shared libraries are included (the `core-tests` manifest); without
/// them this stages the `petal` manifest. Returns the rootfs directory.
pub fn build_zircon_rootfs(arch: Arch, fuchsia: bool) -> PathBuf {
    let sub = if fuchsia { "core-tests" } else { "zircon" };
    let rootfs_dir = PROJECT_DIR
        .join("target")
        .join("rootfs")
        .join(sub)
        .join(arch.name());
    let bin_dir = rootfs_dir.join("bin");

    // The manifest owns its directory: clear stale files so content
    // from other manifests (or old layouts) cannot leak into the image.
    dir::clear(&rootfs_dir).unwrap();

    // The core-tests manifest is defined to include the prebuilt
    // Fuchsia test binaries: fail early (instead of at boot time) when
    // they are missing.
    let has_cts = fuchsia;
    if has_cts {
        let cts = PROJECT_DIR
            .join("prebuilt/zircon-test")
            .join(arch.name())
            .join("core-tests-standalone");
        assert!(
            cts.is_file(),
            "the core-tests manifest requires {} (see prebuilt/zircon-test/README.md)",
            cts.display()
        );
    }

    // Always run the build steps below. Cargo handles caching
    // internally (skipping compilation if sources haven't changed),
    // so building is cheap when nothing changed. The
    // previous check (exit if output files exist) skipped rebuilds
    // even when petal source code had changed, causing stale
    // binaries in the rootfs.

    std::fs::create_dir_all(&bin_dir)
        .unwrap_or_else(|e| panic!("failed to create {}: {}", bin_dir.display(), e));

    // Build every petal program (one batched cargo invocation) and copy
    // the ELFs to rootfs. The kernel loads ELF directly (proper segment
    // mapping with permissions), no objcopy stripping needed.
    let names = petal_bin_names();
    build_petal_batch(arch, &names);
    let petal_out = PROJECT_DIR
        .join("target/petal")
        .join(petal_target(arch))
        .join("release");
    for name in &names {
        let elf = petal_out.join(name);
        let dest = bin_dir.join(name);
        if copy_if_newer(&elf, &dest) {
            println!("  {} (ELF) -> {}", name, dest.display());
        }
    }

    // Copy all prebuilt Fuchsia Zircon test binaries and shared
    // libraries into rootfs. core-tests-standalone requires ld.so.1
    // and several shared libraries (libc.so, libc++.so.2, etc.).
    if has_cts {
        let prebuilt_dir = PROJECT_DIR.join("prebuilt/zircon-test").join(arch.name());
        let lib_dir = rootfs_dir.join("lib");
        std::fs::create_dir_all(&lib_dir).ok();
        for entry in std::fs::read_dir(&prebuilt_dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str == "README.md" || name_str.starts_with('.') {
                continue;
            }
            // Binaries go to bin/, libraries and ld.so.1 go to lib/
            let dest = if name_str.contains(".so") || name_str == "ld.so.1" {
                lib_dir.join(&name)
            } else {
                bin_dir.join(&name)
            };
            if copy_if_newer(&entry.path(), &dest) {
                println!("  {} (prebuilt) -> {}", name_str, dest.display());
            }
        }
    }

    println!("Zircon rootfs built at {}", rootfs_dir.display());
    rootfs_dir
}

/// Create an SFS image from the Zircon rootfs directory.
///
/// `fuchsia` selects the `core-tests` manifest (petal + prebuilt
/// Fuchsia binaries); otherwise the `petal` manifest. The image is
/// written to `image`. Returns the path to the image file.
pub fn build_zircon_rootfs_image(arch: Arch, fuchsia: bool, image: PathBuf) -> PathBuf {
    let rootfs_dir = build_zircon_rootfs(arch, fuchsia);
    std::fs::create_dir_all(image.parent().unwrap_or(&image)).ok();

    // Skip if image exists and is newer than all rootfs files.
    // Check bin/, lib/, and etc/ directories so that updated prebuilt
    // shared libraries or extra-args also trigger an image rebuild.
    if image.is_file() {
        let img_mtime = image.metadata().and_then(|m| m.modified()).ok();
        let newest_file = ["bin", "lib", "etc"]
            .iter()
            .filter_map(|sub| rootfs_dir.join(sub).read_dir().ok())
            .flatten()
            .flatten()
            .filter_map(|e| e.metadata().ok()?.modified().ok())
            .max();
        if let (Some(img_t), Some(file_t)) = (img_mtime, newest_file) {
            if img_t >= file_t {
                return image;
            }
        }
    }

    use crate::sfs_image::zip_dir;
    use std::sync::{Arc, Mutex};
    use zcore_fs::vfs::FileSystem;
    use zcore_sfs::SimpleFileSystem;

    // Size the image from the staged content instead of a fixed
    // pre-allocation: the petal manifest alone is ~35 MiB (all petal
    // test binaries) and the core-tests manifest adds the prebuilt
    // Fuchsia binaries on top. Content + 50% + 4 MiB slack, aligned to
    // the 4 KiB block size.
    let content_bytes: u64 = ["bin", "lib", "etc"]
        .iter()
        .filter_map(|sub| rootfs_dir.join(sub).read_dir().ok())
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum();
    let max_space = {
        let space = (content_bytes as usize * 3 / 2) + 4 * 1024 * 1024;
        (space + 4095) & !4095
    };
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&image)
        .expect("failed to open zircon rootfs image");
    file.set_len(max_space as u64)
        .expect("failed to pre-allocate rootfs image");
    let fs = SimpleFileSystem::create(Arc::new(Mutex::new(file)), max_space)
        .expect("failed to create sfs");
    zip_dir(&rootfs_dir, fs.root_inode()).expect("failed to zip zircon rootfs");

    println!("Zircon rootfs image: {} ", image.display());
    image
}

/// Decide which petal program the embedded ZBI carries for a product's
/// `root-proc` value.
///
/// The embedded ZBI boots on boards that ship no rootfs (raspi400,
/// jolla-c2) when ROOTPROC cannot be resolved from a filesystem, so it
/// must carry the product's init program when that program is a petal
/// binary. Root processes that are not petal programs (busybox,
/// core-tests-standalone) need a rootfs; those boards fall back to the
/// petal shell in the embedded ZBI.
pub fn zbi_bin_for(root_proc: &str) -> &str {
    let name = root_proc
        .split('?')
        .next()
        .unwrap_or("")
        .trim_start_matches("/bin/");
    if petal_bin_names().iter().any(|n| n == name) {
        name
    } else {
        "shell"
    }
}

/// Build a petal program and package it into a ZBI file.
/// Returns the path to the ZBI file.
pub fn build_petal_zbi(arch: Arch, bin_name: &str) -> PathBuf {
    let elf = build_petal(arch, bin_name);

    // Package the full ELF (not a stripped flat binary) so userstart
    // can parse PT_LOAD segments and map data/bss sections properly.
    // See #241 for why flat binaries don't work for larger programs.
    let elf_data =
        std::fs::read(&elf).unwrap_or_else(|e| panic!("Failed to read {}: {}", elf.display(), e));

    println!(
        "Packaging petal ZBI '{}' ({} bytes ELF)",
        bin_name,
        elf_data.len()
    );

    let bootfs_name = format!("bin/{}", bin_name);
    let zbi_data = zircon_abi::zbi::build_test_zbi(bootfs_name.as_bytes(), &elf_data);

    let out_dir = petal_output_dir(arch);
    std::fs::create_dir_all(&out_dir).unwrap();
    let zbi_path = out_dir.join("petal.zbi");
    std::fs::write(&zbi_path, &zbi_data)
        .unwrap_or_else(|e| panic!("Failed to write {}: {}", zbi_path.display(), e));

    println!(
        "ZBI written to {} ({} bytes)",
        zbi_path.display(),
        zbi_data.len()
    );
    zbi_path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The embedded ZBI must carry the product's init program when it
    /// is a petal binary, so rootfs-less boards boot what the product
    /// declares (e.g. zircon-hello boots hello, not the shell).
    #[test]
    fn zbi_bin_matches_product_root_proc() {
        assert_eq!(zbi_bin_for("/bin/hello"), "hello");
        assert_eq!(zbi_bin_for("/bin/shell"), "shell");
        assert_eq!(zbi_bin_for("/bin/vmo-test"), "vmo-test");
        // Non-petal root processes need a rootfs; the ZBI falls back to
        // the petal shell (the args after '?' are not part of the name).
        assert_eq!(zbi_bin_for("/bin/busybox?sh"), "shell");
        assert_eq!(
            zbi_bin_for("/bin/core-tests-standalone?--gtest_shuffle=false"),
            "shell"
        );
        assert_eq!(zbi_bin_for(""), "shell");
    }
}
