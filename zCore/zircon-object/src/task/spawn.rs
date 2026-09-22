//! Process spawning from ELF binaries.
//!
//! Provides a reusable `spawn_process` function that creates a Zircon
//! process from raw ELF data. Used by:
//! - `zircon-loader` for Zircon init and rootfs programs
//! - `linux-syscall` for cross-personality execve (Linux → Zircon)
//!
//! The `SpawnConfig` is created by `zircon-loader` (which owns the vDSO
//! and thread_fn) and can be registered globally via `set_spawn_config`
//! so that `linux-syscall` can spawn Zircon processes without depending
//! on `zircon-loader` directly.

use alloc::format;
use alloc::sync::Arc;
use spin::Once;

use super::{Job, Process, Thread};
use crate::ipc::Channel;
use crate::object::KernelObject;
use crate::object::{Handle, Rights};
use crate::vm::{MMUFlags, VmObject, VmarFlags, PAGE_SIZE};

/// Global spawn config, set once at kernel init by `zircon-loader`.
/// Allows `linux-syscall` to spawn Zircon processes without a direct
/// dependency on `zircon-loader`.
static GLOBAL_SPAWN_CONFIG: Once<SpawnConfig> = Once::new();

/// Register the global Zircon spawn configuration.
/// Called once at kernel init by `zircon-loader`.
pub fn set_spawn_config(config: SpawnConfig) {
    GLOBAL_SPAWN_CONFIG.call_once(|| config);
}

/// Spawn a Zircon process using the globally registered config.
/// Returns `None` if no config has been registered.
pub fn spawn_zircon(
    job: &Arc<Job>,
    name: &str,
    elf_data: &[u8],
) -> Option<crate::ZxResult<Arc<Process>>> {
    GLOBAL_SPAWN_CONFIG
        .get()
        .map(|config| spawn_process(job, name, elf_data, config))
}

/// Configuration for spawning a process.
pub struct SpawnConfig {
    /// vDSO VMO (code + data pages).
    pub vdso_vmo: Arc<VmObject>,
    /// Size of vDSO code region (bytes before the data page).
    pub vdso_code_size: usize,
    /// Number of stack pages to allocate.
    pub stack_pages: usize,
    /// Thread function for the new process's threads.
    pub thread_fn: crate::task::thread::ThreadFn,
}

/// Spawn a new Zircon process from raw ELF data.
///
/// Creates a process under `job`, loads the ELF, maps the vDSO,
/// allocates a stack, creates a bootstrap channel, and starts
/// execution. Returns the new process.
///
/// The process entry point receives `(startup_handle, vdso_base)`
/// per the Zircon process startup protocol.
pub fn spawn_process(
    job: &Arc<Job>,
    name: &str,
    elf_data: &[u8],
    config: &SpawnConfig,
) -> crate::ZxResult<Arc<Process>> {
    use crate::util::elf_loader::*;
    use xmas_elf::ElfFile;

    let proc = Process::create(job, name)?;
    let thread = Thread::create(&proc, &format!("{}-main", name))?;
    let vmar = proc.vmar();

    // Load ELF segments
    let elf = ElfFile::new(elf_data).map_err(|_| crate::ZxError::INVALID_ARGS)?;
    let size = elf.load_segment_size();
    let image_vmar = vmar.allocate(None, size, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)?;
    let _vmo = image_vmar.load_from_elf(&elf)?;
    let base = image_vmar.addr();
    let entry = base + elf.header.pt2.entry_point() as usize;

    // Stack
    let stack_size = config.stack_pages * PAGE_SIZE;
    let stack_vmo = VmObject::new_paged(config.stack_pages);
    stack_vmo.set_name(&format!("{}-stack", name));
    let stack_flags = MMUFlags::READ | MMUFlags::WRITE | MMUFlags::USER;
    let stack_base = vmar.map(None, stack_vmo, 0, stack_size, stack_flags)?;
    let sp = stack_base + stack_size;

    // vDSO: code pages (RX) + data page (R)
    let vdso_code_flags = MMUFlags::READ | MMUFlags::EXECUTE | MMUFlags::USER;
    let vdso_code_addr = vmar.map(
        None,
        config.vdso_vmo.clone(),
        0,
        config.vdso_code_size,
        vdso_code_flags,
    )?;
    let vdso_data_flags = MMUFlags::READ | MMUFlags::USER;
    let _vdso_data_addr = vmar.map(
        None,
        config.vdso_vmo.clone(),
        config.vdso_code_size,
        PAGE_SIZE,
        vdso_data_flags,
    )?;

    // Bootstrap channel
    let (ch0, ch1) = Channel::create();
    proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));
    let handle = Handle::new(ch1, Rights::DEFAULT_CHANNEL);

    // Start: _start(startup_handle, vdso_base)
    proc.start(
        &thread,
        entry,
        sp,
        Some(handle),
        vdso_code_addr,
        config.thread_fn,
    )?;

    Ok(proc)
}
