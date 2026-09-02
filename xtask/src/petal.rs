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

/// Build the petal hello program for the given architecture.
/// Returns the path to the compiled ELF binary.
pub fn build_petal(arch: Arch) -> PathBuf {
    let target = petal_target(arch);
    println!("Building petal for {} (target: {})", arch.name(), target);

    let status = Command::new("cargo")
        .args(["build", "--release"])
        .arg("--manifest-path")
        .arg(PROJECT_DIR.join("petal/Cargo.toml"))
        .arg("--target")
        .arg(target)
        .arg("--target-dir")
        .arg(PROJECT_DIR.join("target/petal"))
        .status()
        .expect("failed to run cargo build for petal");

    if !status.success() {
        panic!("petal build failed");
    }

    PROJECT_DIR
        .join("target/petal")
        .join(target)
        .join("release")
        .join("hello")
}

/// Strip an ELF binary to a flat binary using objcopy.
/// Returns the path to the flat binary.
fn strip_to_flat_binary(elf_path: &std::path::Path, arch: Arch) -> PathBuf {
    let out_dir = petal_output_dir(arch);
    std::fs::create_dir_all(&out_dir).unwrap();
    let flat_path = out_dir.join("hello.bin");

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

    let status = Command::new("cargo")
        .args(["build", "--release"])
        .arg("--manifest-path")
        .arg(PROJECT_DIR.join("zCore/userstart/Cargo.toml"))
        .arg("--target")
        .arg(target)
        .arg("--target-dir")
        .arg(PROJECT_DIR.join("target/userstart"))
        .status()
        .expect("failed to run cargo build for userstart");

    if !status.success() {
        panic!("userstart build failed");
    }

    PROJECT_DIR
        .join("target/userstart")
        .join(target)
        .join("release")
        .join("userstart")
}

/// Build petal and package it into a ZBI file.
/// Returns the path to the ZBI file.
pub fn build_petal_zbi(arch: Arch) -> PathBuf {
    let elf = build_petal(arch);
    let flat = strip_to_flat_binary(&elf, arch);

    let flat_data =
        std::fs::read(&flat).unwrap_or_else(|e| panic!("Failed to read {}: {}", flat.display(), e));

    println!("Packaging petal ZBI ({} bytes of code)", flat_data.len());

    let zbi_data = zircon_abi::zbi::build_test_zbi(b"bin/hello", &flat_data);

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
