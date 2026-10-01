//! Process spawning from ELF binaries.
//!
//! Provides a reusable `spawn_process` function that creates a Zircon
//! process from raw ELF data. Used by:
//! - `zircon-loader` for Zircon init and rootfs programs
//! - `linux-syscall` for cross-flavour execve (Linux -> Zircon)
//!
//! The `SpawnConfig` is created by `zircon-loader` (which owns the vDSO
//! and thread_fn) and can be registered globally via `set_spawn_config`
//! so that `linux-syscall` can spawn Zircon processes without depending
//! on `zircon-loader` directly.
//!
//! ## Module structure
//!
//! - `mod.rs` (this file): Core process spawning, config registration,
//!   cross-flavour dispatch.
//! - `loader_service.rs`: Fuchsia `fuchsia.ldsvc/Loader` FIDL service
//!   for dynamically-linked binaries. Only compiled with the
//!   `dynamic-linking` feature.
//! - `vdso_plt.rs`: Eager vDSO PLT resolution for `ld.so.1`. Only
//!   compiled with the `dynamic-linking` feature.

#[cfg(feature = "dynamic-linking")]
mod loader_service;
#[cfg(feature = "dynamic-linking")]
mod vdso_plt;

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

/// Spawn a Linux process -- stub when linux feature is disabled.
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

/// Spawn a Linux process with args -- stub when linux feature is disabled.
#[cfg(not(feature = "linux"))]
pub fn spawn_linux_with_args(
    _elf_data: &[u8],
    _path: &str,
    _args: &[&str],
) -> Option<crate::ZxResult<Arc<Process>>> {
    None
}

/// Spawn a process by flavour -- unified dispatch.
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
    /// Extra argv entries (appended after argv[0] = program name).
    pub extra_args: alloc::vec::Vec<alloc::string::String>,
}

/// Spawn a new Zircon process from raw ELF data.
///
/// Creates a process under `job`, loads the ELF, maps the vDSO,
/// allocates a stack, creates a bootstrap channel, and starts
/// execution. Returns the new process.
///
/// If the ELF has a `PT_INTERP` header (dynamically linked) and the
/// `dynamic-linking` feature is enabled, the interpreter (e.g.,
/// `ld.so.1`) is loaded from the rootfs instead. The original program
/// is passed as a VMO handle on the bootstrap channel so the dynamic
/// linker can load it.
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

    // Check for PT_INTERP (dynamic linking).
    let elf_check = ElfFile::new(elf_data).map_err(|_| crate::ZxError::INVALID_ARGS)?;

    // Detect and load the dynamic linker (PT_INTERP), if present.
    #[cfg(feature = "dynamic-linking")]
    let interp_data = {
        let interp_path = elf_check
            .get_interpreter()
            .ok()
            .map(alloc::string::String::from);
        if let Some(ref path) = interp_path {
            info!(
                "spawn_process: ELF has PT_INTERP '{}', loading interpreter",
                path
            );
            let data = read_rootfs_file(path)
                .or_else(|| read_rootfs_file(&format!("/lib/{}", path)))
                .ok_or_else(|| {
                    warn!("spawn_process: interpreter '{}' not found in rootfs", path);
                    crate::ZxError::NOT_FOUND
                })?;
            Some(data)
        } else {
            None
        }
    };
    #[cfg(not(feature = "dynamic-linking"))]
    let interp_data: Option<alloc::vec::Vec<u8>> = {
        // Warn if the binary requires dynamic linking but the feature is disabled.
        if elf_check.get_interpreter().is_ok() {
            warn!(
                "spawn_process: ELF has PT_INTERP but dynamic-linking feature is disabled; \
                 loading as static binary"
            );
        }
        None
    };

    let has_interp = interp_data.is_some();
    let load_data = interp_data.as_deref().unwrap_or(elf_data);
    let elf = ElfFile::new(load_data).map_err(|_| crate::ZxError::INVALID_ARGS)?;

    let proc = Process::create(job, name)?;
    let thread = Thread::create(&proc, &format!("{}-main", name))?;
    let vmar = proc.vmar();

    // Load the ELF segments (interpreter if PT_INTERP, main program otherwise).
    // Avoid allocating at vaddr 0: PIE binaries like ld.so.1 have PT_LOAD
    // starting at vaddr 0, and loading at base 0 causes fsbase=0 to alias
    // the ELF header, corrupting TLS accesses via %fs:0.
    let size = elf.load_segment_size();
    let min_offset = if elf.header.pt2.entry_point() < size as u64 {
        Some(0x10_0000) // 1 MiB minimum offset for PIE/shared objects
    } else {
        None // Statically-linked at a fixed address
    };
    let image_vmar = vmar.allocate(min_offset, size, VmarFlags::CAN_MAP_RXW, PAGE_SIZE)?;
    let _vmo = image_vmar.load_from_elf(&elf)?;
    let base = image_vmar.addr();
    let entry = base + elf.header.pt2.entry_point() as usize;

    // Apply ELF relocations only for statically-linked executables.
    // When has_interp is true, we loaded the dynamic linker (ld.so.1),
    // which self-relocates via its rcrt1 entry code.  Applying relocations
    // here too would double-relocate all RELR entries, corrupting data
    // pointers (e.g. stdout's FILE struct gets base added twice).
    if !has_interp {
        if let Err(e) = elf.relocate(image_vmar.clone()) {
            warn!("spawn_process: ELF relocation failed: {}", e);
        }
    }

    // Stack
    let stack_size = config.stack_pages * PAGE_SIZE;
    let stack_vmo = VmObject::new_paged(config.stack_pages);
    stack_vmo.set_name(&format!("{}-stack", name));
    let stack_flags = MMUFlags::READ | MMUFlags::WRITE | MMUFlags::USER;
    let stack_base = vmar.map(None, stack_vmo, 0, stack_size, stack_flags)?;
    let sp = stack_base + stack_size;

    // vDSO: map the entire ELF into the process address space.
    let vdso_flags = MMUFlags::READ | MMUFlags::EXECUTE | MMUFlags::USER;
    let vdso_code_addr = vmar.map(
        None,
        config.vdso_vmo.clone(),
        0,
        config.vdso_code_size,
        vdso_flags,
    )?;

    // Eagerly resolve vDSO symbols in the loaded ELF's PLT.
    // ld.so.1 can't use lazy PLT binding before initializing its resolver.
    // We resolve _zx_* and zx_* JUMP_SLOT entries to their vDSO addresses.
    #[cfg(feature = "dynamic-linking")]
    if has_interp {
        vdso_plt::resolve_vdso_plt(load_data, base, vdso_code_addr, &vmar);
    }

    // Bootstrap channel -- construct a Fuchsia processargs message.
    let (ch0, ch1) = Channel::create();

    let root_job = job.clone();
    use crate::dev::Resource;
    let root_resource = Resource::create(
        "root",
        crate::dev::ResourceKind::ROOT,
        0,
        0,
        crate::dev::ResourceFlags::empty(),
    );

    // Build the handle list and corresponding handle_info entries.
    // The handle_info array tells the receiver what each handle is.
    use zircon_abi::processargs::*;

    // Build argv early -- it's included in message 1 for the new-style
    // libc startup (StartCompilerAbi + _zx_startup_get_handles).
    let mut argv = format!("{}\0", name);
    for arg in &config.extra_args {
        argv.push_str(arg);
        argv.push('\0');
    }

    // TWO processargs messages on the bootstrap channel:
    // Message 1 (read by libc's _zx_startup_get_handles): loader
    //   handles + process identity + argv
    // Message 2 (read by old-style libc): duplicate handles + argv

    // --- Message 1: For ld.so.1 (old-style processargs_read) ---
    // Include ALL handles so ld.so.1 can process them.
    let root_resource2 = Resource::create(
        "root",
        crate::dev::ResourceKind::ROOT,
        0,
        0,
        crate::dev::ResourceFlags::empty(),
    );
    let msg1_base_handles = alloc::vec![
        Handle::new(proc.clone(), Rights::DEFAULT_PROCESS), // PA_PROC_SELF
        Handle::new(thread.clone(), Rights::DEFAULT_THREAD), // PA_THREAD_SELF
        Handle::new(proc.vmar(), Rights::all()),            // PA_VMAR_ROOT
        Handle::new(job.clone(), Rights::DEFAULT_JOB),      // PA_JOB_DEFAULT
        Handle::new(config.vdso_vmo.clone(), Rights::DEFAULT_VMO), // PA_VMO_VDSO
        Handle::new(root_resource2, Rights::DEFAULT_RESOURCE), // PA_RESOURCE
        Handle::new(image_vmar.clone(), Rights::DEFAULT_VMAR), // PA_VMAR_LOADED
    ];
    let msg1_base_info = alloc::vec![
        pa_hnd(PA_PROC_SELF, 0),
        pa_hnd(PA_THREAD_SELF, 0),
        pa_hnd(PA_VMAR_ROOT, 0),
        pa_hnd(PA_JOB_DEFAULT, 0),
        pa_hnd(PA_VMO_VDSO, 0),
        pa_hnd(PA_RESOURCE, 0),
        pa_hnd(PA_VMAR_LOADED, 0),
    ];

    // When dynamic linking is enabled, add the executable VMO and
    // loader-service channel to message 1 for ld.so.1.
    #[cfg(feature = "dynamic-linking")]
    let (msg1_handles, msg1_info, ldsvc_kernel_end) = {
        let mut handles = msg1_base_handles;
        let mut info = msg1_base_info;
        let ldsvc = if has_interp {
            let prog_vmo = VmObject::new_paged(crate::vm::pages(elf_data.len()));
            prog_vmo.write(0, elf_data)?;
            prog_vmo.set_name(name);
            handles.push(Handle::new(prog_vmo, Rights::DEFAULT_VMO | Rights::EXECUTE));
            info.push(pa_hnd(PA_VMO_EXECUTABLE, 0));

            let (ldsvc_kernel, ldsvc_user) = Channel::create();
            handles.push(Handle::new(ldsvc_user, Rights::DEFAULT_CHANNEL));
            info.push(pa_hnd(PA_LDSVC_LOADER, 0));
            Some(ldsvc_kernel)
        } else {
            None
        };
        (handles, info, ldsvc)
    };
    #[cfg(not(feature = "dynamic-linking"))]
    let (msg1_handles, msg1_info) = (msg1_base_handles, msg1_base_info);

    let msg1_data = build_message(&msg1_info, argv.as_bytes());
    ch0.write(crate::ipc::MessagePacket {
        data: msg1_data,
        handles: msg1_handles,
    })
    .map_err(|_| crate::ZxError::INTERNAL)?;

    // --- Message 2: For libc's _zx_startup_get_handles ---
    // Include sub-resource handles required by standalone tests.
    let mmio_resource = Resource::create(
        "mmio",
        crate::dev::ResourceKind::MMIO,
        0,
        0,
        crate::dev::ResourceFlags::empty(),
    );
    let irq_resource = Resource::create(
        "irq",
        crate::dev::ResourceKind::IRQ,
        0,
        0,
        crate::dev::ResourceFlags::empty(),
    );
    let system_resource = Resource::create(
        "system",
        crate::dev::ResourceKind::SYSTEM,
        0,
        0,
        crate::dev::ResourceFlags::empty(),
    );
    // Create a minimal ZBI VMO for standalone test's GetOptions/GetBootOptions.
    // The VMO name must be "zbi" -- standalone-init.cc looks it up by name.
    let zbi_vmo = VmObject::new_paged(1);
    // Write a minimal ZBI container header (empty, no items)
    let zbi_header: [u8; 32] = {
        let mut h = [0u8; 32];
        // ZBI_TYPE_CONTAINER = 0x544f4f42 ("BOOT")
        h[0..4].copy_from_slice(&0x544f4f42u32.to_le_bytes());
        // length = 0 (no items after header)
        h[4..8].copy_from_slice(&0u32.to_le_bytes());
        // extra = ZBI_CONTAINER_MAGIC = 0x868cf7e6
        h[8..12].copy_from_slice(&0x868cf7e6u32.to_le_bytes());
        // flags = ZBI_FLAGS_VERSION = 0x00010000
        h[12..16].copy_from_slice(&0x00010000u32.to_le_bytes());
        // reserved0, reserved1 = 0
        // magic = ZBI_ITEM_MAGIC = 0xb5781729
        h[24..28].copy_from_slice(&0xb5781729u32.to_le_bytes());
        // crc32 = ZBI_ITEM_NO_CRC32 = 0x4a87e8d6
        h[28..32].copy_from_slice(&0x4a87e8d6u32.to_le_bytes());
        h
    };
    zbi_vmo.write(0, &zbi_header).ok();
    zbi_vmo.set_name("zbi");

    // Create boot-options.txt VMO (empty, no boot options)
    let boot_opts_vmo = VmObject::new_paged(1);
    boot_opts_vmo.set_name("boot-options.txt");

    // Create a UTC clock for libc's clock_gettime(CLOCK_REALTIME).
    // Auto-started so it's immediately readable.
    use crate::signal::Clock;
    const ZX_CLOCK_OPT_AUTO_START: u64 = 1 << 0;
    let utc_clock = Clock::new(ZX_CLOCK_OPT_AUTO_START)?;
    let utc_clock = alloc::sync::Arc::new(utc_clock);

    let msg2_handles = alloc::vec![
        Handle::new(proc.clone(), Rights::DEFAULT_PROCESS), // PA_PROC_SELF
        Handle::new(thread.clone(), Rights::DEFAULT_THREAD), // PA_THREAD_SELF
        Handle::new(proc.vmar(), Rights::all()),            // PA_VMAR_ROOT
        Handle::new(root_job, Rights::DEFAULT_JOB),         // PA_JOB_DEFAULT
        Handle::new(config.vdso_vmo.clone(), Rights::DEFAULT_VMO), // PA_VMO_VDSO
        Handle::new(root_resource, Rights::DEFAULT_RESOURCE), // PA_RESOURCE
        Handle::new(image_vmar.clone(), Rights::DEFAULT_VMAR), // PA_VMAR_LOADED
        Handle::new(mmio_resource, Rights::DEFAULT_RESOURCE), // PA_MMIO_RESOURCE
        Handle::new(irq_resource, Rights::DEFAULT_RESOURCE), // PA_IRQ_RESOURCE
        Handle::new(system_resource, Rights::DEFAULT_RESOURCE), // PA_SYSTEM_RESOURCE
        Handle::new(zbi_vmo, Rights::DEFAULT_VMO),          // PA_VMO_BOOTDATA
        Handle::new(boot_opts_vmo, Rights::DEFAULT_VMO),    // PA_VMO_BOOTDATA (boot-options.txt)
        Handle::new(
            utc_clock,
            Rights::DUPLICATE
                | Rights::TRANSFER
                | Rights::READ
                | Rights::WAIT
                | Rights::INSPECT
                | Rights::SIGNAL
                | Rights::MAP
        ), // PA_CLOCK_UTC
    ];
    let msg2_info = alloc::vec![
        pa_hnd(PA_PROC_SELF, 0),
        pa_hnd(PA_THREAD_SELF, 0),
        pa_hnd(PA_VMAR_ROOT, 0),
        pa_hnd(PA_JOB_DEFAULT, 0),
        pa_hnd(PA_VMO_VDSO, 0),
        pa_hnd(PA_RESOURCE, 0),
        pa_hnd(PA_VMAR_LOADED, 0),
        pa_hnd(PA_MMIO_RESOURCE, 0),
        pa_hnd(PA_IRQ_RESOURCE, 0),
        pa_hnd(PA_SYSTEM_RESOURCE, 0),
        pa_hnd(PA_VMO_BOOTDATA, 0),
        pa_hnd(PA_VMO_BOOTDATA, 1), // arg=1 distinguishes boot-options.txt
        pa_hnd(PA_CLOCK_UTC, 0),
    ];
    let msg2_data = build_message(&msg2_info, argv.as_bytes());

    ch0.write(crate::ipc::MessagePacket {
        data: msg2_data,
        handles: msg2_handles,
    })
    .map_err(|_| crate::ZxError::INTERNAL)?;

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

    // Spawn the loader service if this is a dynamically linked binary.
    #[cfg(feature = "dynamic-linking")]
    if let Some(ldsvc) = ldsvc_kernel_end {
        loader_service::spawn_loader_service(ldsvc);
    }

    Ok(proc)
}
