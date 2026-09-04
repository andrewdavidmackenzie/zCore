//! Helper tool to create x86_64 bootable disk images.
//!
//! Uses the `bootloader` crate to create UEFI bootable images
//! from the zCore kernel ELF. This is the same boot path used
//! on real hardware.
//!
//! Usage:
//!   x86-bootimage <kernel-elf> <output-image> [--bios]
//!
//! Default is UEFI (matching real hardware). Use --bios for
//! legacy BIOS boot if UEFI build is not available.

use anyhow::{Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <kernel-elf> <output-image> [--bios]", args[0]);
        std::process::exit(1);
    }

    let kernel_path = PathBuf::from(&args[1]);
    let output_path = PathBuf::from(&args[2]);
    let use_bios = args.get(3).is_some_and(|a| a == "--bios");

    if !kernel_path.exists() {
        anyhow::bail!("Kernel ELF not found: {}", kernel_path.display());
    }

    if use_bios {
        println!(
            "Creating BIOS boot image from {}...",
            kernel_path.display()
        );
        bootloader::BiosBoot::new(&kernel_path)
            .create_disk_image(&output_path)
            .context("failed to create BIOS boot image")?;
    } else {
        println!(
            "Creating UEFI boot image from {}...",
            kernel_path.display()
        );
        bootloader::UefiBoot::new(&kernel_path)
            .create_disk_image(&output_path)
            .context("failed to create UEFI boot image")?;
    }

    println!(
        "Boot image created: {} ({} bytes)",
        output_path.display(),
        std::fs::metadata(&output_path)?.len()
    );
    Ok(())
}
