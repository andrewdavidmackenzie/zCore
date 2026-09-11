//! Inline syscall wrappers for Zircon userspace programs.
//!
//! These use hardware trap instructions to enter the kernel:
//! - `svc #0` on aarch64 (syscall number in x16)
//! - `syscall` on x86_64 (syscall number in rax)
//! - `ecall` on riscv64 (syscall number in a7)
//!
//! # Safety
//! All functions are unsafe because they perform raw syscalls with
//! unchecked arguments.

use crate::errors::ZxStatus;

/// Raw syscall with 0 arguments.
#[inline(always)]
pub unsafe fn syscall0(num: u32) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 1 argument.
#[inline(always)]
pub unsafe fn syscall1(num: u32, a0: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 2 arguments.
#[inline(always)]
pub unsafe fn syscall2(num: u32, a0: u64, a1: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0,
        in("x1") a1,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0,
        in("rsi") a1,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0,
        in("a1") a1,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 3 arguments.
#[inline(always)]
pub unsafe fn syscall3(num: u32, a0: u64, a1: u64, a2: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0,
        in("x1") a1,
        in("x2") a2,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0,
        in("rsi") a1,
        in("rdx") a2,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0,
        in("a1") a1,
        in("a2") a2,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 4 arguments.
///
/// Note: on x86_64, the 4th argument uses `r10` (not `rcx`) because the
/// `syscall` instruction clobbers `rcx` (stores RIP). This matches the
/// real Zircon/Linux syscall ABI. These wrappers are for bare-metal use
/// only (behind the `userspace` feature), not for libos mode.
#[inline(always)]
pub unsafe fn syscall4(num: u32, a0: u64, a1: u64, a2: u64, a3: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0,
        in("x1") a1,
        in("x2") a2,
        in("x3") a3,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0,
        in("rsi") a1,
        in("rdx") a2,
        in("r10") a3,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0,
        in("a1") a1,
        in("a2") a2,
        in("a3") a3,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 5 arguments.
#[inline(always)]
pub unsafe fn syscall5(num: u32, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0, in("x1") a1, in("x2") a2, in("x3") a3, in("x4") a4,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0, in("rsi") a1, in("rdx") a2, in("r10") a3, in("r8") a4,
        lateout("rax") ret,
        out("rcx") _, out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0, in("a1") a1, in("a2") a2, in("a3") a3, in("a4") a4,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 6 arguments.
#[inline(always)]
pub unsafe fn syscall6(num: u32, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0, in("x1") a1, in("x2") a2, in("x3") a3, in("x4") a4, in("x5") a5,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "syscall",
        in("eax") num,
        in("rdi") a0, in("rsi") a1, in("rdx") a2, in("r10") a3, in("r8") a4, in("r9") a5,
        lateout("rax") ret,
        out("rcx") _, out("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0, in("a1") a1, in("a2") a2, in("a3") a3, in("a4") a4, in("a5") a5,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 7 arguments.
///
/// On x86_64 the 7th argument is passed on the stack (the `syscall`
/// instruction only provides 6 register arguments). On aarch64 and
/// riscv64, registers x6/a6 are used.
#[inline(always)]
pub unsafe fn syscall7(
    num: u32,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    a6: u64,
) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0, in("x1") a1, in("x2") a2, in("x3") a3,
        in("x4") a4, in("x5") a5, in("x6") a6,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    {
        // x86_64 syscall ABI only has 6 register args. The 7th is on the stack.
        // Push it, syscall, pop it.
        core::arch::asm!(
            "push {arg6}",
            "syscall",
            "pop {arg6}",
            arg6 = in(reg) a6,
            in("eax") num,
            in("rdi") a0, in("rsi") a1, in("rdx") a2,
            in("r10") a3, in("r8") a4, in("r9") a5,
            lateout("rax") ret,
            out("rcx") _, out("r11") _,
        );
    }
    #[cfg(target_arch = "riscv64")]
    core::arch::asm!(
        "ecall",
        in("a7") num as u64,
        in("a0") a0, in("a1") a1, in("a2") a2, in("a3") a3,
        in("a4") a4, in("a5") a5, in("a6") a6,
        lateout("a0") ret,
        options(nostack),
    );
    ret as ZxStatus
}

/// Raw syscall with 8 arguments.
///
/// On x86_64 args 7 and 8 are passed on the stack.
/// On aarch64 and riscv64, registers x7/a7 are used (note: a7 is also
/// the syscall number register on riscv64, loaded before the args).
#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub unsafe fn syscall8(
    num: u32,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    a6: u64,
    a7: u64,
) -> ZxStatus {
    let ret: i64;
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!(
        "svc #0",
        in("x16") num as u64,
        in("x0") a0, in("x1") a1, in("x2") a2, in("x3") a3,
        in("x4") a4, in("x5") a5, in("x6") a6, in("x7") a7,
        lateout("x0") ret,
        options(nostack),
    );
    #[cfg(target_arch = "x86_64")]
    {
        // x86_64: args 7 and 8 on the stack
        core::arch::asm!(
            "push {arg7}",
            "push {arg6}",
            "syscall",
            "pop {arg6}",
            "pop {arg7}",
            arg6 = in(reg) a6,
            arg7 = in(reg) a7,
            in("eax") num,
            in("rdi") a0, in("rsi") a1, in("rdx") a2,
            in("r10") a3, in("r8") a4, in("r9") a5,
            lateout("rax") ret,
            out("rcx") _, out("r11") _,
        );
    }
    #[cfg(target_arch = "riscv64")]
    {
        // riscv64: a7 is both the syscall number and the 8th argument register.
        // Load the syscall number first, then the 8th arg overwrites a7 --
        // but ecall reads the syscall number from a7 at the point of the trap.
        // To avoid the conflict, we store the syscall number in a7 last.
        // Actually, on riscv64 the Zircon ABI uses a7 for syscall number
        // and only supports 7 register arguments (a0-a6).
        // 8-arg syscalls on riscv64 would need a stack-based convention.
        // For now, this is a compile error -- no 8-arg riscv64 syscalls needed yet.
        compile_error!("8-argument syscalls not yet supported on riscv64");
    }
    ret as ZxStatus
}

// --- Typed wrappers for common syscalls ---

/// Write a debug message to the kernel log.
///
/// # Safety
/// `buf` must point to `len` valid bytes.
pub unsafe fn zx_debug_write(buf: *const u8, len: usize) -> ZxStatus {
    syscall2(crate::consts::SYS_DEBUG_WRITE, buf as u64, len as u64)
}

/// Write a debug message to the kernel log (safe wrapper, bytes).
pub fn debug_write(msg: &[u8]) -> ZxStatus {
    unsafe { zx_debug_write(msg.as_ptr(), msg.len()) }
}

/// Write a debug message to the kernel log (safe wrapper, str).
pub fn debug_print(msg: &str) -> ZxStatus {
    debug_write(msg.as_bytes())
}

/// Exit the current process (safe wrapper).
pub fn process_exit(retcode: i64) -> ! {
    unsafe {
        syscall1(crate::consts::SYS_PROCESS_EXIT, retcode as u64);
        core::hint::unreachable_unchecked()
    }
}

/// Exit the current process (raw unsafe version).
pub unsafe fn zx_process_exit(retcode: i64) -> ! {
    syscall1(crate::consts::SYS_PROCESS_EXIT, retcode as u64);
    core::hint::unreachable_unchecked()
}

/// Exit the current thread.
pub unsafe fn zx_thread_exit() -> ! {
    syscall0(crate::consts::SYS_THREAD_EXIT);
    core::hint::unreachable_unchecked()
}

// --- Handle type alias ---

/// Zircon handle value (matches `zx_handle_t`).
pub type HandleValue = u32;

/// Invalid handle sentinel.
pub const ZX_HANDLE_INVALID: HandleValue = 0;

// --- Channel syscalls ---

/// Read a message from a channel.
///
/// # Safety
/// All pointers must be valid for the specified sizes.
#[allow(clippy::too_many_arguments)]
pub unsafe fn zx_channel_read(
    handle: HandleValue,
    options: u32,
    bytes: *mut u8,
    handles: *mut HandleValue,
    num_bytes: u32,
    num_handles: u32,
    actual_bytes: *mut u32,
    actual_handles: *mut u32,
) -> ZxStatus {
    syscall8(
        crate::consts::SYS_CHANNEL_READ,
        handle as u64,
        options as u64,
        bytes as u64,
        handles as u64,
        num_bytes as u64,
        num_handles as u64,
        actual_bytes as u64,
        actual_handles as u64,
    )
}

/// Create a channel pair.
///
/// # Safety
/// `out0` and `out1` must be valid pointers.
pub unsafe fn zx_channel_create(
    options: u32,
    out0: *mut HandleValue,
    out1: *mut HandleValue,
) -> ZxStatus {
    syscall3(
        crate::consts::SYS_CHANNEL_CREATE,
        options as u64,
        out0 as u64,
        out1 as u64,
    )
}

/// Write a message to a channel.
///
/// # Safety
/// All pointers must be valid for the specified sizes.
pub unsafe fn zx_channel_write(
    handle: HandleValue,
    options: u32,
    bytes: *const u8,
    num_bytes: u32,
    handles: *const HandleValue,
    num_handles: u32,
) -> ZxStatus {
    syscall6(
        crate::consts::SYS_CHANNEL_WRITE,
        handle as u64,
        options as u64,
        bytes as u64,
        num_bytes as u64,
        handles as u64,
        num_handles as u64,
    )
}

/// Read a message from a channel (extended version with handle info).
///
/// # Safety
/// All pointers must be valid for the specified sizes.
pub unsafe fn zx_channel_read_etc(
    handle: HandleValue,
    options: u32,
    bytes: *mut u8,
    handles: *mut crate::types::HandleInfo,
    num_bytes: u32,
    num_handles: u32,
    actual_bytes: *mut u32,
    actual_handles: *mut u32,
) -> ZxStatus {
    syscall8(
        crate::consts::SYS_CHANNEL_READ_ETC,
        handle as u64,
        options as u64,
        bytes as u64,
        handles as u64,
        num_bytes as u64,
        num_handles as u64,
        actual_bytes as u64,
        actual_handles as u64,
    )
}

/// Write a message to a channel (extended version with handle dispositions).
///
/// # Safety
/// All pointers must be valid for the specified sizes.
pub unsafe fn zx_channel_write_etc(
    handle: HandleValue,
    options: u32,
    bytes: *const u8,
    num_bytes: u32,
    handles: *mut crate::types::HandleDisposition,
    num_handles: u32,
) -> ZxStatus {
    syscall6(
        crate::consts::SYS_CHANNEL_WRITE_ETC,
        handle as u64,
        options as u64,
        bytes as u64,
        num_bytes as u64,
        handles as u64,
        num_handles as u64,
    )
}

/// Send a message to a channel and wait for a reply (no-retry step).
///
/// # Safety
/// `args` must point to a valid `ChannelCallArgs`. Output pointers must be valid.
///
/// Note: the kernel dispatcher currently only passes 6 arguments, so
/// `read_status` is not yet propagated to the handler.
// TODO: propagate read_status through the dispatcher and handler
pub unsafe fn zx_channel_call_noretry(
    handle: HandleValue,
    options: u32,
    deadline: i64,
    args: *const crate::types::ChannelCallArgs,
    actual_bytes: *mut u32,
    actual_handles: *mut u32,
    _read_status: *mut ZxStatus,
) -> ZxStatus {
    // The kernel handler only accepts 6 args; read_status is not yet
    // forwarded. Pass the first 6 via syscall6.
    syscall6(
        crate::consts::SYS_CHANNEL_CALL_NORETRY,
        handle as u64,
        options as u64,
        deadline as u64,
        args as u64,
        actual_bytes as u64,
        actual_handles as u64,
    )
}

// --- Process/Thread syscalls ---

/// Create a new process.
///
/// # Safety
/// `name` must point to `name_size` valid bytes. Output pointers must be valid.
pub unsafe fn zx_process_create(
    job: HandleValue,
    name: *const u8,
    name_size: usize,
    options: u32,
    proc_handle: *mut HandleValue,
    vmar_handle: *mut HandleValue,
) -> ZxStatus {
    syscall6(
        crate::consts::SYS_PROCESS_CREATE,
        job as u64,
        name as u64,
        name_size as u64,
        options as u64,
        proc_handle as u64,
        vmar_handle as u64,
    )
}

/// Create a new thread.
///
/// # Safety
/// `name` must point to `name_size` valid bytes. Output pointer must be valid.
pub unsafe fn zx_thread_create(
    proc_handle: HandleValue,
    name: *const u8,
    name_size: usize,
    options: u32,
    thread_handle: *mut HandleValue,
) -> ZxStatus {
    syscall5(
        crate::consts::SYS_THREAD_CREATE,
        proc_handle as u64,
        name as u64,
        name_size as u64,
        options as u64,
        thread_handle as u64,
    )
}

/// Start a process's first thread.
///
/// # Safety
/// Handles must be valid.
pub unsafe fn zx_process_start(
    proc_handle: HandleValue,
    thread_handle: HandleValue,
    entry: usize,
    stack: usize,
    arg1_handle: HandleValue,
    arg2: usize,
) -> ZxStatus {
    syscall6(
        crate::consts::SYS_PROCESS_START,
        proc_handle as u64,
        thread_handle as u64,
        entry as u64,
        stack as u64,
        arg1_handle as u64,
        arg2 as u64,
    )
}

// --- VMO syscalls ---

/// Create a VMO.
///
/// # Safety
/// `out` must be a valid pointer.
pub unsafe fn zx_vmo_create(size: u64, options: u32, out: *mut HandleValue) -> ZxStatus {
    syscall3(
        crate::consts::SYS_VMO_CREATE,
        size,
        options as u64,
        out as u64,
    )
}

/// Read from a VMO.
///
/// # Safety
/// `buf` must be valid for `buf_size` bytes.
pub unsafe fn zx_vmo_read(
    handle: HandleValue,
    buf: *mut u8,
    offset: u64,
    buf_size: usize,
) -> ZxStatus {
    syscall4(
        crate::consts::SYS_VMO_READ,
        handle as u64,
        buf as u64,
        offset,
        buf_size as u64,
    )
}

/// Write to a VMO.
///
/// # Safety
/// `buf` must be valid for `buf_size` bytes.
pub unsafe fn zx_vmo_write(
    handle: HandleValue,
    buf: *const u8,
    offset: u64,
    buf_size: usize,
) -> ZxStatus {
    syscall4(
        crate::consts::SYS_VMO_WRITE,
        handle as u64,
        buf as u64,
        offset,
        buf_size as u64,
    )
}

/// Get the size of a VMO.
///
/// # Safety
/// `size` must be a valid pointer.
pub unsafe fn zx_vmo_get_size(handle: HandleValue, size: *mut usize) -> ZxStatus {
    syscall2(crate::consts::SYS_VMO_GET_SIZE, handle as u64, size as u64)
}

/// Replace a VMO handle with one that has execute rights.
///
/// # Safety
/// `out` must be a valid pointer.
pub unsafe fn zx_vmo_replace_as_executable(
    handle: HandleValue,
    vmex_resource: HandleValue,
    out: *mut HandleValue,
) -> ZxStatus {
    syscall3(
        crate::consts::SYS_VMO_REPLACE_AS_EXECUTABLE,
        handle as u64,
        vmex_resource as u64,
        out as u64,
    )
}

// --- VMAR syscalls ---

/// Map a VMO into a VMAR.
///
/// # Safety
/// Handles must be valid. `mapped_addr` must be a valid pointer.
pub unsafe fn zx_vmar_map(
    vmar_handle: HandleValue,
    options: u32,
    vmar_offset: usize,
    vmo_handle: HandleValue,
    vmo_offset: usize,
    len: usize,
    mapped_addr: *mut usize,
) -> ZxStatus {
    syscall7(
        crate::consts::SYS_VMAR_MAP,
        vmar_handle as u64,
        options as u64,
        vmar_offset as u64,
        vmo_handle as u64,
        vmo_offset as u64,
        len as u64,
        mapped_addr as u64,
    )
}

// --- Object syscalls ---

/// Wait for a signal on an object.
///
/// # Safety
/// `observed` must be a valid pointer if non-null.
pub unsafe fn zx_object_wait_one(
    handle: HandleValue,
    signals: u32,
    deadline: i64,
    observed: *mut u32,
) -> ZxStatus {
    syscall4(
        crate::consts::SYS_OBJECT_WAIT_ONE,
        handle as u64,
        signals as u64,
        deadline as u64,
        observed as u64,
    )
}

/// Wait for signals on multiple objects.
///
/// # Safety
/// `items` must point to `num_items` valid `zx_wait_item_t` structs.
pub unsafe fn zx_object_wait_many(
    items: *mut crate::types::WaitItem,
    num_items: usize,
    deadline: i64,
) -> ZxStatus {
    syscall3(
        crate::consts::SYS_OBJECT_WAIT_MANY,
        items as u64,
        num_items as u64,
        deadline as u64,
    )
}

/// Subscribe for signals on an object, delivering via a port.
pub unsafe fn zx_object_wait_async(
    handle: HandleValue,
    port: HandleValue,
    key: u64,
    signals: u32,
    options: u32,
) -> ZxStatus {
    syscall5(
        crate::consts::SYS_OBJECT_WAIT_ASYNC,
        handle as u64,
        port as u64,
        key,
        signals as u64,
        options as u64,
    )
}

/// Get information about an object.
///
/// # Safety
/// `buffer` must point to `buffer_size` valid bytes. Output pointers must be valid.
pub unsafe fn zx_object_get_info(
    handle: HandleValue,
    topic: u32,
    buffer: *mut u8,
    buffer_size: usize,
    actual: *mut usize,
    avail: *mut usize,
) -> ZxStatus {
    syscall6(
        crate::consts::SYS_OBJECT_GET_INFO,
        handle as u64,
        topic as u64,
        buffer as u64,
        buffer_size as u64,
        actual as u64,
        avail as u64,
    )
}

/// Get a property of an object.
///
/// # Safety
/// `value` must point to `value_size` valid bytes.
pub unsafe fn zx_object_get_property(
    handle: HandleValue,
    property: u32,
    value: *mut u8,
    value_size: usize,
) -> ZxStatus {
    syscall4(
        crate::consts::SYS_OBJECT_GET_PROPERTY,
        handle as u64,
        property as u64,
        value as u64,
        value_size as u64,
    )
}

/// Set a property of an object.
///
/// # Safety
/// `value` must point to `value_size` valid bytes.
pub unsafe fn zx_object_set_property(
    handle: HandleValue,
    property: u32,
    value: *const u8,
    value_size: usize,
) -> ZxStatus {
    syscall4(
        crate::consts::SYS_OBJECT_SET_PROPERTY,
        handle as u64,
        property as u64,
        value as u64,
        value_size as u64,
    )
}

// --- Handle syscalls ---

/// Close a handle.
pub unsafe fn zx_handle_close(handle: HandleValue) -> ZxStatus {
    syscall1(crate::consts::SYS_HANDLE_CLOSE, handle as u64)
}

/// Close multiple handles.
///
/// # Safety
/// `handles` must point to `num_handles` valid `HandleValue`s.
pub unsafe fn zx_handle_close_many(handles: *const HandleValue, num_handles: usize) -> ZxStatus {
    syscall2(
        crate::consts::SYS_HANDLE_CLOSE_MANY,
        handles as u64,
        num_handles as u64,
    )
}

/// Duplicate a handle with reduced rights.
///
/// # Safety
/// `out` must be a valid pointer.
pub unsafe fn zx_handle_duplicate(
    handle: HandleValue,
    rights: u32,
    out: *mut HandleValue,
) -> ZxStatus {
    syscall3(
        crate::consts::SYS_HANDLE_DUPLICATE,
        handle as u64,
        rights as u64,
        out as u64,
    )
}

/// Replace a handle with one that has different rights.
///
/// # Safety
/// `out` must be a valid pointer.
pub unsafe fn zx_handle_replace(
    handle: HandleValue,
    rights: u32,
    out: *mut HandleValue,
) -> ZxStatus {
    syscall3(
        crate::consts::SYS_HANDLE_REPLACE,
        handle as u64,
        rights as u64,
        out as u64,
    )
}

// --- Socket syscalls ---

/// Create a socket pair.
///
/// # Safety
/// `out0` and `out1` must be valid pointers.
pub unsafe fn zx_socket_create(
    options: u32,
    out0: *mut HandleValue,
    out1: *mut HandleValue,
) -> ZxStatus {
    syscall3(
        crate::consts::SYS_SOCKET_CREATE,
        options as u64,
        out0 as u64,
        out1 as u64,
    )
}

/// Write data to a socket.
///
/// # Safety
/// `buffer` must point to `buffer_size` valid bytes.
pub unsafe fn zx_socket_write(
    handle: HandleValue,
    options: u32,
    buffer: *const u8,
    buffer_size: usize,
    actual: *mut usize,
) -> ZxStatus {
    syscall5(
        crate::consts::SYS_SOCKET_WRITE,
        handle as u64,
        options as u64,
        buffer as u64,
        buffer_size as u64,
        actual as u64,
    )
}

/// Read data from a socket.
///
/// # Safety
/// `buffer` must point to `buffer_size` valid bytes.
pub unsafe fn zx_socket_read(
    handle: HandleValue,
    options: u32,
    buffer: *mut u8,
    buffer_size: usize,
    actual: *mut usize,
) -> ZxStatus {
    syscall5(
        crate::consts::SYS_SOCKET_READ,
        handle as u64,
        options as u64,
        buffer as u64,
        buffer_size as u64,
        actual as u64,
    )
}

// --- Event syscalls ---

/// Create an event object.
///
/// # Safety
/// `out` must be a valid pointer.
pub unsafe fn zx_event_create(options: u32, out: *mut HandleValue) -> ZxStatus {
    syscall2(crate::consts::SYS_EVENT_CREATE, options as u64, out as u64)
}

/// Create an event pair.
///
/// # Safety
/// `out0` and `out1` must be valid pointers.
pub unsafe fn zx_eventpair_create(
    options: u32,
    out0: *mut HandleValue,
    out1: *mut HandleValue,
) -> ZxStatus {
    syscall3(
        crate::consts::SYS_EVENTPAIR_CREATE,
        options as u64,
        out0 as u64,
        out1 as u64,
    )
}

// --- FIFO syscalls ---

/// Create a FIFO pair.
///
/// # Safety
/// `out0` and `out1` must be valid pointers.
pub unsafe fn zx_fifo_create(
    elem_count: usize,
    elem_size: usize,
    options: u32,
    out0: *mut HandleValue,
    out1: *mut HandleValue,
) -> ZxStatus {
    syscall5(
        crate::consts::SYS_FIFO_CREATE,
        elem_count as u64,
        elem_size as u64,
        options as u64,
        out0 as u64,
        out1 as u64,
    )
}

// --- Timer syscalls ---

/// Create a timer.
///
/// # Safety
/// `out` must be a valid pointer.
pub unsafe fn zx_timer_create(options: u32, clock_id: u32, out: *mut HandleValue) -> ZxStatus {
    syscall3(
        crate::consts::SYS_TIMER_CREATE,
        options as u64,
        clock_id as u64,
        out as u64,
    )
}

/// Set a timer deadline.
pub unsafe fn zx_timer_set(handle: HandleValue, deadline: i64, slack: u64) -> ZxStatus {
    syscall3(
        crate::consts::SYS_TIMER_SET,
        handle as u64,
        deadline as u64,
        slack,
    )
}

/// Cancel a pending timer.
pub unsafe fn zx_timer_cancel(handle: HandleValue) -> ZxStatus {
    syscall1(crate::consts::SYS_TIMER_CANCEL, handle as u64)
}

// --- Object syscalls (additional) ---

/// Signal an object.
pub unsafe fn zx_object_signal(handle: HandleValue, clear_mask: u32, set_mask: u32) -> ZxStatus {
    syscall3(
        crate::consts::SYS_OBJECT_SIGNAL,
        handle as u64,
        clear_mask as u64,
        set_mask as u64,
    )
}

/// Signal an object's peer.
pub unsafe fn zx_object_signal_peer(
    handle: HandleValue,
    clear_mask: u32,
    set_mask: u32,
) -> ZxStatus {
    syscall3(
        crate::consts::SYS_OBJECT_SIGNAL_PEER,
        handle as u64,
        clear_mask as u64,
        set_mask as u64,
    )
}

// --- Thread syscalls (additional) ---

/// Start a thread.
pub unsafe fn zx_thread_start(
    handle: HandleValue,
    entry: usize,
    stack: usize,
    arg1: usize,
    arg2: usize,
) -> ZxStatus {
    syscall5(
        crate::consts::SYS_THREAD_START,
        handle as u64,
        entry as u64,
        stack as u64,
        arg1 as u64,
        arg2 as u64,
    )
}

// --- Nanosleep ---

/// Sleep for a specified duration.
pub unsafe fn zx_nanosleep(deadline: i64) -> ZxStatus {
    syscall1(crate::consts::SYS_NANOSLEEP, deadline as u64)
}

// --- VMAR syscalls (additional) ---

/// Unmap a region from a VMAR.
pub unsafe fn zx_vmar_unmap(handle: HandleValue, addr: usize, len: usize) -> ZxStatus {
    syscall3(
        crate::consts::SYS_VMAR_UNMAP,
        handle as u64,
        addr as u64,
        len as u64,
    )
}

/// Set protection flags on a VMAR region.
pub unsafe fn zx_vmar_protect(
    handle: HandleValue,
    options: u32,
    addr: usize,
    len: usize,
) -> ZxStatus {
    syscall4(
        crate::consts::SYS_VMAR_PROTECT,
        handle as u64,
        options as u64,
        addr as u64,
        len as u64,
    )
}

/// Allocate a sub-region in a VMAR.
///
/// # Safety
/// Output pointers must be valid.
pub unsafe fn zx_vmar_allocate(
    parent_vmar: HandleValue,
    options: u32,
    offset: usize,
    size: usize,
    child_vmar: *mut HandleValue,
    child_addr: *mut usize,
) -> ZxStatus {
    syscall6(
        crate::consts::SYS_VMAR_ALLOCATE,
        parent_vmar as u64,
        options as u64,
        offset as u64,
        size as u64,
        child_vmar as u64,
        child_addr as u64,
    )
}

// --- Port syscalls ---

/// Create a port.
///
/// # Safety
/// `out` must be a valid pointer.
pub unsafe fn zx_port_create(options: u32, out: *mut HandleValue) -> ZxStatus {
    syscall2(crate::consts::SYS_PORT_CREATE, options as u64, out as u64)
}

/// Wait for a packet on a port.
///
/// # Safety
/// `packet` must point to a valid `zx_port_packet_t`.
pub unsafe fn zx_port_wait(handle: HandleValue, deadline: i64, packet: *mut u8) -> ZxStatus {
    syscall3(
        crate::consts::SYS_PORT_WAIT,
        handle as u64,
        deadline as u64,
        packet as u64,
    )
}

/// Queue a packet to a port.
///
/// # Safety
/// `packet` must point to a valid `zx_port_packet_t`.
pub unsafe fn zx_port_queue(handle: HandleValue, packet: *const u8) -> ZxStatus {
    syscall2(crate::consts::SYS_PORT_QUEUE, handle as u64, packet as u64)
}

// --- Futex syscalls ---

/// Wait on a futex.
pub unsafe fn zx_futex_wait(
    value_ptr: *const i32,
    current_value: i32,
    new_futex_owner: HandleValue,
    deadline: i64,
) -> ZxStatus {
    syscall4(
        crate::consts::SYS_FUTEX_WAIT,
        value_ptr as u64,
        current_value as u64,
        new_futex_owner as u64,
        deadline as u64,
    )
}

/// Wake waiters on a futex.
pub unsafe fn zx_futex_wake(value_ptr: *const i32, wake_count: u32) -> ZxStatus {
    syscall2(
        crate::consts::SYS_FUTEX_WAKE,
        value_ptr as u64,
        wake_count as u64,
    )
}

/// Wake some waiters and requeue others to a different futex.
pub unsafe fn zx_futex_requeue(
    value_ptr: *const i32,
    wake_count: u32,
    current_value: i32,
    requeue_ptr: *const i32,
    requeue_count: u32,
    new_requeue_owner: HandleValue,
) -> ZxStatus {
    syscall6(
        crate::consts::SYS_FUTEX_REQUEUE,
        value_ptr as u64,
        wake_count as u64,
        current_value as u64,
        requeue_ptr as u64,
        requeue_count as u64,
        new_requeue_owner as u64,
    )
}

// --- CPRNG ---

/// Draw random bytes.
///
/// # Safety
/// `buffer` must point to `buffer_size` valid bytes.
pub unsafe fn zx_cprng_draw_once(buffer: *mut u8, buffer_size: usize) -> ZxStatus {
    syscall2(
        crate::consts::SYS_CPRNG_DRAW_ONCE,
        buffer as u64,
        buffer_size as u64,
    )
}

// --- Task syscalls ---

/// Kill a task (job, process, or thread).
pub unsafe fn zx_task_kill(handle: HandleValue) -> ZxStatus {
    syscall1(crate::consts::SYS_TASK_KILL, handle as u64)
}

// --- Debuglog syscalls ---

/// Read from the kernel debug log.
///
/// # Safety
/// `buffer` must point to valid memory.
pub unsafe fn zx_debuglog_read(
    handle: HandleValue,
    options: u32,
    buffer: *mut u8,
    buffer_size: usize,
) -> ZxStatus {
    syscall4(
        crate::consts::SYS_DEBUGLOG_READ,
        handle as u64,
        options as u64,
        buffer as u64,
        buffer_size as u64,
    )
}

/// Create a debuglog handle.
///
/// # Safety
/// `out` must be a valid pointer.
pub unsafe fn zx_debuglog_create(
    resource: HandleValue,
    options: u32,
    out: *mut HandleValue,
) -> ZxStatus {
    syscall3(
        crate::consts::SYS_DEBUGLOG_CREATE,
        resource as u64,
        options as u64,
        out as u64,
    )
}

/// Write to a debuglog.
///
/// # Safety
/// `buffer` must point to `buffer_size` valid bytes.
pub unsafe fn zx_debuglog_write(
    handle: HandleValue,
    options: u32,
    buffer: *const u8,
    buffer_size: usize,
) -> ZxStatus {
    syscall4(
        crate::consts::SYS_DEBUGLOG_WRITE,
        handle as u64,
        options as u64,
        buffer as u64,
        buffer_size as u64,
    )
}

// --- VMO syscalls (additional) ---

/// Set the size of a VMO.
pub unsafe fn zx_vmo_set_size(handle: HandleValue, size: u64) -> ZxStatus {
    syscall2(crate::consts::SYS_VMO_SET_SIZE, handle as u64, size)
}

/// Create a child VMO (clone/snapshot).
///
/// # Safety
/// `out` must be a valid pointer.
pub unsafe fn zx_vmo_create_child(
    handle: HandleValue,
    options: u32,
    offset: u64,
    size: u64,
    out: *mut HandleValue,
) -> ZxStatus {
    syscall5(
        crate::consts::SYS_VMO_CREATE_CHILD,
        handle as u64,
        options as u64,
        offset,
        size,
        out as u64,
    )
}
