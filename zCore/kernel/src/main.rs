#![cfg_attr(not(feature = "libos"), no_std)]
#![cfg_attr(not(feature = "libos"), no_main)]
#![deny(warnings)]
#![allow(static_mut_refs)]

// Zircon is always the base flavour. Linux is additive.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};
use zircon_object::{object::KernelObject, task::Process};

extern crate alloc;
#[macro_use]
extern crate log;

mod logging;

mod fs;
mod handler;
#[cfg(all(feature = "libos", feature = "linux"))]
mod hostfs;

/// LibOS entry point.
#[cfg(feature = "libos")]
fn main() {
    primary_core_init(hal_impl::KernelConfig {
        cmdline: option_env!("ZCORE_CMDLINE").unwrap_or("LOG=warn"),
        ..Default::default()
    });
}

static STARTED: AtomicBool = AtomicBool::new(false);

/// Primary boot entry point, called by platform-specific entry code in hal-impl.
#[no_mangle]
pub extern "Rust" fn primary_core_init(config: hal_impl::KernelConfig) {
    logging::init(logging::parse_log_level(config.cmdline));
    hal_impl::memory::init();
    hal_impl::primary_init_early(config, &handler::ZcoreKernelHandler);

    let options = boot_options();
    info!("Boot options: {:#?}", options);
    hal_impl::memory::insert_regions(&hal_impl::mem::free_pmem_regions());
    hal_impl::primary_init();

    let proc = boot_init(options);
    // Release secondary cores AFTER the init process is created.
    // If released earlier, idle APs enter the executor loop and
    // steal the BSP's init task via work-stealing before the BSP
    // can run it.
    STARTED.store(true, Ordering::SeqCst);
    wait_for_exit(Some(proc))
}

/// Start the init process specified by ROOTPROC.
///
/// Auto-detects the binary flavour from the ELF header:
/// - ELFOSABI_ZIRCON (0xFC) → Zircon process (petal)
/// - Anything else → Linux process (if `linux` feature compiled in)
/// - Falls back to Zircon userboot if no rootfs is available
fn boot_init(options: BootOptions) -> alloc::sync::Arc<zircon_object::task::Process> {
    // Register the Zircon spawn config globally so cross-flavour
    // exec can spawn Zircon processes from Linux context.
    zircon_object::task::spawn::set_spawn_config(zircon_loader::zircon::zircon_spawn_config());

    // Register rootfs reader so zircon-syscall can read files for exec.
    zircon_object::task::spawn::set_rootfs_reader(fs::read_rootfs_file);

    // Register Linux spawn function so Zircon processes can cross-spawn
    // Linux binaries (e.g. petal shell running /bin/linux-hello).
    #[cfg(feature = "linux")]
    if let Some(rootfs) = fs::try_rootfs() {
        linux_loader::linux::init_spawn(rootfs);
    }

    let init_path = options.root_proc.split('?').next().unwrap_or("/bin/hello");
    info!("Init process: {}", options.root_proc);

    // Try to read the init binary from rootfs to detect its flavour.
    if let Some(rootfs) = fs::try_rootfs() {
        if let Ok(inode) = rootfs.root_inode().lookup(init_path) {
            if inode.metadata().is_ok() {
                let mut header = [0u8; 8];
                let n = inode.read_at(0, &mut header).unwrap_or(0);
                if n < 8 {
                    panic!(
                        "Init binary '{}' too small ({} bytes) — not a valid ELF",
                        init_path, n
                    );
                }
                let flavour = zircon_object::task::Flavour::from_elf(&header);
                info!("Detected init flavour: {:?}", flavour);

                match flavour {
                    zircon_object::task::Flavour::Zircon => {
                        let extra_args: alloc::vec::Vec<alloc::string::String> = options
                            .root_proc
                            .split('?')
                            .skip(1)
                            .map(Into::into)
                            .collect();
                        return zircon_loader::zircon::run_from_rootfs(
                            rootfs, init_path, extra_args,
                        );
                    }
                    #[cfg(feature = "linux")]
                    zircon_object::task::Flavour::Linux => {
                        let args = options.root_proc.split('?').map(Into::into).collect();
                        let envs = alloc::vec!["PATH=/usr/sbin:/usr/bin:/sbin:/bin".into()];
                        return linux_loader::linux::run(args, envs, rootfs);
                    }
                    #[cfg(not(feature = "linux"))]
                    zircon_object::task::Flavour::Linux => {
                        panic!(
                            "Init binary '{}' is a Linux ELF but the linux feature is not compiled in",
                            init_path
                        );
                    }
                    zircon_object::task::Flavour::Wasi => {
                        panic!(
                            "Init binary '{}' is a WASM file — cannot use as init process",
                            init_path
                        );
                    }
                }
            }
        }
    }

    // No rootfs or binary not found — fall back to embedded ZBI.
    info!("No rootfs or init binary not found, using embedded ZBI");
    let zbi = load_zbi();
    zircon_loader::zircon::run_userboot(zbi, &options.cmdline)
}

/// Load the petal ZBI (Zircon Boot Image) for userboot fallback.
///
/// In libOS mode, reads the ZBI file path from the first command-line argument.
/// On bare-metal, returns the ZBI embedded at compile time by `zircon-loader`.
fn load_zbi() -> impl AsRef<[u8]> {
    #[cfg(feature = "libos")]
    {
        let path = std::env::args().nth(1).expect(
            "Usage: zcore-libos <ZBI_FILE>\n\
             Build a petal ZBI with: cargo petal-zbi --arch aarch64",
        );
        std::fs::read(path).expect("failed to read ZBI file")
    }

    #[cfg(not(feature = "libos"))]
    {
        zircon_loader::zircon::embedded_zbi()
    }
}

/// Secondary core/hart initialization (SMP).
///
/// Called by platform-specific entry code in hal-impl when a secondary
/// CPU core starts.
#[no_mangle]
pub extern "Rust" fn secondary_core_init() -> ! {
    while !STARTED.load(Ordering::SeqCst) {
        core::hint::spin_loop();
    }
    hal_impl::secondary_init();
    info!("secondary core {} initialized", hal_impl::cpu::cpu_id());
    wait_for_exit(None)
}

/// Wait for the init process to exit, then reset the system.
///
/// On the primary core, waits for the init process to signal termination
/// (either `PROCESS_TERMINATED` or `USER_SIGNAL_0`), logs the exit code,
/// and resets. On secondary cores, enters the executor idle loop to
/// service tasks spawned by the primary core (the future never completes).
fn wait_for_exit(proc: Option<Arc<Process>>) -> ! {
    let exit_code = if let Some(proc) = proc {
        let future = async move {
            use zircon_object::object::Signal;
            let object: Arc<dyn KernelObject> = proc.clone();
            // Wait for either termination signal — Linux processes use
            // PROCESS_TERMINATED, Zircon processes use USER_SIGNAL_0.
            // In dual-flavour mode, wait for either.
            let signal = Signal::PROCESS_TERMINATED | Signal::USER_SIGNAL_0;
            object.wait_signal(signal).await;
            check_exit_code(proc)
        };
        hal_impl::run_executor(future)
    } else {
        // Secondary core: enter the executor idle loop to service
        // tasks spawned by the primary core. The future never
        // completes — secondary cores run until the system shuts down.
        let future = core::future::pending::<i32>();
        hal_impl::run_executor(future)
    };
    info!("exiting with code {}", exit_code);
    hal_impl::cpu::reset()
}

fn check_exit_code(proc: Arc<Process>) -> i32 {
    let code = proc.exit_code().unwrap_or(-1);
    if code != 0 {
        error!(
            "process {:?}({}) exited with code {:?}",
            proc.name(),
            proc.id(),
            code
        );
    } else {
        info!(
            "process {:?}({}) exited with code 0",
            proc.name(),
            proc.id()
        )
    }
    code as i32
}

// ── Boot options ─────────────────────────────────────────────────────

#[derive(Debug)]
struct BootOptions {
    cmdline: alloc::string::String,
    /// Root process path (e.g. "/bin/busybox?sh" or "/bin/hello").
    root_proc: alloc::string::String,
}

/// Parse boot options from the kernel command line.
///
/// Extracts `ROOTPROC=<path>` from the cmdline provided by hal-impl.
/// Defaults to `/bin/busybox?sh` in Linux mode or `/bin/hello` otherwise.
fn boot_options() -> BootOptions {
    use alloc::string::ToString;
    let cmdline = hal_impl::boot::cmdline();
    let root_proc = parse_cmdline_value(&cmdline, "ROOTPROC")
        .unwrap_or(if cfg!(feature = "linux") {
            "/bin/busybox?sh"
        } else {
            "/bin/hello"
        })
        .to_string();
    BootOptions { cmdline, root_proc }
}

/// Extract a value from a "KEY=VALUE KEY2=VALUE2" cmdline string.
fn parse_cmdline_value<'a>(cmdline: &'a str, key: &str) -> Option<&'a str> {
    for token in cmdline.split_whitespace() {
        let mut iter = token.splitn(2, '=');
        if let (Some(k), Some(v)) = (iter.next(), iter.next()) {
            if k.trim() == key {
                return Some(v.trim());
            }
        }
    }
    None
}
