//! Build petal userspace programs and package them into a ZBI.

use crate::arch::Arch;
use crate::PROJECT_DIR;
use std::path::PathBuf;
use std::process::Command;

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
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .args(["-p", "userstart"])
        .arg("--target")
        .arg(target)
        .arg("--target-dir")
        .arg(&target_dir)
        .args(["-Z", "build-std=core,alloc"])
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

/// Build all petal programs and create a Zircon rootfs directory.
/// The directory layout mirrors the Linux rootfs: `bin/hello`,
/// `bin/channel-test`, `bin/vmo-test`.
/// Returns the path to the rootfs directory.
pub fn build_zircon_rootfs(arch: Arch) -> PathBuf {
    let rootfs_dir = PROJECT_DIR
        .join("target")
        .join("rootfs")
        .join("zircon")
        .join(arch.name());
    let bin_dir = rootfs_dir.join("bin");

    const PETAL_BINS: &[&str] = &["hello", "channel-test", "vmo-test"];

    // Check if rootfs is already populated with all expected binaries
    if PETAL_BINS.iter().all(|name| bin_dir.join(name).is_file()) {
        return rootfs_dir;
    }

    std::fs::create_dir_all(&bin_dir)
        .unwrap_or_else(|e| panic!("failed to create {}: {}", bin_dir.display(), e));

    // Build each petal program and copy the ELF to rootfs.
    // The kernel loads ELF directly (proper segment mapping with
    // permissions), no objcopy stripping needed.
    for name in PETAL_BINS {
        let elf = build_petal(arch, name);
        let dest = bin_dir.join(name);
        std::fs::copy(&elf, &dest).unwrap_or_else(|e| {
            panic!(
                "failed to copy {} to {}: {}",
                elf.display(),
                dest.display(),
                e
            )
        });
        println!("  {} (ELF) -> {}", name, dest.display());
    }

    println!("Zircon rootfs built at {}", rootfs_dir.display());
    rootfs_dir
}

/// Create an SFS image from the Zircon rootfs directory.
/// Returns the path to the image file.
pub fn build_zircon_rootfs_image(arch: Arch) -> PathBuf {
    let rootfs_dir = build_zircon_rootfs(arch);
    let dir = PROJECT_DIR
        .join("target")
        .join(format!("qemu-{}", arch.name()))
        .join("release");
    std::fs::create_dir_all(&dir).ok();
    let image = dir.join(format!("{}-zircon.img", arch.name()));

    // Skip if image exists and is newer than all rootfs binaries
    if image.is_file() {
        let img_mtime = image.metadata().and_then(|m| m.modified()).ok();
        let newest_bin = rootfs_dir.join("bin").read_dir().ok().and_then(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.metadata().ok()?.modified().ok())
                .max()
        });
        if let (Some(img_t), Some(bin_t)) = (img_mtime, newest_bin) {
            if img_t >= bin_t {
                return image;
            }
        }
    }

    use rcore_fs::vfs::FileSystem;
    use rcore_fs_fuse::zip::zip_dir;
    use rcore_fs_sfs::SimpleFileSystem;
    use std::sync::{Arc, Mutex};

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&image)
        .expect("failed to open zircon rootfs image");
    const MAX_SPACE: usize = 16 * 1024 * 1024; // 16 MiB (much smaller than Linux)
    let fs = SimpleFileSystem::create(Arc::new(Mutex::new(file)), MAX_SPACE)
        .expect("failed to create sfs");
    zip_dir(&rootfs_dir, fs.root_inode()).expect("failed to zip zircon rootfs");

    println!("Zircon rootfs image: {} ", image.display());
    image
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
