//! Target configuration parser.
//!
//! Reads target definitions from `targets/<name>.toml`. Each target defines
//! the architecture, drivers, optional QEMU configuration, and the rustc
//! target specification fields.
//!
//! The xtask generates a temporary JSON file from the `[rustc-target]`
//! section and passes it to `cargo build --target <path>`.

use serde_derive::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

/// Parsed target configuration from a `targets/<name>.toml` file.
#[derive(Debug, Deserialize)]
pub struct TargetConfig {
    /// Target architecture: "aarch64", "x86_64", "riscv64".
    pub arch: String,
    /// Path to the linker script (relative to workspace root).
    #[serde(rename = "linker-script")]
    pub linker_script: String,
    /// List of driver names to include. Each maps to a cargo feature.
    #[serde(default)]
    pub drivers: Vec<String>,
    /// Additional cargo features beyond those implied by drivers.
    #[serde(default)]
    pub features: Vec<String>,
    /// QEMU configuration (absent for real-hardware-only targets).
    #[allow(dead_code)]
    pub qemu: Option<QemuConfig>,
    /// Rustc target specification fields (written to JSON at build time).
    /// Absent for libos targets that use the host's default target.
    #[serde(rename = "rustc-target", default)]
    pub rustc_target: HashMap<String, toml::Value>,
}

/// QEMU launch configuration.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct QemuConfig {
    /// QEMU machine type (e.g., "virt", "q35").
    pub machine: String,
    /// QEMU CPU model (e.g., "cortex-a72", "qemu64,+fsgsbase,+rdrand").
    pub cpu: Option<String>,
    /// Memory size (e.g., "2G").
    #[serde(default = "default_memory")]
    pub memory: String,
    /// Extra QEMU arguments.
    #[serde(default)]
    pub extra: Vec<String>,
}

fn default_memory() -> String {
    "2G".to_string()
}

/// Well-known driver names and their corresponding cargo feature flags.
/// Every driver in the `drivers` list maps to a cargo feature that gates
/// its compilation in kernel-drivers.
fn driver_to_feature(driver: &str) -> &'static str {
    match driver {
        // Interrupt controllers
        "apic" => "apic",
        "gic-400" => "gic-400",
        "riscv-plic" => "riscv-plic",
        "riscv-intc" => "riscv-intc",
        // UART serial
        "pl011-uart" => "pl011-uart",
        "uart-16550" => "uart-16550",
        // Keyboard input
        "ps2-keyboard" => "ps2-keyboard",
        // Mock (LibOS)
        "mock-uart" => "mock-uart",
        // Block / bus
        "pci" => "pci",
        "virtio-blk" => "virtio",
        _ => {
            panic!("unknown driver '{}' in target definition -- add it to driver_to_feature() in xtask/src/config.rs", driver);
        }
    }
}

impl TargetConfig {
    /// Load a target configuration by name.
    ///
    /// Looks for `targets/<name>.toml` relative to the workspace root.
    pub fn load(name: &str) -> Self {
        let workspace_root = Path::new(std::env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let path = workspace_root.join("targets").join(format!("{name}.toml"));
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read target '{}': {}", path.display(), e));
        toml::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse target '{}': {}", path.display(), e))
    }

    /// List all available target names (from `targets/*.toml`).
    #[allow(dead_code)]
    pub fn list_targets() -> Vec<String> {
        let workspace_root = Path::new(std::env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let targets_dir = workspace_root.join("targets");
        let mut names = Vec::new();
        if let Ok(entries) = fs::read_dir(&targets_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "toml") {
                    if let Some(stem) = path.file_stem() {
                        names.push(stem.to_string_lossy().to_string());
                    }
                }
            }
        }
        names.sort();
        names
    }

    /// Collect all cargo features implied by this target's drivers
    /// and explicit features list.
    pub fn cargo_features(&self) -> Vec<String> {
        let mut features: Vec<String> = self.features.clone();
        for driver in &self.drivers {
            let feat = driver_to_feature(driver);
            if !features.contains(&feat.to_string()) {
                features.push(feat.to_string());
            }
        }
        features
    }

    /// Generate the rustc target spec JSON content.
    ///
    /// Combines the `[rustc-target]` fields with common defaults
    /// and the linker script path.
    pub fn generate_target_json(&self) -> String {
        let mut spec: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

        // Always set these fields.
        spec.insert("arch".into(), self.arch.clone().into());
        spec.insert("executables".into(), true.into());
        spec.insert("linker".into(), "rust-lld".into());
        spec.insert("linker-flavor".into(), "ld.lld".into());
        spec.insert("panic-strategy".into(), "abort".into());
        spec.insert("target-pointer-width".into(), 64.into());

        // Linker script via pre-link-args.
        let linker_arg = format!("-T{}", self.linker_script);
        let pre_link: serde_json::Map<String, serde_json::Value> = [(
            "ld.lld".to_string(),
            serde_json::Value::Array(vec![linker_arg.into()]),
        )]
        .into_iter()
        .collect();
        spec.insert("pre-link-args".into(), pre_link.into());

        // Merge in all [rustc-target] fields (overrides defaults).
        for (key, value) in &self.rustc_target {
            spec.insert(key.clone(), toml_to_json(value));
        }

        // x86_64 uses "gnu-lld" linker-flavor instead of "ld.lld".
        if self.arch == "x86_64" {
            spec.insert("linker-flavor".into(), "gnu-lld".into());
        }

        serde_json::to_string_pretty(&spec).unwrap()
    }

    /// Write the target spec JSON to `targets/<name>.json` and return
    /// the path. The JSON sits alongside the TOML source so both the
    /// xtask and Makefile can reference the same `--target` path.
    /// Only writes if content changed to avoid unnecessary rebuilds.
    pub fn write_target_json(&self, name: &str) -> PathBuf {
        let workspace_root = Path::new(std::env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let targets_dir = workspace_root.join("targets");
        let path = targets_dir.join(format!("{name}.json"));
        let json = self.generate_target_json();
        let existing = fs::read_to_string(&path).unwrap_or_default();
        if existing != json {
            fs::write(&path, &json)
                .unwrap_or_else(|e| panic!("Failed to write {}: {}", path.display(), e));
        }
        path
    }
}

/// Convert a TOML value to a serde_json value.
fn toml_to_json(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::Value::Number((*i).into()),
        toml::Value::Float(f) => {
            serde_json::Value::Number(serde_json::Number::from_f64(*f).unwrap())
        }
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Array(a) => serde_json::Value::Array(a.iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => {
            let map: serde_json::Map<String, serde_json::Value> = t
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        toml::Value::Datetime(d) => serde_json::Value::String(d.to_string()),
    }
}
