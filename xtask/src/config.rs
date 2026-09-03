//! Machine configuration parser.
//!
//! Reads `config/machine-features.toml` to determine build settings
//! for each target machine (architecture, features, PCI support, etc.).
//!
//! Inlined from the former `z-config` crate.

use serde_derive::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

/// Parsed machine configuration.
#[derive(Debug)]
pub struct MachineConfig {
    /// Manufacturer name (e.g., "qemu", "allwinner").
    #[allow(dead_code)]
    pub manufacturer: String,
    /// Target architecture (e.g., "aarch64", "riscv64").
    pub arch: String,
    /// Path to a user image to link into the kernel (optional).
    pub user_img: Option<PathBuf>,
    /// Whether PCI is supported on this machine.
    pub pci_support: bool,
    /// Additional Cargo features to enable for this machine.
    pub features: Vec<String>,
}

impl MachineConfig {
    /// Look up a machine by name in `config/machine-features.toml`.
    pub fn select(hardware: impl AsRef<str>) -> Option<Self> {
        type ConfigFile = HashMap<String, HashMap<String, RawHardwareConfig>>;

        #[derive(Deserialize, Debug)]
        struct RawHardwareConfig {
            arch: String,
            #[serde(rename(deserialize = "link-user-img"))]
            user_img: Option<PathBuf>,
            #[serde(rename(deserialize = "pci-support"))]
            pci_support: Option<bool>,
            features: Option<Vec<String>>,
        }

        let file = Path::new(std::env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("config")
            .join("machine-features.toml");
        let file = fs::read_to_string(file).unwrap();
        let config = toml::from_str::<ConfigFile>(&file).unwrap();
        for (manufacturer, products) in config {
            for (name, raw) in products {
                if name == hardware.as_ref() {
                    return Some(Self {
                        manufacturer,
                        arch: raw.arch,
                        user_img: raw.user_img,
                        pci_support: raw.pci_support.unwrap_or(true),
                        features: raw.features.unwrap_or_default(),
                    });
                }
            }
        }
        None
    }
}
