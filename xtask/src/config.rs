//! Machine configuration parser.
//!
//! Reads machine definitions from `[workspace.metadata.machines]` in the
//! root `Cargo.toml`. Each machine has an architecture, optional linked
//! user image, PCI support flag, and additional Cargo features.

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
    /// Look up a machine by name in `[workspace.metadata.machines]`.
    pub fn select(hardware: impl AsRef<str>) -> Option<Self> {
        #[derive(Deserialize)]
        struct CargoToml {
            workspace: WorkspaceSection,
        }

        #[derive(Deserialize)]
        struct WorkspaceSection {
            metadata: Option<MetadataSection>,
        }

        #[derive(Deserialize)]
        struct MetadataSection {
            machines: Option<HashMap<String, HashMap<String, RawHardwareConfig>>>,
        }

        #[derive(Deserialize, Debug)]
        struct RawHardwareConfig {
            arch: String,
            #[serde(rename(deserialize = "link-user-img"))]
            user_img: Option<PathBuf>,
            #[serde(rename(deserialize = "pci-support"))]
            pci_support: Option<bool>,
            features: Option<Vec<String>>,
        }

        let cargo_toml_path = Path::new(std::env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Cargo.toml");
        let content = fs::read_to_string(&cargo_toml_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", cargo_toml_path.display(), e));
        let parsed: CargoToml = toml::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {}", cargo_toml_path.display(), e));

        let machines = parsed.workspace.metadata?.machines?;

        for (manufacturer, products) in machines {
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
