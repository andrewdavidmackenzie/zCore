#![cfg_attr(not(feature = "libos"), no_std)]
#![cfg_attr(not(feature = "libos"), no_main)]
#![deny(warnings)]
#![allow(static_mut_refs)]

// Zircon is always the base flavour. Linux is additive.

use core::sync::atomic::{AtomicBool, Ordering};

extern crate alloc;
#[macro_use]
extern crate log;

#[macro_use]
mod logging;

mod fs;
mod handler;
mod utils;

/// LibOS entry point.
#[cfg(feature = "libos")]
fn main() {
    primary_core_init(hal_impl::KernelConfig {
        cmdline: env!("ZCORE_CMDLINE"),
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

    let options = utils::boot_options();
    info!("Boot options: {:#?}", options);
    hal_impl::memory::insert_regions(&hal_impl::mem::free_pmem_regions());
    hal_impl::primary_init();
    STARTED.store(true, Ordering::SeqCst);

    let proc = boot_init(options);
    utils::wait_for_exit(Some(proc))
}

/// Start the init process specified by ROOTPROC.
///
/// Auto-detects the binary flavour from the ELF header:
/// - ELFOSABI_ZIRCON (0xFC) → Zircon process (petal)
/// - Anything else → Linux process (if `linux` feature compiled in)
/// - Falls back to Zircon userboot if no rootfs is available
fn boot_init(options: utils::BootOptions) -> alloc::sync::Arc<zircon_object::task::Process> {
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
                        return zircon_loader::zircon::run_from_rootfs(rootfs, init_path);
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

    // No rootfs or binary not found — fall back to embedded ZBI
    info!("No rootfs or init binary not found, using embedded ZBI");
    zircon_loader::zircon::run_userboot(fs::zbi(), &options.cmdline)
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
    utils::wait_for_exit(None)
}
