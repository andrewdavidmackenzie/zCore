#![deny(warnings)]

#[macro_use]
extern crate clap;

#[cfg(not(target_arch = "riscv64"))]
mod dump;

mod arch;
mod build;
mod commands;
mod errors;
mod linux;

use arch::{Arch, ArchArg};
use build::{GdbArgs, OutArgs, QemuArgs};
use clap::Parser;
use errors::XError;
use linux::LinuxRootfs;
use once_cell::sync::Lazy;
use std::{
    fs,
    net::Ipv4Addr,
    path::{Path, PathBuf},
};

use crate::build::{BuildArgs, BuildConfig};

/// The path of zCore project.
static PROJECT_DIR: Lazy<&'static Path> =
    Lazy::new(|| Path::new(std::env!("CARGO_MANIFEST_DIR")).parent().unwrap());
/// The path to store arch-dependent files from network.
static ARCHS: Lazy<PathBuf> =
    Lazy::new(|| PROJECT_DIR.join("ignored").join("origin").join("archs"));
/// The path to store third party repos from network.
static REPOS: Lazy<PathBuf> =
    Lazy::new(|| PROJECT_DIR.join("ignored").join("origin").join("repos"));
/// The path to cache generated files durning processes.
static TARGET: Lazy<PathBuf> = Lazy::new(|| PROJECT_DIR.join("ignored").join("target"));

/// Build or test zCore.
#[derive(Parser)]
#[clap(name = "zCore configure")]
#[clap(version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Sets git proxy.
    ///
    /// Input your proxy port through `--port`,
    /// or leave blank to unset it.
    ///
    /// Set `--global` for global configuration.
    ///
    /// ## Example
    ///
    /// ```bash
    /// cargo git-proxy --global --port 12345
    /// ```
    ///
    /// ```bash
    /// cargo git-proxy --global
    /// ```
    GitProxy(ProxyPort),

    /// Dumps build config.
    ///
    /// ## Example
    ///
    /// ```bash
    /// cargo dump
    /// ```
    #[cfg(not(target_arch = "riscv64"))]
    Dump,

    /// Download zircon binaries.
    ///
    /// ## Example
    ///
    /// ```bash
    /// cargo zircon-init
    /// ```
    ZirconInit,

    /// Updates toolchain, dependencies and submodules.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo update-all
    /// ```
    UpdateAll,

    /// Checks code without running.
    ///
    /// Try to compile the project with various different features.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo check-style
    /// ```
    CheckStyle,

    /// Dumps the kernel disassembly.
    ///
    /// The default output is `target/zcore.asm`.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo asm --arch riscv64 --output riscv64.asm
    /// ```
    Asm(OutArgs),

    /// Strips kernel binary for specific architecture.
    ///
    /// The default output is `target/{arch}/release/zcore.bin`.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo bin --arch riscv64 --output zcore.bin
    /// ```
    Bin(OutArgs),

    /// Runs zCore in qemu.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo qemu --arch riscv64 --smp 4
    /// ```
    Qemu(QemuArgs),

    /// Launches gdb and connects to a port.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo gdb --arch riscv64 --port 1234
    /// ```
    Gdb(GdbArgs),

    /// Rebuilds the linux rootfs.
    ///
    /// This command will remove the existing rootfs directory for this architecture,
    /// and rebuild the minimum rootfs.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo rootfs --arch riscv64
    /// ```
    Rootfs(ArchArg),

    /// Copies musl so files to rootfs directory.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo musl-libs --arch riscv64
    /// ```
    MuslLibs(ArchArg),

    /// Copies ffmpeg so files to rootfs directory.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo ffmpeg --arch riscv64
    /// ```
    Ffmpeg(ArchArg),

    /// Copies opencv so files to rootfs directory.
    ///
    /// If ffmpeg is already there, this opencv will build with ffmpeg support.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo opencv --arch riscv64
    /// ```
    Opencv(ArchArg),

    /// Copies libc test files to rootfs directory.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo libc-test --arch riscv64
    /// ```
    LibcTest(ArchArg),

    /// Copies other test files to rootfs directory.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo other-test --arch riscv64
    /// ```
    OtherTest(ArchArg),

    /// Builds the linux rootfs image file.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo image --arch riscv64
    /// ```
    Image(ArchArg),

    /// Builds the libos rootfs and puts it into libc test.
    ///
    /// > **NOTICE** This may not be the final form of this command, so this command has no alias.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo xtask libos-libc-test
    /// ```
    LibosLibcTest,

    /// Runs zCore in linux libos mode and runs the executable at the specified path.
    ///
    /// > **NOTICE** zCore can only run a single executable in libos mode, and it will exit after finishing.
    ///
    /// # Example
    ///
    /// ```bash
    /// cargo linux-libos --args /bin/busybox
    /// ```
    LinuxLibos(LinuxLibosArg),
}

#[derive(Args)]
struct ProxyPort {
    /// Proxy port.
    #[clap(long)]
    port: Option<u16>,
    /// Global config.
    #[clap(short, long)]
    global: bool,
}

#[derive(Args)]
struct LinuxLibosArg {
    /// Command for busybox.
    #[clap(short, long)]
    pub args: String,
}

fn main() {
    use Commands::*;
    match Cli::parse().command {
        GitProxy(ProxyPort { port, global }) => {
            if let Some(port) = port {
                set_git_proxy(global, port);
            } else {
                unset_git_proxy(global);
            }
        }
        #[cfg(not(target_arch = "riscv64"))]
        Dump => dump::dump_config(),
        ZirconInit => install_zircon_prebuilt(),
        UpdateAll => update_all(),
        CheckStyle => check_style(),

        Rootfs(arg) => arg.linux_rootfs().make(true),
        MuslLibs(arg) => {
            // Discard return value
            arg.linux_rootfs().put_musl_libs();
        }
        Opencv(arg) => arg.linux_rootfs().put_opencv(),
        Ffmpeg(arg) => arg.linux_rootfs().put_ffmpeg(),
        LibcTest(arg) => arg.linux_rootfs().put_libc_test(),
        OtherTest(arg) => arg.linux_rootfs().put_other_test(),
        Image(arg) => arg.linux_rootfs().image(),

        Asm(args) => args.asm(),
        Bin(args) => {
            // Discard return value
            args.bin();
        }
        Qemu(args) => args.qemu(),
        Gdb(args) => args.gdb(),

        LibosLibcTest => {
            libos::rootfs(true);
            libos::put_libc_test();
        }
        LinuxLibos(arg) => libos::linux_run(arg.args),
    }
}

/// Updates submodules.
fn git_submodule_update(init: bool) {
    use os_xtask_utils::{CommandExt, Git};
    Git::submodule_update(init).invoke();
}

/// Downloads test cases and libraries required for zircon mode.
fn install_zircon_prebuilt() {
    use commands::wget;
    use os_xtask_utils::{dir, CommandExt, Tar};
    const URL: &str =
        "https://github.com/rcore-os/zCore/releases/download/prebuilt-2208/prebuilt.tar.xz";
    let tar = Arch::X86_64.origin().join("prebuilt.tar.xz");
    wget(URL, &tar);
    // Extract to target path
    let dir = PROJECT_DIR.join("prebuilt");
    let target = TARGET.join("zircon");
    dir::rm(&dir).unwrap();
    dir::rm(&target).unwrap();
    fs::create_dir_all(&target).unwrap();
    Tar::xf(&tar, Some(&target)).invoke();
    dircpy::copy_dir(target.join("prebuilt"), dir).unwrap();
}

/// Updates toolchain and dependencies.
fn update_all() {
    use os_xtask_utils::{Cargo, CommandExt, Ext};
    git_submodule_update(false);
    Ext::new("rustup").arg("update").invoke();
    Cargo::update().invoke();
}

/// Sets git proxy.
fn set_git_proxy(global: bool, port: u16) {
    use os_xtask_utils::{CommandExt, Git};
    let dns = fs::read_to_string("/etc/resolv.conf")
        .unwrap()
        .lines()
        .find_map(|line| {
            line.strip_prefix("nameserver ")
                .and_then(|s| s.parse::<Ipv4Addr>().ok())
        })
        .expect("FAILED: detect DNS");
    let proxy = format!("socks5://{dns}:{port}");
    Git::config(global).args(["http.proxy", &proxy]).invoke();
    Git::config(global).args(["https.proxy", &proxy]).invoke();
    println!("git proxy = {proxy}");
}

/// Unsets git proxy.
fn unset_git_proxy(global: bool) {
    use os_xtask_utils::{CommandExt, Git};
    Git::config(global).args(["--unset", "http.proxy"]).invoke();
    Git::config(global)
        .args(["--unset", "https.proxy"])
        .invoke();
    println!("git proxy =");
}

/// Checks code style.
fn check_style() {
    use os_xtask_utils::{Cargo, CommandExt};
    println!("Check workspace");
    Cargo::fmt().arg("--all").arg("--").arg("--check").invoke();
    Cargo::clippy().all_features().invoke();
    Cargo::doc().all_features().arg("--no-deps").invoke();

    println!("Check libos");
    // println!("    Checks zircon libos");
    // Cargo::clippy()
    //     .package("zcore")
    //     .features(false, &["zircon", "libos"])
    //     .invoke();
    println!("    Checks linux libos");
    Cargo::clippy()
        .package("zcore")
        .features(false, ["linux", "libos"])
        .invoke();

    println!("Check bare-metal");
    for arch in [Arch::Riscv64, Arch::X86_64, Arch::Aarch64] {
        println!("    Checks {} bare-metal", arch.name());
        BuildConfig::from_args(BuildArgs {
            machine: format!("virt-{}", arch.name()),
            debug: false,
        })
        .invoke(Cargo::clippy);
    }
}

mod libos {
    use crate::{arch::Arch, commands::wget, linux::LinuxRootfs, ARCHS, TARGET};
    use os_xtask_utils::{dir, Cargo, CommandExt, Tar};
    use std::fs;

    /// Deploys the rootfs used by libos.
    pub(super) fn rootfs(clear: bool) {
        // Download
        const URL: &str =
            "https://github.com/YdrMaster/zCore/releases/download/musl-cache/rootfs-libos.tar.gz";
        let origin = ARCHS.join("libos").join("rootfs-libos.tar.gz");
        dir::create_parent(&origin).unwrap();
        wget(URL, &origin);
        // Extract
        let target = TARGET.join("libos");
        fs::create_dir_all(&target).unwrap();
        Tar::xf(origin.as_os_str(), Some(&target)).invoke();
        // Copy
        const ROOTFS: &str = "rootfs/libos";
        if clear {
            dir::clear(ROOTFS).unwrap();
        }
        dircpy::copy_dir(target.join("rootfs"), ROOTFS).unwrap();
    }

    /// Copies the x86_64 libc-test into libos.
    pub(super) fn put_libc_test() {
        const TARGET: &str = "rootfs/libos/libc-test";
        let x86_64 = LinuxRootfs::new(Arch::X86_64);
        x86_64.put_libc_test();
        dir::clear(TARGET).unwrap();
        dircpy::copy_dir(x86_64.path().join("libc-test"), TARGET).unwrap();
    }

    /// Runs an application in libos mode.
    pub(super) fn linux_run(args: String) {
        println!("{}", std::env!("OUT_DIR"));
        rootfs(false);
        // Launch!
        Cargo::run()
            .package("zcore")
            .release()
            .features(true, ["linux", "libos"])
            .arg("--")
            .args(args.split_whitespace())
            .invoke()
    }
}
