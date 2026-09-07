//! Build petal userspace programs and package them into a ZBI.

use crate::arch::Arch;
use crate::PROJECT_DIR;
use std::path::PathBuf;
use std::process::Command;

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
        .status()
        .expect("failed to run cargo build for petal");

    if !status.success() {
        panic!("petal build failed");
    }

    target_dir.join(target).join("release").join(bin_name)
}

/// Strip an ELF binary to a flat binary using objcopy.
/// Returns the path to the flat binary.
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

    target_dir.join(target).join("release").join("userstart")
}

/// Build all petal programs and create a Zircon rootfs directory.
/// The directory layout mirrors the Linux rootfs: `bin/hello`,
/// `bin/channel_test`, `bin/vmo_test`.
/// Returns the path to the rootfs directory.
pub fn build_zircon_rootfs(arch: Arch) -> PathBuf {
    let rootfs_dir = PROJECT_DIR.join("rootfs").join(arch.name()).join("zircon");
    let bin_dir = rootfs_dir.join("bin");

    // Check if rootfs is already populated
    if bin_dir.join("hello").is_file() {
        return rootfs_dir;
    }

    std::fs::create_dir_all(&bin_dir)
        .unwrap_or_else(|e| panic!("failed to create {}: {}", bin_dir.display(), e));

    // Build each petal program and copy the flat binary to rootfs
    for name in &["hello", "channel_test", "vmo_test"] {
        let elf = build_petal(arch, name);
        let flat = strip_to_flat_binary(&elf, arch, name);
        let dest = bin_dir.join(name);
        std::fs::copy(&flat, &dest).unwrap_or_else(|e| {
            panic!(
                "failed to copy {} to {}: {}",
                flat.display(),
                dest.display(),
                e
            )
        });
        println!("  {} -> {}", name, dest.display());
    }

    println!("Zircon rootfs built at {}", rootfs_dir.display());
    rootfs_dir
}

/// Create an SFS image from the Zircon rootfs directory.
/// Returns the path to the image file.
pub fn build_zircon_rootfs_image(arch: Arch) -> PathBuf {
    let rootfs_dir = build_zircon_rootfs(arch);
    let image = PROJECT_DIR
        .join("zCore")
        .join(format!("{}-zircon.img", arch.name()));

    // Skip if image is up to date
    if image.is_file() {
        // Simple check: if it exists, assume it's good (same as Linux rootfs)
        return image;
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
    let flat = strip_to_flat_binary(&elf, arch, bin_name);

    let flat_data =
        std::fs::read(&flat).unwrap_or_else(|e| panic!("Failed to read {}: {}", flat.display(), e));

    println!(
        "Packaging petal ZBI '{}' ({} bytes of code)",
        bin_name,
        flat_data.len()
    );

    let bootfs_name = format!("bin/{}", bin_name);
    let zbi_data = zircon_abi::zbi::build_test_zbi(bootfs_name.as_bytes(), &flat_data);

    let zbi_path = petal_output_dir(arch).join("petal.zbi");
    std::fs::write(&zbi_path, &zbi_data)
        .unwrap_or_else(|e| panic!("Failed to write {}: {}", zbi_path.display(), e));

    println!(
        "ZBI written to {} ({} bytes)",
        zbi_path.display(),
        zbi_data.len()
    );
    zbi_path
}
