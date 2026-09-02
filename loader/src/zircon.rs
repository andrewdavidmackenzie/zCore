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

use xmas_elf::ElfFile;

use kernel_hal::context::{TrapReason, UserContext, UserContextField};
use kernel_hal::{MMUFlags, PAGE_SIZE};
use zircon_object::dev::{Resource, ResourceFlags, ResourceKind};
use zircon_object::ipc::{Channel, MessagePacket};
use zircon_object::kcounter;
use zircon_object::object::{Handle, KernelObject, Rights};
use zircon_object::task::{CurrentThread, ExceptionType, Job, Process, Thread, ThreadState};
use zircon_object::util::elf_loader::ElfExt;
use zircon_object::vm::VmObject;

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

// Note: ZBI parsing (userstart_code, extract_program_from_zbi,
// extract_first_file_from_bootfs) was removed. ZBI parsing is now
// done by the userstart userspace program.

fn kcounter_vmos() -> (Arc<VmObject>, Arc<VmObject>) {
    // Provide dummy kcounter VMOs. Real kcounter data requires linker-
    // provided symbols that are only available on x86_64. For petal/userstart
    // testing, dummy VMOs are sufficient.
    use zircon_object::util::kcounter::DescriptorVmoHeader;
    const HEADER_SIZE: usize = core::mem::size_of::<DescriptorVmoHeader>();
    let desc_vmo = VmObject::new_paged(1);
    let arena_vmo = VmObject::new_paged(1);

    let header = DescriptorVmoHeader::default();
    let header_buf: [u8; HEADER_SIZE] = unsafe { core::mem::transmute(header) };
    desc_vmo.write(0, &header_buf).unwrap();
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

    // Load the userstart ELF binary (embedded at compile time)
    let userstart_elf_bytes: &[u8] = include_bytes!(env!("USERSTART_ELF"));
    let elf = ElfFile::new(userstart_elf_bytes).expect("failed to parse userstart ELF");
    let size = elf.load_segment_size();
    // Map the userstart ELF as a single RWX region.
    // (The binary is small enough that all segments fit in one page.)
    let code_pages = size;
    let code_vmo = VmObject::new_paged(code_pages);
    // Write the raw ELF content -- load_from_elf would fail for small binaries
    // where multiple PT_LOAD segments share a page. Instead, copy segment data
    // directly to the correct offsets.
    for ph in elf.program_iter() {
        if ph.get_type().unwrap() != xmas_elf::program::Type::Load {
            continue;
        }
        let offset = ph.virtual_addr() as usize;
        let file_offset = ph.offset() as usize;
        let file_size = ph.file_size() as usize;
        if file_size > 0 {
            code_vmo
                .write(
                    offset,
                    &userstart_elf_bytes[file_offset..file_offset + file_size],
                )
                .unwrap();
        }
    }
    let code_flags = MMUFlags::READ | MMUFlags::WRITE | MMUFlags::EXECUTE | MMUFlags::USER;
    let entry = vmar
        .map(None, code_vmo, 0, code_pages * PAGE_SIZE, code_flags)
        .unwrap()
        + elf.header.pt2.entry_point() as usize;
    info!(
        "userstart: loaded ELF ({} bytes, entry={:#x})",
        userstart_elf_bytes.len(),
        entry
    );

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

    // In Zircon mode, when the root process (userstart) exits, shut down.
    // Only shut down for the "userstart" process, not child processes like "init".
    if thread.is_first_thread() && thread.proc().name() == "userstart" {
        info!("Zircon root process (userstart) exited, shutting down");
        info!("(if QEMU does not exit, press Ctrl-A then X to quit)");
        #[cfg(not(feature = "libos"))]
        kernel_hal::cpu::reset();
    }
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
