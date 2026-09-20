#![cfg_attr(not(feature = "libos"), no_std)]
#![cfg_attr(not(feature = "libos"), no_main)]
#![deny(warnings)]
#![allow(static_mut_refs)]

// Exactly one personality must be selected.
#[cfg(all(feature = "linux", feature = "zircon"))]
compile_error!("Features `linux` and `zircon` cannot be enabled at the same time");
#[cfg(not(any(feature = "linux", feature = "zircon")))]
compile_error!("One of the features `linux` or `zircon` must be enabled");

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

    let proc = boot_personality(options);
    utils::wait_for_exit(Some(proc))
}

/// Start the Linux personality. Returns the init process.
#[cfg(feature = "linux")]
fn boot_personality(options: utils::BootOptions) -> alloc::sync::Arc<zircon_object::task::Process> {
    let args = options.root_proc.split('?').map(Into::into).collect();
    let envs = alloc::vec!["PATH=/usr/sbin:/usr/bin:/sbin:/bin".into()];
    linux_loader::linux::run(args, envs, fs::rootfs())
}

/// Start the Zircon personality. Returns the init process.
#[cfg(feature = "zircon")]
fn boot_personality(options: utils::BootOptions) -> alloc::sync::Arc<zircon_object::task::Process> {
    if let Some(rootfs) = fs::try_zircon_rootfs() {
        let init_path = options.root_proc.split('?').next().unwrap_or("/bin/hello");
        zircon_loader::zircon::run_from_rootfs(rootfs, init_path)
    } else {
        zircon_loader::zircon::run_userboot(fs::zbi(), &options.cmdline)
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
    utils::wait_for_exit(None)
}
