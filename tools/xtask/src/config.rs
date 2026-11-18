//! Board and product configuration.
//!
//! A build configuration is exactly one **board** plus one **product**,
//! selected as `<product>.<board>` — mirroring Fuchsia's
//! `fx set PRODUCT.BOARD`.
//!
//! - **Board** (`configs/boards/<name>.toml`): the hardware to build for —
//!   architecture, CPU cores, drivers, linker script, QEMU configuration,
//!   boot/flash method, and the rustc target specification.
//! - **Product** (`configs/products/<name>.toml`): the software to build —
//!   OS personality (flavour), init program, rootfs contents, kernel
//!   command line, and software cargo features.
//!
//! Field ownership is strict: a board file declaring a product field (or
//! vice versa) is a configuration error, not a silent last-wins merge.
//! Products may `extends` another product (Fuchsia-style GNI layering):
//! child keys override the parent, and lists append.
//!
//! The xtask generates a rustc target-spec JSON from the board and passes
//! it to `cargo build --target <path>`. The JSON is named after the full
//! selection (`<product>.<board>`) so cargo keeps an independent build
//! directory per selection — flavour, features, and `ZCORE_CMDLINE` are
//! compile-time inputs (see `zCore/hal-impl/build.rs`).

use serde_derive::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

/// The zCore workspace root (parent of `tools/`).
pub fn workspace_root() -> &'static Path {
    Path::new(std::env!("CARGO_MANIFEST_DIR"))
        .parent() // tools/
        .unwrap()
        .parent() // workspace root
        .unwrap()
}

/// Directory containing board definitions, relative to the workspace root.
const BOARDS_DIR: &str = "configs/boards";
/// Directory containing product definitions, relative to the workspace root.
const PRODUCTS_DIR: &str = "configs/products";
/// The persistent selection file (gitignored), written by `cargo zcore set`.
const SELECTION_FILE: &str = "configs/config.toml";
/// The selection used when neither `-m` nor `configs/config.toml` provides one.
const DEFAULT_SELECTION: &str = "busybox.qemu-aarch64";

/// Board configuration from `configs/boards/<name>.toml`.
///
/// A board defines the hardware the build targets: architecture, CPU
/// cores, drivers, linker script, QEMU launch parameters, and the rustc
/// target specification.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardConfig {
    /// Target architecture: "aarch64", "x86_64", "riscv64", "host".
    pub arch: String,
    /// Number of CPU cores. Defaults to 1.
    /// Used to pass -smp to QEMU and to limit SMP core startup.
    #[serde(default = "default_cores")]
    pub cores: u8,
    /// Path to the linker script (relative to workspace root).
    #[serde(rename = "linker-script")]
    pub linker_script: String,
    /// List of driver names to include. Each maps to a cargo feature.
    #[serde(default)]
    pub drivers: Vec<String>,
    /// Board-scoped cargo features (e.g. "uefi-boot", "board-raspi400",
    /// "libos", board driver families). Software features belong to the
    /// product.
    #[serde(default)]
    pub features: Vec<String>,
    /// Features for the UEFI stub build (e.g. "uefi-console",
    /// "board-raspi400"). Only used when `uefi-boot` is in features.
    #[serde(rename = "stub-features", default)]
    pub stub_features: Vec<String>,
    /// QEMU launch configuration (absent for real-hardware-only boards).
    /// Consumed by `cargo qemu` to launch the board's machine model.
    #[serde(default)]
    pub qemu: Option<QemuConfig>,
    /// Rustc target specification fields (written to JSON at build time).
    /// Absent for libos, which uses the host's default target.
    #[serde(rename = "rustc-target", default)]
    pub rustc_target: HashMap<String, toml::Value>,
    /// How the kernel is loaded: "raw" (QEMU -kernel / SD extlinux),
    /// "uefi" (UEFI stub + ESP), "sdcard", "usb", or "host" (libos).
    /// Consumed by the `make flash`/`make image` targets.
    #[allow(dead_code)]
    #[serde(rename = "boot-method", default = "default_boot_method")]
    pub boot_method: String,
    /// How to write the image to real hardware: "sd" (SD card partition
    /// mount), "usb" (raw disk write). Absent for QEMU/libos boards.
    /// Consumed by the `make flash` target.
    #[allow(dead_code)]
    #[serde(rename = "flash-method")]
    pub flash_method: Option<String>,
    /// Product used when this board is selected by its bare name
    /// (legacy `-m <board>` syntax, deprecated — use
    /// `-m <product>.<board>`).
    #[serde(rename = "default-product", default = "default_product")]
    pub default_product: String,
}

fn default_cores() -> u8 {
    1
}

fn default_boot_method() -> String {
    "raw".to_string()
}

fn default_product() -> String {
    "busybox".to_string()
}

/// QEMU launch configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QemuConfig {
    /// QEMU machine type (e.g., "virt", "q35").
    pub machine: String,
    /// QEMU CPU model (e.g., "cortex-a72", "qemu64,+rdrand").
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

/// Product configuration from `configs/products/<name>.toml`.
///
/// A product defines the software the build produces: the OS personality,
/// the init program, rootfs contents, and kernel command-line policy.
/// Products may `extends` another product; child keys override the
/// parent and lists append. This is the *resolved* form — load via
/// [`ProductConfig::load`], which applies `extends` inheritance.
#[derive(Debug)]
pub struct ProductConfig {
    /// OS personality: "" for Zircon only, "linux" to add Linux syscall
    /// emulation. Can be overridden with `--flavour`.
    pub flavour: String,
    /// Init program path (e.g., "/bin/shell", "/bin/busybox?sh").
    /// The part after '?' becomes argv for the init process.
    pub root_proc: String,
    /// Default log level (error, warn, info, debug, trace).
    /// CLI `--log` still overrides.
    pub log: String,
    /// Which rootfs contents to package: "full" (busybox + petal +
    /// demos), "petal" (petal programs only), or "core-tests" (petal +
    /// prebuilt Fuchsia test binaries).
    pub rootfs_contents: String,
    /// Extra kernel command-line items appended after LOG/ROOTPROC
    /// (e.g., "kernel.enable-debugging-syscalls=true").
    pub extra_cmdline: Vec<String>,
    /// Software cargo features (e.g., "dynamic-linking").
    pub features: Vec<String>,
    /// Board-scoped software features, applied only when the named board
    /// is part of the selection: `{ "qemu-x86_64" = ["dynamic-linking"] }`.
    pub board_features: HashMap<String, Vec<String>>,
    /// Boards this product is known to run on. Absent means any board.
    /// This encodes Fuchsia-style constrained product×board pairings
    /// (e.g. core-tests is x86_64-only, Zircon-only products exclude
    /// riscv64 where zircon-abi cannot compile).
    pub supported_boards: Option<Vec<String>>,
}

fn default_log() -> String {
    "warn".to_string()
}

fn default_rootfs_contents() -> String {
    "full".to_string()
}

/// A resolved `<product>.<board>` build selection.
#[derive(Debug)]
pub struct Selection {
    pub product: ProductConfig,
    pub board: BoardConfig,
    /// Product name (e.g., "petal").
    pub product_name: String,
    /// Board name (e.g., "qemu-aarch64").
    pub board_name: String,
}

impl Selection {
    /// The selection name used for output directories and the generated
    /// target-spec JSON: `<product>.<board>`.
    pub fn name(&self) -> String {
        format!("{}.{}", self.product_name, self.board_name)
    }

    /// Path of the persistent selection file (gitignored).
    pub fn config_path() -> PathBuf {
        workspace_root().join(SELECTION_FILE)
    }

    /// The selection saved by `cargo zcore set`, if any.
    pub fn saved() -> Option<String> {
        let content = fs::read_to_string(Self::config_path()).ok()?;
        #[derive(serde_derive::Deserialize)]
        struct Saved {
            selection: String,
        }
        toml::from_str::<Saved>(&content).ok().map(|s| s.selection)
    }

    /// Persists this selection as the default for build commands that
    /// are not given an explicit `-m`.
    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("failed to create {}: {}", parent.display(), e));
        }
        let content = format!(
            "# Persistent zCore build selection, written by `cargo zcore set`.\n\
             # This file is gitignored; delete it to fall back to the default.\n\
             selection = \"{}\"\n",
            self.name()
        );
        fs::write(&path, content)
            .unwrap_or_else(|e| panic!("failed to write {}: {}", path.display(), e));
    }

    /// Resolves which selection to build:
    ///
    /// 1. An explicit `-m <product>.<board>` (also legacy bare board
    ///    names, with a deprecation warning).
    /// 2. The selection saved by `cargo zcore set` (`configs/config.toml`).
    /// 3. The built-in default (`busybox.qemu-aarch64`).
    pub fn resolve(spec: Option<&str>) -> Self {
        match spec {
            Some(spec) => Self::parse(spec),
            None => {
                if let Some(saved) = Self::saved() {
                    eprintln!("Using saved selection: {saved} (`cargo zcore set` to change)");
                    return Self::parse(&saved);
                }
                eprintln!(
                    "No saved selection (`cargo zcore set <product>.<board>` to set one); \
                     using default {DEFAULT_SELECTION}"
                );
                Self::parse(DEFAULT_SELECTION)
            }
        }
    }

    /// Prints a human-readable summary of the resolved selection,
    /// like Fuchsia's `fx set` output.
    pub fn print_summary(&self) {
        println!("Selection: {}", self.name());
        println!(
            "  product:   {} (configs/products/{}.toml)",
            self.product_name, self.product_name
        );
        println!(
            "  board:     {} (configs/boards/{}.toml)",
            self.board_name, self.board_name
        );
        println!("  arch:      {}", self.board.arch);
        println!("  cores:     {}", self.board.cores);
        println!("  flavour:   {:?}", self.product.flavour);
        println!("  root-proc: {}", self.product.root_proc);
        println!("  log:       {}", self.product.log);
        println!("  rootfs:    {}", self.product.rootfs_contents);
        println!("  output:    target/{}/release/", self.name());
    }

    /// Parses a `-m`/`--machine` selection specifier.
    ///
    /// - `"<product>.<board>"` — the full form.
    /// - `"<board>"` — legacy bare board name: uses the board's
    ///   `default-product` and prints a deprecation warning.
    ///
    /// Validates that both halves exist and that the board is in the
    /// product's `supported-boards` list (when set).
    pub fn parse(spec: &str) -> Self {
        let (product_name, board) = match spec.split_once('.') {
            Some((product, board)) if !product.is_empty() && !board.is_empty() => {
                let board = BoardConfig::load(board);
                (product.to_string(), board)
            }
            _ => {
                // Legacy bare board name: resolve the default product.
                let board = BoardConfig::load(spec);
                let product_name = board.default_product.clone();
                eprintln!(
                    "warning: bare board name '{spec}' is deprecated, \
                     using default product '{product_name}.{spec}' \
                     (pass -m {product_name}.{spec} to silence this warning)"
                );
                (product_name, board)
            }
        };
        let board_name = spec
            .split_once('.')
            .map_or(spec.to_string(), |(_, board)| board.to_string());

        let product = ProductConfig::load(&product_name);

        // Validate the pairing.
        if let Some(supported) = &product.supported_boards {
            if !supported.contains(&board_name) {
                panic!(
                    "product '{product_name}' does not support board '{board_name}'.\n\
                     Supported boards: {}\n\
                     Boards this product is known to run on are listed in its \
                     `supported-boards` field ({PRODUCTS_DIR}/{product_name}.toml).",
                    supported.join(", ")
                );
            }
        }

        Self {
            product,
            board,
            product_name,
            board_name,
        }
    }

    /// Whether the selection builds the Linux personality (the "linux"
    /// cargo feature) in addition to Zircon.
    pub fn is_linux(&self) -> bool {
        self.product.flavour.split(',').any(|p| p.trim() == "linux")
    }

    /// Builds the `ZCORE_CMDLINE` string for this selection.
    ///
    /// `log_override` (from `--log` or the `LOG=` environment variable)
    /// takes precedence over the product's default log level.
    pub fn build_cmdline(&self, log_override: Option<&str>) -> String {
        let log = log_override.unwrap_or(&self.product.log);
        if self.product.root_proc.is_empty() {
            panic!(
                "product '{}' must define `root-proc` \
                 ({PRODUCTS_DIR}/{}.toml)",
                self.product_name, self.product_name
            );
        }
        let mut cmdline = format!("LOG={} ROOTPROC={}", log, self.product.root_proc);
        for extra in &self.product.extra_cmdline {
            cmdline.push(' ');
            cmdline.push_str(extra);
        }
        cmdline
    }

    /// Collects all cargo features implied by this selection: board
    /// features, drivers, product features, board-scoped product
    /// features, and the flavour.
    pub fn cargo_features(&self) -> Vec<String> {
        let mut features: Vec<String> = self.board.features.clone();
        for driver in &self.board.drivers {
            let feat = driver_to_feature(driver);
            if !features.iter().any(|f| f == feat) {
                features.push(feat.to_string());
            }
        }
        for f in &self.product.features {
            if !features.contains(f) {
                features.push(f.clone());
            }
        }
        if let Some(board_feats) = self.product.board_features.get(&self.board_name) {
            for f in board_feats {
                if !features.contains(f) {
                    features.push(f.clone());
                }
            }
        }
        if self.is_linux() && !features.iter().any(|f| f == "linux") {
            features.push("linux".to_string());
        }
        features
    }

    /// Generates the rustc target-spec JSON content for the board.
    pub fn generate_target_json(&self) -> String {
        let mut spec: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

        // Always set these fields.
        spec.insert("arch".into(), self.board.arch.clone().into());
        spec.insert("executables".into(), true.into());
        spec.insert("linker".into(), "rust-lld".into());
        spec.insert("linker-flavor".into(), "ld.lld".into());
        spec.insert("panic-strategy".into(), "abort".into());
        spec.insert("target-pointer-width".into(), 64.into());

        // Linker script via pre-link-args.
        let linker_arg = format!("-T{}", self.board.linker_script);
        let pre_link: serde_json::Map<String, serde_json::Value> = [(
            "ld.lld".to_string(),
            serde_json::Value::Array(vec![linker_arg.into()]),
        )]
        .into_iter()
        .collect();
        spec.insert("pre-link-args".into(), pre_link.into());

        // Merge in all [rustc-target] fields (overrides defaults).
        for (key, value) in &self.board.rustc_target {
            spec.insert(key.clone(), toml_to_json(value));
        }

        // x86_64 uses "gnu-lld" linker-flavor instead of "ld.lld".
        if self.board.arch == "x86_64" {
            spec.insert("linker-flavor".into(), "gnu-lld".into());
        }

        serde_json::to_string_pretty(&spec).unwrap()
    }

    /// Writes the target-spec JSON for this selection and returns its path.
    ///
    /// The file lives under `target/specs/` — it is a generated build
    /// artifact (wiped by `cargo clean`), not source. It is named
    /// `<product>.<board>.json`: its *content* is board data, but the
    /// name keys cargo's output directory (`target/<product>.<board>/`)
    /// so that switching products on one board yields independent build
    /// artifacts — flavour, features, and `ZCORE_CMDLINE` are
    /// compile-time inputs.
    /// Only writes if content changed to avoid unnecessary rebuilds.
    pub fn write_target_json(&self) -> PathBuf {
        let path = workspace_root()
            .join("target")
            .join("specs")
            .join(format!("{}.json", self.name()));
        let json = self.generate_target_json();
        let existing = fs::read_to_string(&path).unwrap_or_default();
        if existing != json {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .unwrap_or_else(|e| panic!("Failed to create {}: {}", parent.display(), e));
            }
            fs::write(&path, &json)
                .unwrap_or_else(|e| panic!("Failed to write {}: {}", path.display(), e));
        }
        path
    }
}

impl BoardConfig {
    /// Loads a board definition by name from `configs/boards/`.
    pub fn load(name: &str) -> Self {
        let path = workspace_root()
            .join(BOARDS_DIR)
            .join(format!("{name}.toml"));
        if !path.exists() {
            panic!(
                "unknown board '{name}': {} not found.\n\
                 Available boards: {}\n\
                 (boards live in {BOARDS_DIR}/; use `-m <product>.<board>`)",
                path.display(),
                Self::list().join(", ")
            );
        }
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read board '{}': {}", path.display(), e));
        toml::from_str(&content).unwrap_or_else(|e| {
            panic!(
                "Failed to parse board '{}': {}\n\
                 (boards own hardware fields only; software fields like \
                 flavour/root-proc/log belong to products in {PRODUCTS_DIR}/)",
                path.display(),
                e
            )
        })
    }

    /// Lists all available board names (from `configs/boards/*.toml`),
    /// sorted, one logical name each.
    pub fn list() -> Vec<String> {
        list_names(&workspace_root().join(BOARDS_DIR))
    }
}

impl ProductConfig {
    /// Loads a product definition by name from `configs/products/`,
    /// resolving the `extends` inheritance chain.
    ///
    /// Layering follows Fuchsia's GNI `import()` semantics: child keys
    /// override the parent, and lists (`extra-cmdline`, `features`,
    /// `board-features`) append. `supported-boards` is replaced when the
    /// child sets it.
    pub fn load(name: &str) -> Self {
        let raw = RawProductConfig::load(name);
        let mut resolved = raw.clone_without_extends();
        let mut chain: Vec<String> = vec![name.to_string()];

        let mut parent_name = raw.extends;
        while let Some(p) = parent_name {
            if chain.contains(&p) {
                panic!("product inheritance cycle: {} -> {}", chain.join(" -> "), p);
            }
            chain.push(p.clone());
            let parent = RawProductConfig::load(&p);
            parent_name = parent.extends.clone();
            resolved = RawProductConfig::overlay(resolved, parent);
        }
        resolved.into_resolved()
    }

    /// Lists all available product names (from `configs/products/*.toml`),
    /// sorted.
    pub fn list() -> Vec<String> {
        list_names(&workspace_root().join(PRODUCTS_DIR))
    }
}

/// Raw (unresolved) product TOML — all fields optional so layering can
/// fill them in.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
struct RawProductConfig {
    #[serde(default)]
    extends: Option<String>,
    #[serde(default)]
    flavour: Option<String>,
    #[serde(rename = "root-proc", default)]
    root_proc: Option<String>,
    #[serde(default)]
    log: Option<String>,
    #[serde(rename = "rootfs-contents", default)]
    rootfs_contents: Option<String>,
    #[serde(rename = "extra-cmdline", default)]
    extra_cmdline: Option<Vec<String>>,
    #[serde(default)]
    features: Option<Vec<String>>,
    #[serde(rename = "board-features", default)]
    board_features: Option<HashMap<String, Vec<String>>>,
    #[serde(rename = "supported-boards", default)]
    supported_boards: Option<Vec<String>>,
}

impl RawProductConfig {
    fn load(name: &str) -> Self {
        let path = workspace_root()
            .join(PRODUCTS_DIR)
            .join(format!("{name}.toml"));
        if !path.exists() {
            panic!(
                "unknown product '{name}': {} not found.\n\
                 Available products: {}\n\
                 (products live in {PRODUCTS_DIR}/; use `-m <product>.<board>`)",
                path.display(),
                ProductConfig::list().join(", ")
            );
        }
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read product '{}': {}", path.display(), e));
        toml::from_str(&content).unwrap_or_else(|e| {
            panic!(
                "Failed to parse product '{}': {}\n\
                 (products own software fields only; hardware fields like \
                 arch/drivers/linker-script belong to boards in {BOARDS_DIR}/)",
                path.display(),
                e
            )
        })
    }

    /// The child with `extends` removed (the starting point of resolution).
    fn clone_without_extends(&self) -> Self {
        Self {
            extends: None,
            ..self.clone()
        }
    }

    /// Overlays `parent` under `child`: child keys win, lists append.
    fn overlay(child: Self, parent: Self) -> Self {
        /// Child key wins when set; otherwise the parent's is inherited.
        fn or<T>(child: Option<T>, parent: Option<T>) -> Option<T> {
            child.or(parent)
        }
        fn append(child: Option<Vec<String>>, parent: Option<Vec<String>>) -> Option<Vec<String>> {
            match (child, parent) {
                (Some(mut c), Some(p)) => {
                    c.extend(p);
                    Some(c)
                }
                (c, p) => c.or(p),
            }
        }
        fn merge_maps(
            child: Option<HashMap<String, Vec<String>>>,
            parent: Option<HashMap<String, Vec<String>>>,
        ) -> Option<HashMap<String, Vec<String>>> {
            match (child, parent) {
                (Some(mut c), Some(p)) => {
                    for (k, pv) in p {
                        match c.get_mut(&k) {
                            Some(cv) => cv.extend(pv),
                            None => {
                                c.insert(k, pv);
                            }
                        }
                    }
                    Some(c)
                }
                (c, p) => c.or(p),
            }
        }
        Self {
            extends: None,
            flavour: or(child.flavour, parent.flavour),
            root_proc: or(child.root_proc, parent.root_proc),
            log: or(child.log, parent.log),
            rootfs_contents: or(child.rootfs_contents, parent.rootfs_contents),
            extra_cmdline: append(child.extra_cmdline, parent.extra_cmdline),
            features: append(child.features, parent.features),
            board_features: merge_maps(child.board_features, parent.board_features),
            supported_boards: or(child.supported_boards, parent.supported_boards),
        }
    }

    /// Applies layer-independent defaults, producing the resolved
    /// [`ProductConfig`].
    fn into_resolved(self) -> ProductConfig {
        ProductConfig {
            flavour: self.flavour.unwrap_or_default(),
            root_proc: self.root_proc.unwrap_or_default(),
            log: self.log.unwrap_or_else(default_log),
            rootfs_contents: self.rootfs_contents.unwrap_or_else(default_rootfs_contents),
            extra_cmdline: self.extra_cmdline.unwrap_or_default(),
            features: self.features.unwrap_or_default(),
            board_features: self.board_features.unwrap_or_default(),
            supported_boards: self.supported_boards,
        }
    }
}

/// Well-known driver names and their corresponding cargo feature flags.
/// Every driver in the `drivers` list maps to a cargo feature that gates
/// its compilation in drivers.
fn driver_to_feature(driver: &str) -> &'static str {
    match driver {
        // Interrupt controllers
        "apic" => "apic",
        "gic-400" => "gic-400",
        "gic-v3" => "gic-v3",
        "riscv-plic" => "riscv-plic",
        "riscv-intc" => "riscv-intc",
        // UART serial
        "pl011-uart" => "pl011-uart",
        "uart-16550" => "uart-16550",
        // Keyboard input
        "ps2-keyboard" => "ps2-keyboard",
        // Display
        "fb-console" => "fb-console",
        // Mock (LibOS)
        "mock-uart" => "mock-uart",
        // Block / bus
        "pci" => "pci",
        "virtio-blk" => "virtio",
        _ => {
            panic!("unknown driver '{driver}' in board definition -- add it to driver_to_feature() in tools/xtask/src/config.rs");
        }
    }
}

/// Enumerates `<dir>/*.toml` file stems, sorted.
fn list_names(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Names of board-scoped cargo features. Products must not set these
    /// directly (they are hardware concerns owned by boards); a product
    /// that needs one on a specific board can use `board-features`.
    fn board_feature_names() -> HashSet<&'static str> {
        ["libos", "uefi-boot"].into_iter().collect()
    }

    /// A board file declaring a product-owned field must be rejected —
    /// field ownership is disjoint, not a silent last-wins merge.
    #[test]
    fn board_rejects_product_fields() {
        let toml = r#"
            arch = "aarch64"
            linker-script = "ld"
            flavour = "linux"
        "#;
        let err = toml::from_str::<BoardConfig>(toml).unwrap_err();
        assert!(err.to_string().contains("flavour"), "got: {err}");
    }

    /// A product file declaring a board-owned field must be rejected.
    #[test]
    fn product_rejects_board_fields() {
        let toml = r#"
            root-proc = "/bin/shell"
            arch = "aarch64"
        "#;
        let err = toml::from_str::<RawProductConfig>(toml).unwrap_err();
        assert!(err.to_string().contains("arch"), "got: {err}");
    }

    /// Products must not set board-scoped features directly.
    #[test]
    fn products_do_not_set_board_features() {
        for product in ProductConfig::list() {
            let pc = ProductConfig::load(&product);
            for f in &pc.features {
                assert!(
                    !board_feature_names().contains(f.as_str()),
                    "product '{product}' sets board-scoped feature '{f}' — \
                     move it to the board or use board-features"
                );
            }
        }
    }

    /// Every board's default product must compose with that board
    /// (loads the product, resolves extends, validates supported-boards).
    #[test]
    fn every_boards_default_product_composes() {
        for board in BoardConfig::list() {
            let bc = BoardConfig::load(&board);
            let s = Selection::parse(&format!("{}.{}", bc.default_product, board));
            let _ = s.cargo_features();
            let _ = s.build_cmdline(None);
        }
    }

    /// Every product×board pair that passes the supported-boards check
    /// must produce a valid cmdline and feature set.
    #[test]
    fn all_valid_pairs_compose() {
        let boards = BoardConfig::list();
        for product in ProductConfig::list() {
            let pc = ProductConfig::load(&product);
            let supported = pc.supported_boards.clone().unwrap_or_default();
            for board in &boards {
                if !supported.is_empty() && !supported.contains(board) {
                    continue;
                }
                let s = Selection::parse(&format!("{product}.{board}"));
                assert!(!s.build_cmdline(None).is_empty());
                assert!(!s.cargo_features().is_empty());
            }
        }
    }

    #[test]
    fn product_layering_overrides_and_appends() {
        // petal-linux extends petal: flavour overridden, root-proc inherited.
        let petal = ProductConfig::load("petal");
        assert_eq!(petal.flavour, "");
        assert_eq!(petal.root_proc, "/bin/shell");

        let petal_linux = ProductConfig::load("petal-linux");
        assert_eq!(petal_linux.flavour, "linux");
        assert_eq!(petal_linux.root_proc, "/bin/shell");
    }

    #[test]
    fn selection_parse_full_form() {
        let s = Selection::parse("busybox.qemu-aarch64");
        assert_eq!(s.product_name, "busybox");
        assert_eq!(s.board_name, "qemu-aarch64");
        assert_eq!(s.name(), "busybox.qemu-aarch64");
        assert!(s.is_linux());
        assert_eq!(s.board.arch, "aarch64");
    }

    #[test]
    fn selection_cmdline_includes_extra() {
        let s = Selection::parse("core-tests.qemu-x86_64");
        let cmdline = s.build_cmdline(None);
        assert!(cmdline.contains("ROOTPROC=/bin/core-tests-standalone"));
        assert!(cmdline.contains("kernel.enable-debugging-syscalls=true"));
    }

    #[test]
    fn unsupported_pair_is_rejected() {
        // core-tests supports only qemu-x86_64; pairing with another
        // board must panic with a clear message.
        let result = std::panic::catch_unwind(|| Selection::parse("core-tests.qemu-aarch64"));
        assert!(result.is_err(), "core-tests.qemu-aarch64 must be rejected");
    }

    #[test]
    fn zircon_only_products_exclude_riscv_boards() {
        // Zircon-only products list supported-boards; riscv64 boards
        // cannot run Zircon-only builds (zircon-abi compile_error).
        for product in ["petal", "zircon-hello"] {
            let pc = ProductConfig::load(product);
            let supported = pc
                .supported_boards
                .expect("zircon-only products list boards");
            for board in &supported {
                assert!(
                    !board.contains("riscv"),
                    "{product} must not list riscv boards"
                );
            }
        }
    }

    #[test]
    fn target_json_is_board_content_named_by_selection() {
        let s = Selection::parse("petal.qemu-aarch64");
        let json = s.generate_target_json();
        assert!(json.contains("aarch64-unknown-linux-gnu"));
        // The JSON for the same board under a different product must
        // have identical content (it is board data).
        let s2 = Selection::parse("busybox.qemu-aarch64");
        assert_eq!(json, s2.generate_target_json());
    }
}
