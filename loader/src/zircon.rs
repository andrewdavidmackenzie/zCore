//! Run Zircon userspace programs (userstart) and manage trap/interrupt/syscall.
//!
//! This module implements the kernel side of the Zircon boot protocol.
//! `userstart` is zCore's Rust replacement for Fuchsia's `userboot` -- the
//! first userspace process launched by the kernel.
//!
//! The boot sequence:
//! 1. Parse the ZBI (Zircon Boot Image) to find a bootfs filesystem
//! 2. Find the first program in the bootfs
//! 3. Map it into a new process as executable code
//! 4. Start the process with bootstrap handles
//!
//! If no ZBI is provided or it contains no bootfs, a built-in hello
//! program is used instead (writes a debug message and exits).

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{future::Future, pin::Pin};

use kernel_hal::context::{TrapReason, UserContext, UserContextField};
use kernel_hal::{MMUFlags, PAGE_SIZE};
use zircon_abi::zbi::*;
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::ipc::{Channel, MessagePacket};
use zircon_object::kcounter;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::task::{CurrentThread, ExceptionType, Job, Process, Thread, ThreadState};
use zircon_object::vm::{VmObject, VmarFlags};

// Handle indices in the bootstrap channel message.
// These describe userstart itself.
const K_PROC_SELF: usize = 0;
const K_VMARROOT_SELF: usize = 1;
// Essential job and resource handles
const K_ROOTJOB: usize = 2;
const K_ROOTRESOURCE: usize = 3;
// Essential VMO handles
const K_ZBI: usize = 4;
const K_FIRSTVDSO: usize = 5;
const K_CRASHLOG: usize = 8;
const K_COUNTER_NAMES: usize = 9;
const K_COUNTERS: usize = 10;
const K_FISTINSTRUMENTATIONDATA: usize = 11;
const K_HANDLECOUNT: usize = 15;

/// Generate the userstart machine code for the current architecture.
///
/// The generated code:
/// 1. Calls `zx_debug_write(msg, len)` -- syscall 20
/// 2. Calls `zx_process_exit(0)` -- syscall 100
///
/// Returns `(code, msg_offset)` where `msg_offset` is the byte offset
/// of the message string within the code page.
/// Generate the userstart hello program machine code.
///
/// Public so it can be used by `build_test_zbi()` to construct a test ZBI.
pub fn userstart_code() -> Vec<u8> {
    let msg = b"userstart: Hello from zCore Zircon mode!\n";
    let msg_len = msg.len();

    cfg_if! {
        if #[cfg(target_arch = "aarch64")] {
            // AArch64 userstart code:
            //   0x00: adr x0, msg        // x0 = address of message
            //   0x04: mov x1, #msg_len   // x1 = length
            //   0x08: mov x16, #20       // syscall number: SYS_DEBUG_WRITE
            //   0x0c: svc #0             // syscall
            //   0x10: mov x0, #0         // retcode = 0
            //   0x14: mov x16, #100      // syscall number: SYS_PROCESS_EXIT
            //   0x18: svc #0             // syscall
            //   0x1c: b .               // infinite loop (should not reach)
            //   0x20: <message bytes>
            let msg_offset: u32 = 0x20;
            let adr_imm = msg_offset; // ADR uses PC-relative, PC is at 0x00
            let mut code: Vec<u8> = Vec::new();
            // adr x0, #msg_offset  (from PC=0x00, target=0x20, offset=0x20)
            // ADR encoding: immlo=offset[1:0], immhi=offset[20:2]
            let immlo = (adr_imm & 0x3) as u32;
            let immhi = ((adr_imm >> 2) & 0x7ffff) as u32;
            let adr = 0x10000000u32 | (immlo << 29) | (immhi << 5) | 0; // Rd=x0
            code.extend_from_slice(&adr.to_le_bytes());

            // mov x1, #msg_len (MOVZ x1, #msg_len)
            let movz_x1 = 0xd2800001u32 | ((msg_len as u32) << 5);
            code.extend_from_slice(&movz_x1.to_le_bytes());

            // mov x16, #20 (MOVZ x16, #20)
            let movz_x16_20 = 0xd2800010u32 | (20u32 << 5);
            code.extend_from_slice(&movz_x16_20.to_le_bytes());

            // svc #0
            code.extend_from_slice(&0xd4000001u32.to_le_bytes());

            // mov x0, #0 (MOVZ x0, #0)
            code.extend_from_slice(&0xd2800000u32.to_le_bytes());

            // mov x16, #100 (MOVZ x16, #100)
            let movz_x16_100 = 0xd2800010u32 | (100u32 << 5);
            code.extend_from_slice(&movz_x16_100.to_le_bytes());

            // svc #0
            code.extend_from_slice(&0xd4000001u32.to_le_bytes());

            // b . (branch to self -- infinite loop)
            code.extend_from_slice(&0x14000000u32.to_le_bytes());

            // Message data
            code.extend_from_slice(msg);
            code
        } else if #[cfg(target_arch = "x86_64")] {
            // x86_64 userstart code:
            //   lea rdi, [rip + msg]   // arg0 = message pointer
            //   mov rsi, msg_len       // arg1 = length
            //   mov eax, 20            // syscall number: SYS_DEBUG_WRITE
            //   syscall
            //   xor edi, edi           // retcode = 0
            //   mov eax, 100           // syscall number: SYS_PROCESS_EXIT
            //   syscall
            //   jmp .                  // infinite loop
            //   <message bytes>
            let mut code: Vec<u8> = Vec::new();

            // We'll compute the RIP-relative offset to msg after we know
            // the code size. For now, build the instructions:

            // lea rdi, [rip + offset]  -- 7 bytes: 48 8d 3d XX XX XX XX
            code.extend_from_slice(&[0x48, 0x8d, 0x3d, 0x00, 0x00, 0x00, 0x00]);
            // mov rsi, msg_len -- 7 bytes: 48 c7 c6 XX XX XX XX
            code.extend_from_slice(&[0x48, 0xc7, 0xc6]);
            code.extend_from_slice(&(msg_len as u32).to_le_bytes());
            // mov eax, 20 -- 5 bytes: b8 14 00 00 00
            code.extend_from_slice(&[0xb8, 0x14, 0x00, 0x00, 0x00]);
            // syscall -- 2 bytes: 0f 05
            code.extend_from_slice(&[0x0f, 0x05]);
            // xor edi, edi -- 2 bytes: 31 ff
            code.extend_from_slice(&[0x31, 0xff]);
            // mov eax, 100 -- 5 bytes: b8 64 00 00 00
            code.extend_from_slice(&[0xb8, 0x64, 0x00, 0x00, 0x00]);
            // syscall -- 2 bytes: 0f 05
            code.extend_from_slice(&[0x0f, 0x05]);
            // jmp . -- 2 bytes: eb fe
            code.extend_from_slice(&[0xeb, 0xfe]);

            // Now patch the RIP-relative offset in the LEA instruction.
            // LEA is at offset 0, RIP after LEA = 7, msg is at current code.len()
            let msg_rip_offset = (code.len() as i32) - 7;
            code[3..7].copy_from_slice(&msg_rip_offset.to_le_bytes());

            // Message data
            code.extend_from_slice(msg);
            code
        } else {
            compile_error!("Unsupported architecture for Zircon mode userstart");
        }
    }
}

/// Try to extract the first program from a ZBI's bootfs.
///
/// Parses the ZBI container, finds the first `ZBI_TYPE_STORAGE_BOOTFS` item,
/// then finds the first file in the bootfs directory. Returns the file data
/// as a `Vec<u8>`, or `None` if parsing fails or no bootfs is found.
fn extract_program_from_zbi(zbi_data: &[u8]) -> Option<Vec<u8>> {
    if zbi_data.len() < ZbiHeader::SIZE {
        return None;
    }

    // Parse container header
    let container: &ZbiHeader = unsafe { &*(zbi_data.as_ptr() as *const ZbiHeader) };
    if container.item_type != ZBI_TYPE_CONTAINER || container.magic != ZBI_ITEM_MAGIC {
        info!("ZBI: invalid container header");
        return None;
    }
    if container.extra != ZBI_CONTAINER_MAGIC {
        info!("ZBI: invalid container magic");
        return None;
    }

    // Iterate items inside the container
    let container_end = ZbiHeader::SIZE + container.length as usize;
    let mut offset = ZbiHeader::SIZE; // skip container header

    while offset + ZbiHeader::SIZE <= container_end {
        let item: &ZbiHeader = unsafe { &*(zbi_data.as_ptr().add(offset) as *const ZbiHeader) };
        if item.magic != ZBI_ITEM_MAGIC {
            info!("ZBI: invalid item magic at offset {:#x}", offset);
            break;
        }

        let payload_start = offset + ZbiHeader::SIZE;
        let payload_end = payload_start + item.length as usize;

        if item.item_type == ZBI_TYPE_STORAGE_BOOTFS {
            // Found bootfs -- parse it
            if payload_end > zbi_data.len() {
                info!("ZBI: bootfs payload extends beyond ZBI data");
                return None;
            }
            let bootfs_data = &zbi_data[payload_start..payload_end];
            return extract_first_file_from_bootfs(bootfs_data);
        }

        // Advance to next item (payload + padding to 8-byte alignment)
        offset = payload_start + item.padded_length();
    }

    info!("ZBI: no bootfs item found");
    None
}

/// Extract the first file from a bootfs image.
fn extract_first_file_from_bootfs(bootfs: &[u8]) -> Option<Vec<u8>> {
    let bfs_hdr_size = core::mem::size_of::<ZbiBootfsHeader>();
    if bootfs.len() < bfs_hdr_size {
        return None;
    }

    let header: &ZbiBootfsHeader = unsafe { &*(bootfs.as_ptr() as *const ZbiBootfsHeader) };
    if header.magic != ZBI_BOOTFS_MAGIC {
        info!("bootfs: invalid magic");
        return None;
    }

    // Read the first directory entry
    let dirent_start = bfs_hdr_size;
    if dirent_start + ZbiBootfsDirent::FIXED_SIZE > bootfs.len() {
        info!("bootfs: directory too small");
        return None;
    }

    let dirent: &ZbiBootfsDirent =
        unsafe { &*(bootfs.as_ptr().add(dirent_start) as *const ZbiBootfsDirent) };

    // Extract the filename for logging
    let name_start = dirent_start + ZbiBootfsDirent::FIXED_SIZE;
    let name_end = name_start + dirent.name_len as usize;
    if name_end > bootfs.len() {
        return None;
    }
    let name_bytes = &bootfs[name_start..name_end - 1]; // exclude NUL
    if let Ok(name) = core::str::from_utf8(name_bytes) {
        info!("bootfs: loading '{}' ({} bytes)", name, dirent.data_len);
    }

    // Extract the file data
    let data_start = dirent.data_off as usize;
    let data_end = data_start + dirent.data_len as usize;
    if data_end > bootfs.len() {
        info!("bootfs: file data extends beyond bootfs");
        return None;
    }

    Some(bootfs[data_start..data_end].to_vec())
}

fn kcounter_vmos() -> (Arc<VmObject>, Arc<VmObject>) {
    let (desc_vmo, arena_vmo) = if cfg!(feature = "libos") {
        // dummy VMOs
        use zircon_object::util::kcounter::DescriptorVmoHeader;
        const HEADER_SIZE: usize = core::mem::size_of::<DescriptorVmoHeader>();
        let desc_vmo = VmObject::new_paged(1);
        let arena_vmo = VmObject::new_paged(1);

        let header = DescriptorVmoHeader::default();
        let header_buf: [u8; HEADER_SIZE] = unsafe { core::mem::transmute(header) };
        desc_vmo.write(0, &header_buf).unwrap();
        (desc_vmo, arena_vmo)
    } else {
        use kernel_hal::vm::{GenericPageTable, PageTable};
        use zircon_object::{util::kcounter::AllCounters, vm::pages};
        let pgtable = PageTable::from_current();

        // kcounters names table.
        let desc_vmo_data = AllCounters::raw_desc_vmo_data();
        let paddr = pgtable.query(desc_vmo_data.as_ptr() as usize).unwrap().0;
        let desc_vmo = VmObject::new_physical(paddr, pages(desc_vmo_data.len()));

        // kcounters live data.
        let arena_vmo_data = AllCounters::raw_arena_vmo_data();
        let paddr = pgtable.query(arena_vmo_data.as_ptr() as usize).unwrap().0;
        let arena_vmo = VmObject::new_physical(paddr, pages(arena_vmo_data.len()));
        (desc_vmo, arena_vmo)
    };
    desc_vmo.set_name("counters/desc");
    arena_vmo.set_name("counters/arena");
    (desc_vmo, arena_vmo)
}

/// Run Zircon `userstart` process and load the ZBI file as the bootfs.
///
/// `userstart` is zCore's Rust replacement for Fuchsia's `userboot`. Instead
/// of loading prebuilt Fuchsia binaries, it generates a minimal userspace
/// program directly in memory that:
/// 1. Writes a debug message via `zx_debug_write`
/// 2. Exits via `zx_process_exit(0)`
///
/// This function is also available as `run_userboot()` for backward compatibility.
pub fn run_userstart(zbi: impl AsRef<[u8]>, cmdline: &str) -> Arc<Process> {
    let job = Job::root();
    let proc = Process::create(&job, "userstart").unwrap();
    let thread = Thread::create(&proc, "userstart").unwrap();
    let resource = Resource::create(
        "root",
        ResourceKind::ROOT,
        0,
        0x1_0000_0000,
        ResourceFlags::empty(),
    );
    let vmar = proc.vmar();

    // Try to load a program from the ZBI bootfs; fall back to built-in hello
    let code = match extract_program_from_zbi(zbi.as_ref()) {
        Some(program_data) => {
            info!(
                "userstart: loaded program from ZBI bootfs ({} bytes)",
                program_data.len()
            );
            program_data
        }
        None => {
            info!("userstart: no bootfs program found, using built-in hello");
            userstart_code()
        }
    };
    let code_pages = code.len() / PAGE_SIZE + 1;
    let code_vmo = VmObject::new_paged(code_pages);
    code_vmo.write(0, &code).unwrap();
    let code_flags = MMUFlags::READ | MMUFlags::EXECUTE | MMUFlags::USER;
    let entry = vmar
        .map(None, code_vmo, 0, code_pages * PAGE_SIZE, code_flags)
        .unwrap();

    // Create a stub vDSO VMO (petal programs use inline syscalls from
    // zircon-abi instead of a shared library, so this is just a placeholder
    // to satisfy the handle protocol).
    let vdso_vmo = VmObject::new_paged(1);
    vdso_vmo.set_name("vdso/full");

    // zbi
    let zbi_vmo = {
        let vmo = VmObject::new_paged(zbi.as_ref().len() / PAGE_SIZE + 1);
        vmo.write(0, zbi.as_ref()).unwrap();
        vmo.set_name("zbi");
        vmo
    };

    // stack
    const STACK_PAGES: usize = 8;
    let stack_vmo = VmObject::new_paged(STACK_PAGES);
    let flags = MMUFlags::READ | MMUFlags::WRITE | MMUFlags::USER;
    let stack_bottom = vmar
        .map(None, stack_vmo.clone(), 0, stack_vmo.len(), flags)
        .unwrap();
    let sp = if cfg!(target_arch = "x86_64") {
        // WARN: align stack to 16B, then emulate a 'call' (push rip)
        stack_bottom + stack_vmo.len() - 8
    } else {
        stack_bottom + stack_vmo.len()
    };

    // channel
    let (user_channel, kernel_channel) = Channel::create();
    let handle = Handle::new(user_channel, Rights::DEFAULT_CHANNEL);

    let mut handles = alloc::vec![Handle::new(proc.clone(), Rights::empty()); K_HANDLECOUNT];
    handles[K_PROC_SELF] = Handle::new(proc.clone(), Rights::DEFAULT_PROCESS);
    handles[K_VMARROOT_SELF] = Handle::new(proc.vmar(), Rights::DEFAULT_VMAR | Rights::IO);
    handles[K_ROOTJOB] = Handle::new(job, Rights::DEFAULT_JOB);
    handles[K_ROOTRESOURCE] = Handle::new(resource, Rights::DEFAULT_RESOURCE);
    handles[K_ZBI] = Handle::new(zbi_vmo, Rights::DEFAULT_VMO);

    // vDSO handles (stub VMOs for now)
    let vdso_test1 = vdso_vmo.create_child(false, 0, vdso_vmo.len()).unwrap();
    vdso_test1.set_name("vdso/test1");
    let vdso_test2 = vdso_vmo.create_child(false, 0, vdso_vmo.len()).unwrap();
    vdso_test2.set_name("vdso/test2");
    handles[K_FIRSTVDSO] = Handle::new(vdso_vmo, Rights::DEFAULT_VMO | Rights::EXECUTE);
    handles[K_FIRSTVDSO + 1] = Handle::new(vdso_test1, Rights::DEFAULT_VMO | Rights::EXECUTE);
    handles[K_FIRSTVDSO + 2] = Handle::new(vdso_test2, Rights::DEFAULT_VMO | Rights::EXECUTE);

    let crash_log_vmo = VmObject::new_paged(1);
    crash_log_vmo.set_name("crashlog");
    handles[K_CRASHLOG] = Handle::new(crash_log_vmo, Rights::DEFAULT_VMO);

    // kcounter
    let (desc_vmo, arena_vmo) = kcounter_vmos();
    handles[K_COUNTER_NAMES] = Handle::new(desc_vmo, Rights::DEFAULT_VMO);
    handles[K_COUNTERS] = Handle::new(arena_vmo, Rights::DEFAULT_VMO);

    let instrumentation_data_vmo = VmObject::new_paged(0);
    instrumentation_data_vmo.set_name("UNIMPLEMENTED_VMO");
    handles[K_FISTINSTRUMENTATIONDATA] =
        Handle::new(instrumentation_data_vmo.clone(), Rights::DEFAULT_VMO);
    handles[K_FISTINSTRUMENTATIONDATA + 1] =
        Handle::new(instrumentation_data_vmo.clone(), Rights::DEFAULT_VMO);
    handles[K_FISTINSTRUMENTATIONDATA + 2] =
        Handle::new(instrumentation_data_vmo.clone(), Rights::DEFAULT_VMO);
    handles[K_FISTINSTRUMENTATIONDATA + 3] =
        Handle::new(instrumentation_data_vmo, Rights::DEFAULT_VMO);

    let data = Vec::from(cmdline.replace(':', "\0") + "\0");
    let msg = MessagePacket { data, handles };
    kernel_channel.write(msg).unwrap();

    proc.start(&thread, entry, sp, Some(handle), 0, thread_fn)
        .expect("failed to start main thread");
    proc
}

/// Backward-compatible alias for [`run_userstart`].
pub fn run_userboot(zbi: impl AsRef<[u8]>, cmdline: &str) -> Arc<Process> {
    run_userstart(zbi, cmdline)
}

kcounter!(EXCEPTIONS_USER, "exceptions.user");
kcounter!(EXCEPTIONS_IRQ, "exceptions.irq");
kcounter!(EXCEPTIONS_PGFAULT, "exceptions.pgfault");

fn thread_fn(thread: CurrentThread) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(run_user(thread))
}

async fn run_user(thread: CurrentThread) {
    kernel_hal::thread::set_current_thread(Some(thread.inner()));
    if thread.is_first_thread() {
        thread
            .handle_exception(ExceptionType::ProcessStarting)
            .await;
    };
    thread.handle_exception(ExceptionType::ThreadStarting).await;

    loop {
        // wait
        let mut ctx = thread.wait_for_run().await;
        if thread.state() == ThreadState::Dying {
            break;
        }

        // run
        trace!("go to user: {:#x?}", ctx);
        debug!("switch to {}|{}", thread.proc().name(), thread.name());
        let tmp_time = kernel_hal::timer::timer_now().as_nanos();

        // * Attention
        // The code will enter a magic zone from here.
        // `enter_uspace` will be executed into a wrapped library where context switching takes place.
        // The details are available in the `trapframe` crate on crates.io.
        ctx.enter_uspace();

        // Back from the userspace
        let time = kernel_hal::timer::timer_now().as_nanos() - tmp_time;
        thread.time_add(time);
        trace!("back from user: {:#x?}", ctx);
        EXCEPTIONS_USER.add(1);

        // handle trap/interrupt/syscall
        if let Err(e) = handler_user_trap(&thread, ctx).await {
            if let ExceptionType::ThreadExiting = e {
                break;
            }
            thread.handle_exception(e).await;
        }
    }
    thread.handle_exception(ExceptionType::ThreadExiting).await;
}

async fn handler_user_trap(
    thread: &CurrentThread,
    mut ctx: Box<UserContext>,
) -> Result<(), ExceptionType> {
    let reason = ctx.trap_reason();

    if let TrapReason::Syscall = reason {
        let num = syscall_num(&ctx);
        let args = syscall_args(&ctx);
        ctx.advance_pc(reason);
        thread.put_context(ctx);
        let mut syscall = zircon_syscall::Syscall { thread, thread_fn };
        let ret = syscall.syscall(num as u32, args).await as usize;
        thread
            .with_context(|ctx| ctx.set_field(UserContextField::ReturnValue, ret))
            .map_err(|_| ExceptionType::ThreadExiting)?;
        return Ok(());
    }

    thread.put_context(ctx);
    match reason {
        TrapReason::Interrupt(vector) => {
            EXCEPTIONS_IRQ.add(1); // FIXME
            kernel_hal::interrupt::handle_irq(vector);
            kernel_hal::thread::yield_now().await;
            Ok(())
        }
        TrapReason::PageFault(vaddr, flags) => {
            EXCEPTIONS_PGFAULT.add(1);
            info!("page fault from user mode @ {:#x}({:?})", vaddr, flags);
            let vmar = thread.proc().vmar();
            vmar.handle_page_fault(vaddr, flags).map_err(|err| {
                error!(
                    "failed to handle page fault from user mode @ {:#x}({:?}): {:?}\n{:#x?}",
                    vaddr,
                    flags,
                    err,
                    thread.context_cloned()
                );
                ExceptionType::FatalPageFault
            })
        }
        TrapReason::UndefinedInstruction => Err(ExceptionType::UndefinedInstruction),
        TrapReason::SoftwareBreakpoint => Err(ExceptionType::SoftwareBreakpoint),
        TrapReason::HardwareBreakpoint => Err(ExceptionType::HardwareBreakpoint),
        TrapReason::UnalignedAccess => Err(ExceptionType::UnalignedAccess),
        TrapReason::GernelFault(_) => Err(ExceptionType::General),
        _ => unreachable!(),
    }
}

fn syscall_num(ctx: &UserContext) -> usize {
    let regs = ctx.general();
    cfg_if! {
        if #[cfg(target_arch = "x86_64")] {
            regs.rax
        } else if #[cfg(target_arch = "aarch64")] {
            regs.x16
        } else if #[cfg(target_arch = "riscv64")] {
            regs.a7
        } else {
            unimplemented!()
        }
    }
}

fn syscall_args(ctx: &UserContext) -> [usize; 8] {
    let regs = ctx.general();
    cfg_if! {
        if #[cfg(target_arch = "x86_64")] {
            if cfg!(feature = "libos") {
                let arg7 = unsafe{ (regs.rsp as *const usize).read() };
                let arg8 = unsafe{ (regs.rsp as *const usize).add(1).read() };
                [regs.rdi, regs.rsi, regs.rdx, regs.rcx, regs.r8, regs.r9, arg7, arg8]
            } else {
                [regs.rdi, regs.rsi, regs.rdx, regs.r10, regs.r8, regs.r9, regs.r12, regs.r13]
            }
        } else if #[cfg(target_arch = "aarch64")] {
            [regs.x0, regs.x1, regs.x2, regs.x3, regs.x4, regs.x5, regs.x6, regs.x7]
        } else if #[cfg(target_arch = "riscv64")] {
            [regs.a0, regs.a1, regs.a2, regs.a3, regs.a4, regs.a5, regs.a6, regs.a7]
        } else {
            unimplemented!()
        }
    }
}
