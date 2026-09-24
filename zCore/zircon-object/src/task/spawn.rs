//! Process spawning from ELF binaries.
//!
//! Provides a reusable `spawn_process` function that creates a Zircon
//! process from raw ELF data. Used by:
//! - `zircon-loader` for Zircon init and rootfs programs
//! - `linux-syscall` for cross-flavour execve (Linux → Zircon)
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

/// Function type for reading a file from the rootfs.
type RootfsReadFn = fn(&str) -> Option<alloc::vec::Vec<u8>>;

/// Callback to read a file from the rootfs.
/// Registered at boot by the kernel.
static ROOTFS_READ_FN: Once<RootfsReadFn> = Once::new();

/// Register the rootfs file reader.
pub fn set_rootfs_reader(f: fn(&str) -> Option<alloc::vec::Vec<u8>>) {
    ROOTFS_READ_FN.call_once(|| f);
}

/// Read a file from the rootfs.
pub fn read_rootfs_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    ROOTFS_READ_FN.get().and_then(|f| f(path))
}

/// Register the global Zircon spawn configuration.
/// Called once at kernel init by `zircon-loader`.
pub fn set_spawn_config(config: SpawnConfig) {
    GLOBAL_SPAWN_CONFIG.call_once(|| config);
}

/// Function type for spawning a Linux process from ELF data.
/// Takes (elf_data, args) and returns the process.
/// args[0] is the program path.
#[cfg(feature = "linux")]
type LinuxSpawnFn = fn(&[u8], &[&str]) -> crate::ZxResult<Arc<Process>>;

/// Global Linux spawn function, registered at boot when the linux
/// feature is enabled. Allows Zircon syscalls to spawn Linux processes.
#[cfg(feature = "linux")]
static LINUX_SPAWN_FN: Once<LinuxSpawnFn> = Once::new();

/// Register the Linux spawn function.
#[cfg(feature = "linux")]
pub fn set_linux_spawn_fn(f: LinuxSpawnFn) {
    LINUX_SPAWN_FN.call_once(|| f);
}

/// Spawn a Linux process from ELF data with a single path argument.
/// Returns None if no Linux spawn function has been registered
/// (or if the `linux` feature is not enabled).
#[cfg(feature = "linux")]
pub fn spawn_linux(elf_data: &[u8], path: &str) -> Option<crate::ZxResult<Arc<Process>>> {
    LINUX_SPAWN_FN.get().map(|f| f(elf_data, &[path]))
}

/// Spawn a Linux process — stub when linux feature is disabled.
#[cfg(not(feature = "linux"))]
pub fn spawn_linux(_elf_data: &[u8], _path: &str) -> Option<crate::ZxResult<Arc<Process>>> {
    None
}

/// Spawn a Linux process with explicit arguments.
#[cfg(feature = "linux")]
pub fn spawn_linux_with_args(
    elf_data: &[u8],
    _path: &str,
    args: &[&str],
) -> Option<crate::ZxResult<Arc<Process>>> {
    LINUX_SPAWN_FN.get().map(|f| f(elf_data, args))
}

/// Spawn a Linux process with args — stub when linux feature is disabled.
#[cfg(not(feature = "linux"))]
pub fn spawn_linux_with_args(
    _elf_data: &[u8],
    _path: &str,
    _args: &[&str],
) -> Option<crate::ZxResult<Arc<Process>>> {
    None
}

/// Spawn a process by flavour — unified dispatch.
///
/// Detects the flavour and delegates to the appropriate spawn function.
/// Returns an error if the flavour's spawn function is not registered
/// (e.g. Linux flavour when `linux` feature is not compiled in).
pub fn spawn_by_flavour(
    flavour: super::Flavour,
    job: &Arc<Job>,
    path: &str,
    elf_data: &[u8],
) -> crate::ZxResult<Arc<Process>> {
    match flavour {
        super::Flavour::Zircon => {
            spawn_zircon(job, path, elf_data).unwrap_or_else(|| Err(crate::ZxError::BAD_STATE))
        }
        super::Flavour::Linux => {
            spawn_linux(elf_data, path).unwrap_or_else(|| Err(crate::ZxError::NOT_SUPPORTED))
        }
        super::Flavour::Wasi => {
            // WASI binaries need an interpreter. Spawn the interpreter
            // as a Linux process with the .wasm path as an argument.
            spawn_wasi_via_interpreter(path)
        }
    }
}

/// Spawn a WASI binary via the `/bin/wasi-runner` interpreter.
///
/// Reads the interpreter from rootfs, spawns it as a Linux process
/// with argv = `["/bin/wasi-runner", path]`.
fn spawn_wasi_via_interpreter(wasm_path: &str) -> crate::ZxResult<Arc<Process>> {
    const INTERPRETER: &str = "/bin/wasi-runner";
    let interp_data = read_rootfs_file(INTERPRETER).ok_or(crate::ZxError::NOT_FOUND)?;
    // Spawn the interpreter with the wasm path as an argument.
    // The spawn function sets argv[0] = path, so we encode both
    // the interpreter and wasm path in the args string.
    spawn_linux_with_args(&interp_data, INTERPRETER, &[INTERPRETER, wasm_path])
        .unwrap_or_else(|| Err(crate::ZxError::NOT_SUPPORTED))
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

    info!("spawn_process: creating process '{}'", name);
    let proc = Process::create(job, name)?;
    let thread = Thread::create(&proc, &format!("{}-main", name))?;
    let vmar = proc.vmar();

    // Load ELF segments
    info!("spawn_process: loading ELF");
    let elf = ElfFile::new(elf_data).map_err(|_| crate::ZxError::INVALID_ARGS)?;
    let size = elf.load_segment_size();
    info!("spawn_process: allocating VMAR ({} bytes)", size);
    let image_vmar = vmar.allocate(None, size, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)?;
    info!("spawn_process: loading ELF into VMAR");
    let _vmo = image_vmar.load_from_elf(&elf)?;
    let base = image_vmar.addr();
    let entry = base + elf.header.pt2.entry_point() as usize;
    info!(
        "spawn_process: ELF loaded at {:#x}, entry {:#x}",
        base, entry
    );

    // Stack
    let stack_size = config.stack_pages * PAGE_SIZE;
    let stack_vmo = VmObject::new_paged(config.stack_pages);
    stack_vmo.set_name(&format!("{}-stack", name));
    let stack_flags = MMUFlags::READ | MMUFlags::WRITE | MMUFlags::USER;
    info!("spawn_process: mapping stack");
    let stack_base = vmar.map(None, stack_vmo, 0, stack_size, stack_flags)?;
    let sp = stack_base + stack_size;
    info!("spawn_process: stack at {:#x}, sp={:#x}", stack_base, sp);

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

    // Bootstrap channel — send root job and root resource handles
    // so petal programs can access kernel services.
    let (ch0, ch1) = Channel::create();

    // Create handles for the root job and a root resource.
    let root_job = job.clone();
    use crate::dev::Resource;
    let root_resource = Resource::create(
        "root",
        crate::dev::ResourceKind::ROOT,
        0,
        0,
        crate::dev::ResourceFlags::empty(),
    );
    let bootstrap_handles = alloc::vec![
        Handle::new(root_job, Rights::DEFAULT_CHANNEL),
        Handle::new(root_resource, Rights::DEFAULT_CHANNEL),
    ];
    let msg = crate::ipc::MessagePacket {
        data: alloc::vec![0u8; 4], // minimal data
        handles: bootstrap_handles,
    };
    ch0.write(msg).map_err(|_| crate::ZxError::INTERNAL)?;

    proc.add_handle(Handle::new(ch0, Rights::DEFAULT_CHANNEL));
    let handle = Handle::new(ch1, Rights::DEFAULT_CHANNEL);

    info!(
        "spawn_process: starting, entry={:#x} sp={:#x} vdso={:#x}",
        entry, sp, vdso_code_addr
    );
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
