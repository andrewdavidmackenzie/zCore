use crate::config::MachineConfig;
use crate::{linux::LinuxRootfs, Arch, ArchArg, PROJECT_DIR};
use once_cell::sync::Lazy;
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
    /// Which machine is build for.
    #[clap(long, short)]
    pub machine: String,
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
    #[clap(flatten)]
    arch: ArchArg,
    /// Build as debug mode.
    #[clap(long)]
    debug: bool,
    /// Boot in Zircon mode (instead of Linux mode).
    #[clap(long)]
    zircon: bool,
    /// Log level (error, warn, info, debug, trace). Default: warn.
    #[clap(long, default_value = "warn")]
    log: String,
    /// Number of hart (SMP for Symmetrical Multiple Processor).
    #[clap(long)]
    smp: Option<u8>,
    /// Port for gdb to connect. If set, qemu will block and wait gdb to connect.
    #[clap(long)]
    gdb: Option<u16>,
}

#[derive(Args)]
pub(crate) struct GdbArgs {
    #[clap(flatten)]
    arch: ArchArg,
    #[clap(long)]
    port: u16,
}

static INNER: Lazy<PathBuf> = Lazy::new(|| PROJECT_DIR.join("zCore"));

pub(crate) struct BuildConfig {
    arch: Arch,
    debug: bool,
    env: HashMap<OsString, OsString>,
    pub(crate) features: HashSet<String>,
}

impl BuildConfig {
    pub fn from_args(args: BuildArgs) -> Self {
        let machine = MachineConfig::select(args.machine).expect("Unknown target machine");
        let mut features = HashSet::from_iter(machine.features.iter().cloned());
        let mut env = HashMap::new();
        let arch = Arch::from_str(&machine.arch)
            .unwrap_or_else(|_| panic!("Unknown arch {} for machine", machine.arch));
        // Recursively build image
        if let Some(path) = &machine.user_img {
            features.insert("link-user-img".into());
            env.insert(
                "USER_IMG".into(),
                if path.is_absolute() {
                    path.as_os_str().to_os_string()
                } else {
                    PROJECT_DIR.join(path).as_os_str().to_os_string()
                },
            );
            LinuxRootfs::new(arch).image();
        }
        // PCI not supported
        if !machine.pci_support {
            features.insert("no-pci".into());
        }
        if !features.contains("zircon") {
            features.insert("linux".into());
        }
        Self {
            arch,
            debug: args.debug,
            env,
            features,
        }
    }

    #[inline]
    fn target_file_path(&self) -> PathBuf {
        PROJECT_DIR
            .join("target")
            .join(self.arch.name())
            .join(if self.debug { "debug" } else { "release" })
            .join("zcore")
    }

    pub fn invoke(&self, cargo: impl FnOnce() -> Cargo) {
        let mut cargo = cargo();
        cargo
            .package("zcore")
            .features(false, &self.features)
            .target(INNER.join(format!("{}.json", self.arch.name())))
            .args(["-Z", "json-target-spec"])
            .args(["-Z", "build-std=core,alloc"])
            .args(["-Z", "build-std-features=compiler-builtins-mem"])
            .conditional(!self.debug, |cargo| {
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
        println!("strip zcore to {}", out.display());
        dir::create_parent(&out).unwrap();
        BinUtil::objcopy()
            .arg("--binary-architecture=riscv64")
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
        let out = output.unwrap_or_else(|| PROJECT_DIR.join("target/zcore.asm"));
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
        let is_zircon = self.zircon;

        // Build rootfs image (Linux mode only)
        if !is_zircon {
            self.arch.linux_rootfs().image();
        }

        // Build various strings
        let arch = self.arch.arch;
        let arch_str = arch.name();
        let obj = PROJECT_DIR
            .join("target")
            .join(self.arch.arch.name())
            .join(if self.debug { "debug" } else { "release" })
            .join("zcore");

        // Build the kernel
        let mut build_config = BuildConfig::from_args(BuildArgs {
            machine: format!("virt-{}", self.arch.arch.name()),
            debug: self.debug,
        });
        // Set the kernel command line via compile-time env var
        let cmdline = if is_zircon {
            format!("LOG={}", self.log)
        } else {
            format!("LOG={}:ROOTPROC=/bin/busybox?sh", self.log)
        };
        build_config
            .env
            .insert("ZCORE_CMDLINE".into(), cmdline.into());

        if is_zircon {
            build_config.features.remove("linux");
            build_config.features.insert("zircon".into());
            // Build userstart (first userspace process)
            let userstart_path = crate::petal::build_userstart(arch);
            build_config
                .env
                .insert("USERSTART_ELF".into(), userstart_path.into_os_string());
            // Build petal ZBI (init program loaded by userstart)
            let zbi_path = crate::petal::build_petal_zbi(arch, "hello");
            build_config
                .env
                .insert("PETAL_ZBI".into(), zbi_path.into_os_string());
        }

        // For riscv64 we need a raw binary; for aarch64 we use the ELF directly
        let bin = match arch {
            Arch::Aarch64 => {
                build_config.invoke(Cargo::build);
                obj.clone()
            }
            _ => build_config.bin(None),
        };
        // Set qemu arguments
        let mut qemu = Qemu::system(arch_str);
        qemu.args(["-m", "2G"])
            .args(["-display", "none"])
            .arg("-no-reboot")
            .arg("-nographic")
            .optional(&self.smp, |qemu, smp| {
                qemu.args(["-smp", &smp.to_string()]);
            });
        match arch {
            Arch::Riscv64 => {
                qemu.args(["-machine", "virt"])
                    .arg("-kernel")
                    .arg(&bin)
                    .args(["-bios", "default"])
                    .args(["-serial", "mon:stdio"]);
                if !is_zircon {
                    // Linux mode: pass rootfs image as initrd
                    qemu.arg("-initrd")
                        .arg(INNER.join(format!("{arch_str}.img")));
                }
            }
            Arch::X86_64 => {
                // Create a bootable BIOS disk image using the x86-bootimage tool
                let disk_image = PROJECT_DIR
                    .join("target/x86_64")
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
                let status = std::process::Command::new(&bootimage_tool)
                    .arg(&obj)
                    .arg(&disk_image)
                    .status()
                    .expect("failed to run x86-bootimage");
                if !status.success() {
                    panic!("boot image creation failed");
                }

                if !is_zircon {
                    eprintln!(
                        "WARNING: x86_64 Linux rootfs is not yet supported.\n\
                         The kernel will boot but panic when trying to mount rootfs."
                    );
                }

                qemu.args(["-machine", "q35"])
                    .args(["-cpu", "qemu64,+fsgsbase"])
                    .args(["-serial", "mon:stdio"])
                    .args([
                        "-drive",
                        &format!("format=raw,file={}", disk_image.display()),
                    ]);
            }
            Arch::Aarch64 => {
                // Direct kernel boot: QEMU loads the ELF directly, no UEFI
                // bootloader needed. The kernel's _boot assembly sets up MMU
                // and page tables before jumping to rust_main.
                qemu.args(["-machine", "virt"])
                    .args(["-cpu", "cortex-a72"])
                    .arg("-kernel")
                    .arg(&obj)
                    .args(["-serial", "mon:stdio"]);
                if !is_zircon {
                    // Linux mode: pass rootfs image via block device
                    qemu.args([
                        "-drive",
                        &format!(
                            "file={}/aarch64.img,if=none,format=raw,id=x0",
                            INNER.display()
                        ),
                    ])
                    .args([
                        "-device",
                        "virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0",
                    ]);
                }
                // Zircon mode: ZBI is linked into the kernel binary
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
        match self.arch.arch {
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
