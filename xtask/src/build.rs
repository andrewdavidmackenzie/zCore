use crate::config::TargetConfig;
use crate::{Arch, ArchArg, PROJECT_DIR};
use os_xtask_utils::{dir, BinUtil, Cargo, CommandExt, Ext, Qemu};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs,
    path::PathBuf,
    str::FromStr,
};

#[derive(Clone, Args)]
pub(crate) struct BuildArgs {
    /// Target name (e.g., "qemu-aarch64", "raspi400", "libos").
    /// Reads configuration from targets/<name>.toml.
    #[clap(long, short)]
    pub machine: String,
    /// Flavour: "linux" to add Linux emulation. Omit for Zircon only.
    /// If not set, uses the target's default-flavour from the TOML.
    #[clap(long)]
    pub flavour: Option<String>,
    /// Build as debug mode.
    #[clap(long)]
    pub debug: bool,
}

#[derive(Args)]
pub(crate) struct OutArgs {
    #[clap(flatten)]
    build: BuildArgs,
    /// The file to save asm.
    #[clap(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct QemuArgs {
    /// Target name (e.g., "qemu-aarch64"). Must have a [qemu] section.
    #[clap(long, short)]
    machine: String,
    /// Flavour: "linux" to add Linux emulation. Omit for Zircon only.
    #[clap(long)]
    flavour: Option<String>,
    /// Build as debug mode.
    #[clap(long)]
    debug: bool,
    /// Log level (error, warn, info, debug, trace). Default: warn.
    #[clap(long, default_value = "warn")]
    log: String,
    /// Number of hart (SMP for Symmetrical Multiple Processor).
    #[clap(long)]
    smp: Option<u8>,
    /// Port for gdb to connect. If set, qemu will block and wait gdb to connect.
    #[clap(long)]
    gdb: Option<u16>,
    /// Path to a custom rootfs image (SFS format). Overrides the
    /// default built rootfs. The kernel doesn't care what's in the
    /// image -- it just mounts it and runs ROOTPROC.
    #[clap(long)]
    rootfs_image: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct GdbArgs {
    /// Target name (e.g., "qemu-aarch64").
    #[clap(long, short)]
    machine: String,
    #[clap(long)]
    port: u16,
}

pub(crate) struct BuildConfig {
    arch: Arch,
    /// Target name (e.g., "qemu-aarch64") -- determines output directory.
    target_name: String,
    /// Whether this is a libos (host-native) target.
    is_libos: bool,
    debug: bool,
    pub(crate) env: HashMap<OsString, OsString>,
    pub(crate) features: HashSet<String>,
    /// Path to the generated rustc target spec JSON.
    target_json: PathBuf,
    /// Number of CPU cores from target config.
    cores: u8,
}

impl BuildConfig {
    pub fn from_args(args: BuildArgs) -> Self {
        let target = TargetConfig::load(&args.machine);

        // Determine flavour: CLI override > TOML default.
        // Supports comma-separated list for multiple flavours
        // (e.g. "linux" to add Linux emulation).
        let flavour = args
            .flavour
            .clone()
            .unwrap_or_else(|| target.default_flavour.clone());

        let is_libos = target.arch == "host";
        let arch = if is_libos {
            Arch::host()
        } else {
            Arch::from_str(&target.arch).unwrap_or_else(|_| {
                panic!(
                    "Unknown arch '{}' in target '{}'",
                    target.arch, args.machine
                )
            })
        };

        let mut features: HashSet<String> = target.cargo_features().into_iter().collect();
        let mut env = HashMap::new();

        // Zircon is always the base. "linux" adds Linux syscall emulation.
        if !flavour.is_empty() {
            for p in flavour.split(',').map(str::trim) {
                match p {
                    "linux" => {
                        features.insert("linux".to_string());
                    }
                    "" => {}
                    other => panic!(
                        "Unknown flavour '{}' — use 'linux' to add Linux emulation",
                        other
                    ),
                }
            }
        }

        // Pass through ZCORE_CMDLINE from the environment if set,
        // allowing `make build LOG=info` to flow through to the kernel.
        if let Ok(cmdline) = std::env::var("ZCORE_CMDLINE") {
            env.insert("ZCORE_CMDLINE".into(), cmdline.into());
        }

        // Generate the rustc target spec JSON (not needed for libos).
        let target_json = if is_libos {
            // LibOS uses the host's native target -- no JSON needed.
            PathBuf::new()
        } else {
            target.write_target_json(&args.machine)
        };

        // Zircon is always the base — build userstart and petal ZBI
        // unless already provided via environment.
        // Skip on riscv64: zircon-abi has compile_error! for 8-arg syscalls.
        if !matches!(arch, Arch::Riscv64) {
            if let Ok(val) = std::env::var("USERSTART_ELF") {
                env.insert("USERSTART_ELF".into(), val.into());
            } else {
                let userstart_path = crate::petal::build_userstart(arch);
                env.insert("USERSTART_ELF".into(), userstart_path.into_os_string());
            }
            if let Ok(val) = std::env::var("PETAL_ZBI") {
                env.insert("PETAL_ZBI".into(), val.into());
            } else {
                let zbi_path = crate::petal::build_petal_zbi(arch, "shell");
                env.insert("PETAL_ZBI".into(), zbi_path.into_os_string());
            }
        }

        // The `riscv` crate gates CSR access behind `cfg(riscv)`.
        // Custom JSON target specs don't automatically set this flag,
        // so we add it via RUSTFLAGS for riscv64 targets.
        if matches!(arch, Arch::Riscv64) && !is_libos {
            let mut flags = std::env::var("RUSTFLAGS").unwrap_or_default();
            if !flags.contains("--cfg riscv") {
                if !flags.is_empty() {
                    flags.push(' ');
                }
                flags.push_str("--cfg riscv");
            }
            env.insert("RUSTFLAGS".into(), flags.into());
        }

        Self {
            arch,
            target_name: args.machine.clone(),
            is_libos,
            debug: args.debug,
            env,
            features,
            target_json,
            cores: target.cores,
        }
    }

    #[inline]
    pub(crate) fn target_file_path(&self) -> PathBuf {
        PROJECT_DIR
            .join("target")
            .join(&self.target_name)
            .join(if self.debug { "debug" } else { "release" })
            .join("kernel")
    }

    pub fn invoke(&self, cargo: impl FnOnce() -> Cargo) {
        let mut cargo = cargo();
        cargo.package("kernel").features(false, &self.features);
        if self.is_libos {
            // LibOS builds with the host's native target -- no custom
            // target spec, no build-std.
        } else {
            cargo
                .target(&self.target_json)
                .args(["-Z", "json-target-spec"])
                .args(["-Z", "build-std=core,alloc"])
                .args(["-Z", "build-std-features=compiler-builtins-mem"]);
        }
        cargo.conditional(!self.debug, |cargo| {
            cargo.release();
        });
        for (key, val) in &self.env {
            println!("set build env: {key:?} : {val:?}");
            cargo.env(key, val);
        }
        cargo.invoke();
    }

    pub fn bin(&self, output: Option<PathBuf>) -> PathBuf {
        // Recursively build
        self.invoke(Cargo::build);
        // Determine output path
        let obj = self.target_file_path();
        let out = output.unwrap_or_else(|| obj.with_extension("bin"));
        // Generate
        println!("strip kernel to {}", out.display());
        dir::create_parent(&out).unwrap();
        let mut objcopy = BinUtil::objcopy();
        // riscv64 requires explicit --binary-architecture for objcopy
        if matches!(self.arch, Arch::Riscv64) {
            objcopy.arg("--binary-architecture=riscv64");
        }
        objcopy
            .arg(obj)
            .args(["--strip-all", "-O", "binary"])
            .arg(&out)
            .invoke();
        out
    }
}

impl OutArgs {
    /// Dumps disassembly.
    pub fn asm(self) {
        let Self { build, output } = self;
        let build = BuildConfig::from_args(build);
        // Recursively build
        build.invoke(Cargo::build);
        // Determine output path
        let obj = build.target_file_path();
        let out = output.unwrap_or_else(|| PROJECT_DIR.join("target/kernel.asm"));
        // Generate
        println!("Asm file dumps to '{}'.", out.display());
        dir::create_parent(&out).unwrap();
        fs::write(out, BinUtil::objdump().arg(obj).arg("-d").output().stdout).unwrap();
    }

    /// Generates bin file.
    #[inline]
    pub fn bin(self) -> PathBuf {
        let Self { build, output } = self;
        BuildConfig::from_args(build).bin(output)
    }
}

impl QemuArgs {
    /// Launches in qemu.
    pub fn qemu(self) {
        let target_name = self.machine.clone();

        // Build the kernel -- flavour comes from TOML default or --flavour override.
        let mut build_config = BuildConfig::from_args(BuildArgs {
            machine: target_name.clone(),
            flavour: self.flavour.clone(),
            debug: self.debug,
        });

        // Zircon is always the base. Linux is additive.
        let is_linux = build_config.features.contains("linux");
        // Always provide a rootfs — it contains both Linux and Zircon
        // binaries regardless of which flavour features are enabled.
        let needs_rootfs = true;
        let arch = build_config.arch;
        let arch_str = arch.name();

        // Determine the rootfs image path: custom or default.
        let rootfs_img = if let Some(ref custom) = self.rootfs_image {
            if !custom.is_file() {
                panic!(
                    "Custom rootfs image is not a regular file: {}",
                    custom.display()
                );
            }
            println!("Using custom rootfs image: {}", custom.display());
            custom.clone()
        } else {
            // Build rootfs with all binaries (busybox + petal programs).
            // Same rootfs regardless of flavour — ROOTPROC selects init.
            let rootfs = ArchArg { arch }.linux_rootfs();
            rootfs.image();
            rootfs.image_path()
        };

        let obj = build_config.target_file_path();
        // Set the kernel command line via compile-time env var.
        // If ZCORE_CMDLINE was passed through from the environment
        // (e.g. by the Makefile demo targets), use that instead.
        if !build_config
            .env
            .contains_key(&OsString::from("ZCORE_CMDLINE"))
        {
            let cmdline = if is_linux {
                // With Linux: default to busybox shell
                format!("LOG={} ROOTPROC=/bin/busybox?sh", self.log)
            } else {
                // Without Linux: default to petal shell
                format!("LOG={} ROOTPROC=/bin/shell", self.log)
            };
            build_config
                .env
                .insert("ZCORE_CMDLINE".into(), cmdline.into());
        }

        // Zircon userstart+ZBI are already built by BuildConfig::from_args().

        // For riscv64 we need a raw binary; for aarch64 we use the ELF directly
        // Build the kernel as a stripped raw binary. QEMU -kernel loads
        // it at the base of RAM. On aarch64, raw binary is required so
        // QEMU passes the DTB pointer in x0 (needed for -initrd).
        let bin = build_config.bin(None);
        // Set qemu arguments
        let mut qemu = Qemu::system(arch_str);
        qemu.args(["-m", "2G"])
            .args(["-display", "none"])
            .arg("-no-reboot")
            .arg("-nographic")
            .optional(&self.smp, |qemu, smp| {
                qemu.args(["-smp", &smp.to_string()]);
            });
        // Use target core count if no CLI override
        if self.smp.is_none() && build_config.cores > 1 {
            qemu.args(["-smp", &build_config.cores.to_string()]);
        }
        match arch {
            Arch::Riscv64 => {
                qemu.args(["-machine", "virt"])
                    .arg("-kernel")
                    .arg(&bin)
                    .args(["-bios", "default"])
                    .args(["-serial", "mon:stdio"]);
                if needs_rootfs || self.rootfs_image.is_some() {
                    // Pass rootfs image as initrd
                    qemu.arg("-initrd").arg(&rootfs_img);
                }
            }
            Arch::X86_64 => {
                // Create a bootable BIOS disk image using the x86-bootimage tool
                let disk_image = PROJECT_DIR
                    .join("target")
                    .join(&target_name)
                    .join(if self.debug { "debug" } else { "release" })
                    .join("boot.img");

                let bootimage_tool =
                    PROJECT_DIR.join("tools/x86-bootimage/target/release/x86-bootimage");
                if !bootimage_tool.exists() {
                    println!("Building x86-bootimage tool...");
                    let status = std::process::Command::new("cargo")
                        .args(["build", "--release"])
                        .arg("--manifest-path")
                        .arg(PROJECT_DIR.join("tools/x86-bootimage/Cargo.toml"))
                        .status()
                        .expect("failed to build x86-bootimage tool");
                    if !status.success() {
                        panic!("x86-bootimage tool build failed");
                    }
                }

                println!("Creating x86_64 boot image...");
                let mut cmd = std::process::Command::new(&bootimage_tool);
                cmd.arg(&obj).arg(&disk_image);

                // Embed the rootfs SFS image as a ramdisk in the boot image.
                // The bootloader loads it into physical memory and exposes it
                // via BootInfo.ramdisk_addr / ramdisk_len.
                if needs_rootfs || self.rootfs_image.is_some() {
                    if rootfs_img.exists() {
                        cmd.arg("--ramdisk").arg(&rootfs_img);
                    } else {
                        eprintln!(
                            "WARNING: rootfs image not found at {}.\n\
                             The kernel will boot but panic when trying to mount rootfs.\n\
                             Build it first with: cargo rootfs --arch x86_64",
                            rootfs_img.display()
                        );
                    }
                }

                let status = cmd.status().expect("failed to run x86-bootimage");
                if !status.success() {
                    panic!("boot image creation failed");
                }

                // The bootimage tool creates a BIOS disk image.
                // UEFI is blocked by upstream bootloader#579, tracked in #151.
                qemu.args(["-machine", "q35"])
                    .args(["-cpu", "qemu64,+fsgsbase,+rdrand"])
                    .args(["-serial", "mon:stdio"])
                    .args([
                        "-drive",
                        &format!("format=raw,file={}", disk_image.display()),
                    ]);
            }
            Arch::Aarch64 => {
                // Direct kernel boot with raw binary. QEMU loads it at
                // RAM base (0x40000000) and passes DTB pointer in x0.
                // The kernel's boot.s sets up page tables and MMU.
                qemu.args(["-machine", "virt"])
                    .args(["-cpu", "cortex-a72"])
                    .arg("-kernel")
                    .arg(&bin)
                    .args(["-serial", "mon:stdio"]);
                if needs_rootfs || self.rootfs_image.is_some() {
                    // Pass rootfs image via initrd. QEMU sets
                    // linux,initrd-start/end in the DTB, which the
                    // kernel reads via parse_dtb().
                    qemu.arg("-initrd").arg(&rootfs_img);
                }
                // Zircon mode without rootfs: ZBI is linked into the kernel binary
            }
        }
        qemu.optional(&self.gdb, |qemu, port| {
            qemu.args(["-S", "-gdb", &format!("tcp::{port}")]);
        })
        .invoke();
    }
}

impl GdbArgs {
    pub fn gdb(&self) {
        let target = TargetConfig::load(&self.machine);
        let arch = Arch::from_str(&target.arch).expect("unknown arch");
        match arch {
            Arch::Riscv64 => {
                Ext::new("riscv64-unknown-elf-gdb")
                    .args(["-ex", &format!("target remote localhost:{}", self.port)])
                    .invoke();
            }
            Arch::Aarch64 => {
                Ext::new("aarch64-none-linux-gnu-gdb")
                    .args(["-ex", &format!("target remote localhost:{}", self.port)])
                    .invoke();
            }
            Arch::X86_64 => {
                Ext::new("gdb")
                    .args(["-ex", &format!("target remote localhost:{}", self.port)])
                    .invoke();
            }
        }
    }
}
