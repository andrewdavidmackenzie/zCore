//! Helper tool to create x86_64 bootable disk images.
//!
//! Uses the `bootloader` crate to create BIOS bootable images
//! from the zCore kernel ELF.
//!
//! UEFI boot is blocked by an upstream issue:
//! https://github.com/rust-osdev/bootloader/issues/579
//! When fixed, enable the `uefi` feature in Cargo.toml and
//! add UEFI support here.
//!
//! Usage:
//!   x86-bootimage <kernel-elf> <output-image>

use anyhow::{Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <kernel-elf> <output-image>", args[0]);
        std::process::exit(1);
    }

    let kernel_path = PathBuf::from(&args[1]);
    let output_path = PathBuf::from(&args[2]);

    if !kernel_path.exists() {
        anyhow::bail!("Kernel ELF not found: {}", kernel_path.display());
    }

    println!(
        "Creating BIOS boot image from {}...",
        kernel_path.display()
    );
    bootloader::BiosBoot::new(&kernel_path)
        .create_disk_image(&output_path)
        .context("failed to create BIOS boot image")?;

    println!(
        "Boot image created: {} ({} bytes)",
        output_path.display(),
        std::fs::metadata(&output_path)?.len()
    );
    Ok(())
}
