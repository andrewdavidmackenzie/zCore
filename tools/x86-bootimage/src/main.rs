//! Helper tool to create x86_64 bootable disk images.
//!
//! Uses the `bootloader` crate to create BIOS bootable images
//! from the zCore kernel ELF, optionally embedding a ramdisk (rootfs).
//!
//! UEFI boot is blocked by an upstream issue:
//! https://github.com/rust-osdev/bootloader/issues/579
//! When fixed, enable the `uefi` feature in Cargo.toml and
//! add UEFI support here.
//!
//! Usage:
//!   x86-bootimage <kernel-elf> <output-image> [--ramdisk <rootfs-image>]

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <kernel-elf> <output-image> [--ramdisk <rootfs-image>]",
            args[0]
        );
        std::process::exit(1);
    }

    let kernel_path = PathBuf::from(&args[1]);
    let output_path = PathBuf::from(&args[2]);

    // Parse optional --ramdisk <path>
    let ramdisk_path = args
        .iter()
        .position(|a| a == "--ramdisk")
        .map(|i| {
            args.get(i + 1)
                .expect("--ramdisk requires a path argument")
        })
        .map(PathBuf::from);

    if !kernel_path.exists() {
        anyhow::bail!("Kernel ELF not found: {}", kernel_path.display());
    }
    if let Some(ref rd) = ramdisk_path {
        if !rd.exists() {
            anyhow::bail!("Ramdisk image not found: {}", rd.display());
        }
    }

    let mut boot = bootloader::BiosBoot::new(&kernel_path);
    if let Some(ref rd) = ramdisk_path {
        println!("  Ramdisk: {}", rd.display());
        boot.set_ramdisk(rd as &Path);
    }

    println!(
        "Creating BIOS boot image from {}...",
        kernel_path.display()
    );
    boot.create_disk_image(&output_path)
        .context("failed to create BIOS boot image")?;

    println!(
        "Boot image created: {} ({} bytes)",
        output_path.display(),
        std::fs::metadata(&output_path)?.len()
    );
    Ok(())
}
