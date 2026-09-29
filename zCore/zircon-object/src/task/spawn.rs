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
use crate::vm::{MMUFlags, VmAddressRegion, VmObject, VmarFlags, PAGE_SIZE};

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

    // Eagerly resolve vDSO symbols in the loaded ELF's PLT.
    // ld.so.1 can't use lazy PLT binding before initializing its resolver.
    // We resolve _zx_* and zx_* JUMP_SLOT entries to their vDSO addresses.
    if has_interp {
        resolve_vdso_plt(load_data, base, vdso_code_addr, &vmar);
    }

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
    const ZX_PROCARGS_PROTOCOL: u32 = 0x4150_585d; // from processargs.h
    const ZX_PROCARGS_VERSION: u32 = 0x0000_1000;
    const fn pa_hnd(t: u32, a: u32) -> u32 {
        (t & 0xFFFF) | ((a & 0xFFFF) << 16)
    }
    // Fuchsia processargs handle type constants.
    // From zircon/system/public/zircon/processargs.h
    const PA_PROC_SELF: u32 = 0x01;
    const PA_THREAD_SELF: u32 = 0x02;
    const PA_JOB_DEFAULT: u32 = 0x03;
    const PA_VMAR_ROOT: u32 = 0x04;
    const PA_LDSVC_LOADER: u32 = 0x10;
    const PA_VMO_VDSO: u32 = 0x11;
    const PA_VMO_EXECUTABLE: u32 = 0x14;
    const PA_VMAR_LOADED: u32 = 0x05;
    const PA_RESOURCE: u32 = 0x3F;

    // Send a single processargs message with ALL handles.
    // The new Fuchsia libc (_zx_startup_get_handles) reads exactly one
    // message and processes all handles from it.
    let mut bootstrap_handles = alloc::vec![
        Handle::new(proc.clone(), Rights::DEFAULT_PROCESS), // PA_PROC_SELF
        Handle::new(thread.clone(), Rights::DEFAULT_THREAD), // PA_THREAD_SELF
        Handle::new(proc.vmar(), Rights::all()),            // PA_VMAR_ROOT
        Handle::new(root_job, Rights::DEFAULT_JOB),         // PA_JOB_DEFAULT
        Handle::new(config.vdso_vmo.clone(), Rights::DEFAULT_VMO), // PA_VMO_VDSO
        Handle::new(root_resource, Rights::DEFAULT_RESOURCE), // PA_RESOURCE
        Handle::new(image_vmar.clone(), Rights::DEFAULT_VMAR), // PA_VMAR_LOADED
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

    // If dynamically linked, add executable VMO and loader service
    let ldsvc_kernel_end = if has_interp {
        let prog_vmo = VmObject::new_paged(crate::vm::pages(elf_data.len()));
        prog_vmo.write(0, elf_data)?;
        prog_vmo.set_name(name);
        bootstrap_handles.push(Handle::new(prog_vmo, Rights::DEFAULT_VMO | Rights::EXECUTE));
        handle_info.push(pa_hnd(PA_VMO_EXECUTABLE, 0));

        let (ldsvc_kernel, ldsvc_user) = Channel::create();
        bootstrap_handles.push(Handle::new(ldsvc_user, Rights::DEFAULT_CHANNEL));
        handle_info.push(pa_hnd(PA_LDSVC_LOADER, 0));
        Some(ldsvc_kernel)
    } else {
        None
    };

    let argv = format!("{}\0", name);
    let msg_data = build_processargs_data(
        ZX_PROCARGS_PROTOCOL,
        ZX_PROCARGS_VERSION,
        &handle_info,
        argv.as_bytes(),
    );
    ch0.write(crate::ipc::MessagePacket {
        data: msg_data,
        handles: bootstrap_handles,
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

            // Parse FIDL loader service message.
            // Header: txid(u32) + flags(3 bytes) + magic(1 byte) + ordinal(u64) = 16 bytes
            // Payload for LOAD_OBJECT: fidl_string_t(size:u64, data:u64) + string bytes
            if msg.data.len() < 16 {
                warn!("loader_service: message too short ({})", msg.data.len());
                continue;
            }
            let txid = u32::from_le_bytes(msg.data[0..4].try_into().unwrap());
            let ordinal = u64::from_le_bytes(msg.data[8..16].try_into().unwrap());

            const LDMSG_OP_LOAD_OBJECT: u64 = 0x48C5_A151_D6DF_2853;
            const LDMSG_OP_DONE: u64 = 0x63BA_6B76_D367_1001;
            const LDMSG_OP_CONFIG: u64 = 0x6A8A_1A14_6463_2841;

            match ordinal {
                LDMSG_OP_LOAD_OBJECT => {
                    // Payload: fidl_string_t at offset 16
                    if msg.data.len() < 32 {
                        warn!("loader_service: LOAD_OBJECT too short");
                        continue;
                    }
                    let str_size =
                        u64::from_le_bytes(msg.data[16..24].try_into().unwrap()) as usize;
                    // String data starts at offset 32 (after fidl_string_t)
                    let name_bytes = if msg.data.len() >= 32 + str_size {
                        &msg.data[32..32 + str_size]
                    } else {
                        &msg.data[32..]
                    };
                    // Trim trailing NUL if present.
                    let name_end = name_bytes
                        .iter()
                        .position(|&b| b == 0)
                        .unwrap_or(name_bytes.len());
                    let name = core::str::from_utf8(&name_bytes[..name_end]).unwrap_or("?");
                    info!("loader_service: LOAD_OBJECT '{}'", name);

                    // Look up the library in the rootfs.
                    let lib_path = format!("/lib/{}", name);
                    let response = if let Some(file_data) = read_rootfs_file(&lib_path) {
                        info!(
                            "loader_service: found '{}' ({} bytes)",
                            lib_path,
                            file_data.len()
                        );
                        let vmo = VmObject::new_paged(crate::vm::pages(file_data.len()));
                        if let Err(e) = vmo.write(0, &file_data) {
                            warn!("loader_service: VMO write failed: {:?}", e);
                            make_ldmsg_response(txid, ordinal, -1i32, alloc::vec![])
                        } else {
                            vmo.set_name(name);
                            make_ldmsg_response(
                                txid,
                                ordinal,
                                0i32, // ZX_OK
                                alloc::vec![Handle::new(
                                    vmo,
                                    Rights::DEFAULT_VMO | Rights::EXECUTE,
                                )],
                            )
                        }
                    } else {
                        warn!("loader_service: '{}' not found in rootfs", lib_path);
                        make_ldmsg_response(txid, ordinal, -1i32, alloc::vec![])
                    };

                    if let Err(e) = channel.write(response) {
                        warn!("loader_service: write response failed: {:?}", e);
                        break;
                    }
                }
                LDMSG_OP_CONFIG => {
                    info!("loader_service: CONFIG (ignored)");
                    let response = make_ldmsg_response(txid, ordinal, 0i32, alloc::vec![]);
                    channel.write(response).ok();
                }
                LDMSG_OP_DONE => {
                    info!("loader_service: DONE");
                    break;
                }
                _ => {
                    warn!("loader_service: unknown ordinal {:#x}", ordinal);
                    let response = make_ldmsg_response(txid, ordinal, -2i32, alloc::vec![]); // NOT_SUPPORTED
                    channel.write(response).ok();
                }
            }
        }
        info!("loader_service: done");
    });
}

/// Build a processargs message data buffer.
fn build_processargs_data(
    protocol: u32,
    version: u32,
    handle_info: &[u32],
    argv: &[u8],
) -> alloc::vec::Vec<u8> {
    let header_size = 36usize;
    let handle_info_off = header_size;
    let handle_info_size = handle_info.len() * 4;
    let args_off = handle_info_off + handle_info_size;
    let args_num = if argv.is_empty() {
        0u32
    } else {
        argv.iter().filter(|&&b| b == 0).count() as u32
    };
    let total_size = args_off + argv.len();

    let mut data = alloc::vec![0u8; total_size];
    data[0..4].copy_from_slice(&protocol.to_le_bytes());
    data[4..8].copy_from_slice(&version.to_le_bytes());
    data[8..12].copy_from_slice(&(handle_info_off as u32).to_le_bytes());
    data[12..16].copy_from_slice(&(args_off as u32).to_le_bytes());
    data[16..20].copy_from_slice(&args_num.to_le_bytes());
    data[20..24].copy_from_slice(&(total_size as u32).to_le_bytes()); // environ_off
    data[24..28].copy_from_slice(&0u32.to_le_bytes()); // environ_num
    data[28..32].copy_from_slice(&(total_size as u32).to_le_bytes()); // names_off
    data[32..36].copy_from_slice(&0u32.to_le_bytes()); // names_num

    for (i, &info) in handle_info.iter().enumerate() {
        let off = handle_info_off + i * 4;
        data[off..off + 4].copy_from_slice(&info.to_le_bytes());
    }

    if !argv.is_empty() {
        data[args_off..args_off + argv.len()].copy_from_slice(argv);
    }

    data
}

/// Construct a FIDL loader service response message.
///
/// Format: fidl_message_header_t (16 bytes) + status (i32) + handle_present (u32)
fn make_ldmsg_response(
    txid: u32,
    ordinal: u64,
    status: i32,
    handles: alloc::vec::Vec<Handle>,
) -> crate::ipc::MessagePacket {
    let mut data = alloc::vec![0u8; 24];
    // fidl_message_header_t
    data[0..4].copy_from_slice(&txid.to_le_bytes());
    // at_rest_flags[0] = 0x02 (USE_VERSION_V2), rest = 0
    data[4] = 0x02;
    // magic_number = 0x01 (FIDL magic)
    data[7] = 0x01;
    data[8..16].copy_from_slice(&ordinal.to_le_bytes());
    // rv (status)
    data[16..20].copy_from_slice(&status.to_le_bytes());
    // handle present/absent marker
    if !handles.is_empty() {
        data[20..24].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // FIDL_HANDLE_PRESENT
    }
    crate::ipc::MessagePacket { data, handles }
}

/// Eagerly resolve vDSO symbols in an ELF's PLT GOT.
///
/// Parses the vDSO ELF to build a symbol lookup table, then patches
/// JUMP_SLOT entries in `elf_data` that reference `_zx_*` or `zx_*`
/// symbols to point to the vDSO trampoline addresses.
///
/// This is needed because `ld.so.1` calls `_zx_*` functions via PLT
/// before its lazy binding resolver is initialized.
fn resolve_vdso_plt(
    elf_data: &[u8],
    elf_base: usize,
    vdso_base: usize,
    vmar: &Arc<VmAddressRegion>,
) {
    // Parse the vDSO ELF to build symbol name → offset map.
    // The vDSO ELF is the same data that's in the kernel's VDSO_ELF static.
    // We access it via the VMO content since we don't have a direct reference here.
    // Read the vDSO from the mapped memory.
    let vdso_vmo_data = {
        // Read the vDSO ELF header to find .dynsym and .dynstr
        let mut ehdr = [0u8; 64];
        if vmar.read_memory(vdso_base, &mut ehdr).is_err() {
            warn!("resolve_vdso_plt: cannot read vDSO ELF header");
            return;
        }
        // Check magic
        if ehdr[0..4] != [0x7f, b'E', b'L', b'F'] {
            warn!("resolve_vdso_plt: vDSO is not an ELF");
            return;
        }
        ehdr
    };

    // Parse vDSO's PT_DYNAMIC to find .dynsym, .dynstr, .hash
    let e_phoff = u64::from_le_bytes(vdso_vmo_data[32..40].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(vdso_vmo_data[54..56].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(vdso_vmo_data[56..58].try_into().unwrap()) as usize;

    let mut vdso_dynamic_off = 0usize;
    let mut vdso_dynamic_sz = 0usize;
    for i in 0..e_phnum {
        let ph_off = e_phoff + i * e_phentsize;
        let mut phdr = [0u8; 56];
        if vmar.read_memory(vdso_base + ph_off, &mut phdr).is_err() {
            continue;
        }
        let p_type = u32::from_le_bytes(phdr[0..4].try_into().unwrap());
        if p_type == 2 {
            // PT_DYNAMIC
            vdso_dynamic_off = u64::from_le_bytes(phdr[8..16].try_into().unwrap()) as usize;
            vdso_dynamic_sz = u64::from_le_bytes(phdr[32..40].try_into().unwrap()) as usize;
        }
    }
    if vdso_dynamic_off == 0 {
        warn!("resolve_vdso_plt: no PT_DYNAMIC in vDSO");
        return;
    }

    // Read vDSO .dynamic entries
    let mut vdso_symtab = 0usize;
    let mut vdso_strtab = 0usize;
    let mut vdso_hash = 0usize;
    let mut vdso_syment = 24usize;
    for i in (0..vdso_dynamic_sz).step_by(16) {
        let mut dyn_entry = [0u8; 16];
        if vmar
            .read_memory(vdso_base + vdso_dynamic_off + i, &mut dyn_entry)
            .is_err()
        {
            break;
        }
        let tag = i64::from_le_bytes(dyn_entry[0..8].try_into().unwrap());
        let val = u64::from_le_bytes(dyn_entry[8..16].try_into().unwrap()) as usize;
        match tag {
            0 => break,              // DT_NULL
            4 => vdso_hash = val,    // DT_HASH
            5 => vdso_strtab = val,  // DT_STRTAB
            6 => vdso_symtab = val,  // DT_SYMTAB
            11 => vdso_syment = val, // DT_SYMENT
            _ => {}
        }
    }
    if vdso_symtab == 0 || vdso_strtab == 0 || vdso_hash == 0 {
        warn!("resolve_vdso_plt: incomplete vDSO dynamic section");
        return;
    }

    // Read vDSO hash table to get nsyms
    let mut hash_hdr = [0u8; 8];
    if vmar
        .read_memory(vdso_base + vdso_hash, &mut hash_hdr)
        .is_err()
    {
        return;
    }
    let vdso_nchain = u32::from_le_bytes(hash_hdr[4..8].try_into().unwrap()) as usize;

    // Now parse ld.so.1's dynamic section to find its JMPREL entries
    // We need: DT_JMPREL, DT_PLTRELSZ, DT_SYMTAB, DT_STRTAB
    let elf = match xmas_elf::ElfFile::new(elf_data) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut ld_jmprel = 0u64;
    let mut ld_pltrelsz = 0u64;
    let mut ld_symtab = 0u64;
    let mut ld_strtab = 0u64;
    let mut ld_syment = 24u64;
    let mut ld_relent = 16u64;

    for ph in elf.program_iter() {
        if ph.get_type().unwrap_or(xmas_elf::program::Type::Null)
            == xmas_elf::program::Type::Dynamic
        {
            let dyn_off = ph.offset() as usize;
            let dyn_sz = ph.file_size() as usize;
            for i in (0..dyn_sz).step_by(16) {
                let off = dyn_off + i;
                if off + 16 > elf_data.len() {
                    break;
                }
                let tag = i64::from_le_bytes(elf_data[off..off + 8].try_into().unwrap());
                let val = u64::from_le_bytes(elf_data[off + 8..off + 16].try_into().unwrap());
                match tag {
                    0 => break,
                    2 => ld_pltrelsz = val, // DT_PLTRELSZ
                    5 => ld_strtab = val,   // DT_STRTAB
                    6 => ld_symtab = val,   // DT_SYMTAB
                    11 => ld_syment = val,  // DT_SYMENT
                    19 => ld_relent = val,  // DT_RELENT
                    23 => ld_jmprel = val,  // DT_JMPREL
                    _ => {}
                }
            }
        }
    }
    if ld_jmprel == 0 || ld_pltrelsz == 0 {
        return;
    }
    // Use DT_RELENT if available, otherwise default to 16 (REL on x86_64)
    if ld_relent == 0 {
        ld_relent = 16;
    }

    let count = ld_pltrelsz as usize / ld_relent as usize;
    let mut resolved = 0usize;
    for i in 0..count {
        let off = ld_jmprel as usize + i * ld_relent as usize;
        if off + 16 > elf_data.len() {
            break;
        }
        let r_offset = u64::from_le_bytes(elf_data[off..off + 8].try_into().unwrap());
        let r_info = u64::from_le_bytes(elf_data[off + 8..off + 16].try_into().unwrap());
        let r_type = (r_info & 0xFFFF_FFFF) as u32;
        let r_sym = (r_info >> 32) as usize;

        // Only handle JUMP_SLOT
        if r_type != 7 && r_type != 1026 {
            continue;
        }

        // Look up the symbol name in ld.so.1's .dynstr
        let sym_off = ld_symtab as usize + r_sym * ld_syment as usize;
        if sym_off + 4 > elf_data.len() {
            continue;
        }
        let st_name = u32::from_le_bytes(elf_data[sym_off..sym_off + 4].try_into().unwrap());
        let name_off = ld_strtab as usize + st_name as usize;
        if name_off >= elf_data.len() {
            continue;
        }
        let name_end = elf_data[name_off..]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(0);
        let sym_name = core::str::from_utf8(&elf_data[name_off..name_off + name_end]).unwrap_or("");

        // Only resolve _zx_* and zx_* symbols (vDSO exports)
        if !sym_name.starts_with("_zx_") && !sym_name.starts_with("zx_") {
            continue;
        }

        // Look up this symbol in the vDSO
        if let Some(vdso_addr) = lookup_vdso_symbol(
            vmar,
            vdso_base,
            vdso_symtab,
            vdso_strtab,
            vdso_syment,
            vdso_nchain,
            sym_name,
        ) {
            let got_addr = elf_base + r_offset as usize;
            if vmar
                .write_memory(got_addr, &vdso_addr.to_ne_bytes())
                .is_ok()
            {
                resolved += 1;
            }
        }
    }
    info!(
        "resolve_vdso_plt: resolved {} vDSO symbols in PLT",
        resolved
    );
}

/// Look up a symbol by name in the vDSO's .dynsym.
fn lookup_vdso_symbol(
    vmar: &Arc<VmAddressRegion>,
    vdso_base: usize,
    symtab: usize,
    strtab: usize,
    syment: usize,
    nsyms: usize,
    name: &str,
) -> Option<usize> {
    for i in 1..nsyms {
        let sym_off = vdso_base + symtab + i * syment;
        let mut sym = [0u8; 24];
        if vmar.read_memory(sym_off, &mut sym).is_err() {
            continue;
        }
        let st_name = u32::from_le_bytes(sym[0..4].try_into().unwrap()) as usize;
        let st_value = u64::from_le_bytes(sym[8..16].try_into().unwrap()) as usize;

        // Read the name from .dynstr
        let name_addr = vdso_base + strtab + st_name;
        let mut name_buf = [0u8; 64];
        if vmar.read_memory(name_addr, &mut name_buf).is_err() {
            continue;
        }
        let name_end = name_buf.iter().position(|&b| b == 0).unwrap_or(64);
        let sym_name = core::str::from_utf8(&name_buf[..name_end]).unwrap_or("");

        if sym_name == name {
            return Some(vdso_base + st_value);
        }
    }
    None
}
