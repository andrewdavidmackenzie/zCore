//! Helper tool to create x86_64 bootable disk images.
//!
//! Uses the `bootloader` crate to create BIOS and UEFI bootable images
//! from the zCore kernel ELF, optionally embedding a ramdisk (rootfs).
//!
//! Usage:
//!   x86-bootimage <kernel-elf> <output-image> [--ramdisk <rootfs-image>] [--uefi]

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <kernel-elf> <output-image> [--ramdisk <rootfs-image>] [--uefi]",
            args[0]
        );
        std::process::exit(1);
    }

    let kernel_path = PathBuf::from(&args[1]);
    let output_path = PathBuf::from(&args[2]);
    let uefi_mode = args.iter().any(|a| a == "--uefi");

    // Parse optional --ramdisk <path>
    let ramdisk_path = match args.iter().position(|a| a == "--ramdisk") {
        Some(i) => Some(PathBuf::from(
            args.get(i + 1)
                .ok_or_else(|| anyhow::anyhow!("--ramdisk requires a path argument"))?,
        )),
        None => None,
    };

    if !kernel_path.exists() {
        anyhow::bail!("Kernel ELF not found: {}", kernel_path.display());
    }
    if let Some(ref rd) = ramdisk_path {
        if !rd.exists() {
            anyhow::bail!("Ramdisk image not found: {}", rd.display());
        }
    }

    let mode = if uefi_mode { "UEFI" } else { "BIOS" };
    println!(
        "Creating {} boot image from {}...",
        mode,
        kernel_path.display()
    );

    if uefi_mode {
        let mut boot = bootloader::UefiBoot::new(&kernel_path);
        if let Some(ref rd) = ramdisk_path {
            println!("  Ramdisk: {}", rd.display());
            boot.set_ramdisk(rd as &Path);
        }
        boot.create_disk_image(&output_path)
            .context("failed to create UEFI boot image")?;
    } else {
        let mut boot = bootloader::BiosBoot::new(&kernel_path);
        if let Some(ref rd) = ramdisk_path {
            println!("  Ramdisk: {}", rd.display());
            boot.set_ramdisk(rd as &Path);
        }
        boot.create_disk_image(&output_path)
            .context("failed to create BIOS boot image")?;
    }

    println!(
        "Boot image created: {} ({} bytes)",
        output_path.display(),
        std::fs::metadata(&output_path)?.len()
    );
    Ok(())
}
