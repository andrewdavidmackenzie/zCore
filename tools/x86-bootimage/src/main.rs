//! Helper tool to create x86_64 UEFI bootable disk images or PXE TFTP directories.
//!
//! Uses the `bootloader` crate to create UEFI bootable images
//! from the zCore kernel ELF, optionally embedding a ramdisk (rootfs).
//!
//! Usage:
//!   x86-bootimage <kernel-elf> <output> [--ramdisk <rootfs-image>] [--pxe]
//!
//! Without --pxe: creates a disk image at <output>
//! With --pxe: creates a TFTP directory at <output> for PXE network boot

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <kernel-elf> <output> [--ramdisk <rootfs-image>] [--pxe]",
            args[0]
        );
        std::process::exit(1);
    }

    let kernel_path = PathBuf::from(&args[1]);
    let output_path = PathBuf::from(&args[2]);
    let pxe_mode = args.iter().any(|a| a == "--pxe");

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

    let mut boot = bootloader::UefiBoot::new(&kernel_path);
    if let Some(ref rd) = ramdisk_path {
        println!("  Ramdisk: {}", rd.display());
        boot.set_ramdisk(rd as &Path);
    }

    if pxe_mode {
        println!(
            "Creating PXE TFTP directory from {}...",
            kernel_path.display()
        );
        boot.create_pxe_tftp_folder(&output_path)
            .context("failed to create PXE TFTP folder")?;
        println!("PXE TFTP directory created: {}", output_path.display());
    } else {
        println!(
            "Creating UEFI boot image from {}...",
            kernel_path.display()
        );
        boot.create_disk_image(&output_path)
            .context("failed to create UEFI boot image")?;
        println!(
            "Boot image created: {} ({} bytes)",
            output_path.display(),
            std::fs::metadata(&output_path)?.len()
        );
    }
    Ok(())
}
