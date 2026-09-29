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
/// If the ELF has a `PT_INTERP` header (dynamically linked), the
/// interpreter (e.g., `ld.so.1`) is loaded from the rootfs instead.
/// The original program is passed as a VMO handle on the bootstrap
/// channel so the dynamic linker can load it.
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
    let interp_path = elf_check
        .get_interpreter()
        .ok()
        .map(alloc::string::String::from);

    // If there's an interpreter, load it from rootfs.
    let interp_data = if let Some(ref path) = interp_path {
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

    // Apply ELF relocations (R_X86_64_RELATIVE etc.) for PIE/shared
    // objects like ld.so.1 which are loaded at a non-zero base.
    if let Err(e) = elf.relocate(image_vmar.clone()) {
        warn!("spawn_process: ELF relocation failed: {}", e);
    }

    // Stack
    let stack_size = config.stack_pages * PAGE_SIZE;
    let stack_vmo = VmObject::new_paged(config.stack_pages);
    stack_vmo.set_name(&format!("{}-stack", name));
    let stack_flags = MMUFlags::READ | MMUFlags::WRITE | MMUFlags::USER;
    let stack_base = vmar.map(None, stack_vmo, 0, stack_size, stack_flags)?;
    let sp = stack_base + stack_size;

    // vDSO: map entire ELF .so + data page as a single contiguous RX block.
    // This allows ld.so.1 to parse ELF headers at the base address.
    let vdso_flags = MMUFlags::READ | MMUFlags::EXECUTE | MMUFlags::USER;
    let vdso_code_addr = vmar.map(
        None,
        config.vdso_vmo.clone(),
        0,
        config.vdso_code_size,
        vdso_flags,
    )?;

    // Bootstrap channel — construct a Fuchsia processargs message.
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
    // Fuchsia processargs protocol constants.
    const ZX_PROCARGS_PROTOCOL: u32 = 0x4150_585a; // "ZXPA"
    const ZX_PROCARGS_VERSION: u32 = 0x0001_0000;
    const fn pa_hnd(t: u32, a: u32) -> u32 {
        (t & 0xFFFF) | ((a & 0xFFFF) << 16)
    }
    const PA_PROC_SELF: u32 = 0x01;
    const PA_THREAD_SELF: u32 = 0x02;
    const PA_JOB_DEFAULT: u32 = 0x03;
    const PA_VMAR_ROOT: u32 = 0x04;
    const PA_VMAR_LOADED: u32 = 0x05;
    const PA_LDSVC_LOADER: u32 = 0x10;
    const PA_VMO_VDSO: u32 = 0x11;
    const PA_VMO_EXECUTABLE: u32 = 0x14;
    const PA_RESOURCE: u32 = 0x3F;

    let proc_handle = Handle::new(proc.clone(), Rights::DEFAULT_PROCESS);
    let thread_handle = Handle::new(thread.clone(), Rights::DEFAULT_THREAD);
    let vmar_handle = Handle::new(proc.vmar(), Rights::DEFAULT_VMAR);
    let job_handle = Handle::new(root_job, Rights::DEFAULT_CHANNEL);
    let vdso_handle = Handle::new(config.vdso_vmo.clone(), Rights::DEFAULT_VMO);
    let resource_handle = Handle::new(root_resource, Rights::DEFAULT_CHANNEL);
    let image_vmar_handle = Handle::new(image_vmar.clone(), Rights::DEFAULT_VMAR);

    let mut bootstrap_handles = alloc::vec![
        proc_handle,       // 0: PA_PROC_SELF
        thread_handle,     // 1: PA_THREAD_SELF
        vmar_handle,       // 2: PA_VMAR_ROOT
        job_handle,        // 3: PA_JOB_DEFAULT
        vdso_handle,       // 4: PA_VMO_VDSO
        resource_handle,   // 5: PA_RESOURCE
        image_vmar_handle, // 6: PA_VMAR_LOADED
    ];
    let mut handle_info = alloc::vec![
        pa_hnd(PA_PROC_SELF, 0),
        pa_hnd(PA_THREAD_SELF, 0),
        pa_hnd(PA_VMAR_ROOT, 0),
        pa_hnd(PA_JOB_DEFAULT, 0),
        pa_hnd(PA_VMO_VDSO, 0),
        pa_hnd(PA_RESOURCE, 0),
        pa_hnd(PA_VMAR_LOADED, 0),
    ];

    // If dynamically linked, pass the original program as a VMO
    // and a loader service channel for resolving shared libraries.
    let ldsvc_kernel_end = if has_interp {
        let prog_vmo = VmObject::new_paged(crate::vm::pages(elf_data.len()));
        prog_vmo.write(0, elf_data)?;
        prog_vmo.set_name(name);
        bootstrap_handles.push(Handle::new(prog_vmo, Rights::DEFAULT_VMO | Rights::EXECUTE));
        handle_info.push(pa_hnd(PA_VMO_EXECUTABLE, 0));

        // Loader service: ld.so.1 sends library name requests on this
        // channel, and the kernel responds with VMO handles.
        let (ldsvc_kernel, ldsvc_user) = Channel::create();
        bootstrap_handles.push(Handle::new(ldsvc_user, Rights::DEFAULT_CHANNEL));
        handle_info.push(pa_hnd(PA_LDSVC_LOADER, 0));
        Some(ldsvc_kernel)
    } else {
        None
    };

    let handle_count = bootstrap_handles.len();

    // Construct the processargs message data:
    // [zx_proc_args_t header (36 bytes)]
    // [handle_info array (4 * N bytes)]
    // [argv strings (NUL-separated)]
    let header_size = 36usize;
    let handle_info_off = header_size;
    let handle_info_size = handle_count * 4;
    let args_off = handle_info_off + handle_info_size;

    // argv: just the program name
    let argv = format!("{}\0", name);
    let total_size = args_off + argv.len();

    let mut data = alloc::vec![0u8; total_size];

    // Write header
    data[0..4].copy_from_slice(&ZX_PROCARGS_PROTOCOL.to_le_bytes());
    data[4..8].copy_from_slice(&ZX_PROCARGS_VERSION.to_le_bytes());
    data[8..12].copy_from_slice(&(handle_info_off as u32).to_le_bytes());
    data[12..16].copy_from_slice(&(args_off as u32).to_le_bytes());
    data[16..20].copy_from_slice(&1u32.to_le_bytes()); // args_num = 1
    data[20..24].copy_from_slice(&(total_size as u32).to_le_bytes()); // environ_off (end = no envs)
    data[24..28].copy_from_slice(&0u32.to_le_bytes()); // environ_num = 0
    data[28..32].copy_from_slice(&(total_size as u32).to_le_bytes()); // names_off (end = no names)
    data[32..36].copy_from_slice(&0u32.to_le_bytes()); // names_num = 0

    // Write handle_info array
    for (i, &info) in handle_info.iter().enumerate() {
        let off = handle_info_off + i * 4;
        data[off..off + 4].copy_from_slice(&info.to_le_bytes());
    }

    // Write argv string
    data[args_off..args_off + argv.len()].copy_from_slice(argv.as_bytes());

    let msg = crate::ipc::MessagePacket {
        data,
        handles: bootstrap_handles,
    };
    ch0.write(msg).map_err(|_| crate::ZxError::INTERNAL)?;

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
    if let Some(ldsvc) = ldsvc_kernel_end {
        spawn_loader_service(ldsvc);
    }

    Ok(proc)
}

/// Spawn a kernel task that serves the Fuchsia loader service protocol.
///
/// `ld.so.1` sends `LOADER_SVC_OP_LOAD_OBJECT` (opcode 2) requests
/// on the channel with a library name. We look up the library in
/// the rootfs at `/lib/<name>`, create a VMO from the file data,
/// and send it back as a handle.
fn spawn_loader_service(channel: Arc<Channel>) {
    use alloc::sync::Arc;

    // The loader service runs as a synchronous loop on a kernel task.
    // It exits when the channel is closed (ld.so.1 drops its end
    // after finishing loading).
    hal_impl::thread::spawn(async move {
        info!("loader_service: started");
        loop {
            // Wait for a message from ld.so.1.
            let object: Arc<dyn crate::object::KernelObject> = channel.clone();
            object
                .wait_signal(crate::object::Signal::READABLE | crate::object::Signal::PEER_CLOSED)
                .await;

            // Check if the channel was closed.
            if channel
                .signal()
                .contains(crate::object::Signal::PEER_CLOSED)
            {
                info!("loader_service: channel closed, exiting");
                break;
            }

            // Read the request.
            let msg = match channel.read() {
                Ok(msg) => msg,
                Err(e) => {
                    warn!("loader_service: read failed: {:?}", e);
                    break;
                }
            };

            // Parse: first 4 bytes = opcode, rest = library name.
            if msg.data.len() < 4 {
                warn!("loader_service: message too short");
                continue;
            }
            let opcode = u32::from_le_bytes(msg.data[0..4].try_into().unwrap());

            match opcode {
                // LOADER_SVC_OP_LOAD_OBJECT = 2
                2 => {
                    let name_bytes = &msg.data[4..];
                    // Trim trailing NUL if present.
                    let name_end = name_bytes
                        .iter()
                        .position(|&b| b == 0)
                        .unwrap_or(name_bytes.len());
                    let name = core::str::from_utf8(&name_bytes[..name_end]).unwrap_or("?");
                    info!("loader_service: LOAD_OBJECT '{}'", name);

                    // Look up the library in the rootfs.
                    let lib_path = format!("/lib/{}", name);
                    let response = if let Some(data) = read_rootfs_file(&lib_path) {
                        info!(
                            "loader_service: found '{}' ({} bytes)",
                            lib_path,
                            data.len()
                        );
                        let vmo = VmObject::new_paged(crate::vm::pages(data.len()));
                        if let Err(e) = vmo.write(0, &data) {
                            warn!("loader_service: VMO write failed: {:?}", e);
                            // Send error response (no handles, status in data).
                            crate::ipc::MessagePacket {
                                data: alloc::vec![0xFF; 4], // error
                                handles: alloc::vec![],
                            }
                        } else {
                            vmo.set_name(name);
                            // Send success: status 0 + VMO handle.
                            crate::ipc::MessagePacket {
                                data: alloc::vec![0u8; 4], // ZX_OK
                                handles: alloc::vec![Handle::new(
                                    vmo,
                                    Rights::DEFAULT_VMO | Rights::EXECUTE,
                                )],
                            }
                        }
                    } else {
                        warn!("loader_service: '{}' not found in rootfs", lib_path);
                        crate::ipc::MessagePacket {
                            data: alloc::vec![0xFF; 4], // error
                            handles: alloc::vec![],
                        }
                    };

                    if let Err(e) = channel.write(response) {
                        warn!("loader_service: write response failed: {:?}", e);
                        break;
                    }
                }
                // LOADER_SVC_OP_CONFIG = 3 (set library search path prefix)
                3 => {
                    info!("loader_service: CONFIG (ignored)");
                    let response = crate::ipc::MessagePacket {
                        data: alloc::vec![0u8; 4], // ZX_OK
                        handles: alloc::vec![],
                    };
                    channel.write(response).ok();
                }
                _ => {
                    warn!("loader_service: unknown opcode {}", opcode);
                    let response = crate::ipc::MessagePacket {
                        data: alloc::vec![0xFF; 4], // error
                        handles: alloc::vec![],
                    };
                    channel.write(response).ok();
                }
            }
        }
        info!("loader_service: done");
    });
}
