//! Zircon syscall implementations

#![no_std]
#![deny(warnings, unsafe_code, unreachable_patterns)]
#![allow(unexpected_cfgs)]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

use alloc::sync::Arc;
use core::convert::TryFrom;
use core::sync::atomic::{AtomicI32, Ordering};

use futures::pin_mut;
use hal::MMUFlags;
use hal_impl::user::{IoVecIn, IoVecOut, UserInOutPtr, UserInPtr, UserOutPtr};
use zircon_object::object::{wait_signal_many, KernelObject, KoID, Rights, Signal};
use zircon_object::object::{Handle, HandleBasicInfo, HandleValue, INVALID_HANDLE};
use zircon_object::task::{CurrentThread, PolicyCondition, Thread, ThreadFn};
use zircon_object::{ZxError, ZxResult};

use self::consts::SyscallType as Sys;
pub use self::time::Deadline;

mod channel;
mod consts {
    include!(concat!(env!("OUT_DIR"), "/consts.rs"));
}
mod cprng;
mod ddk;
mod debug;
mod debuglog;
mod exception;
mod fifo;
mod futex;
mod handle;
#[cfg(feature = "hypervisor")]
mod hypervisor;
mod iob;
mod object;
mod pager;
mod port;
mod resource;
mod restricted;
mod sampler;
mod signal;
mod socket;
mod stream;
mod system;
mod task;
mod test_syscalls;
mod time;
mod vmar;
mod vmo;

/// Zircon pseudo-handle for the current thread (`zx_thread_self()`).
const ZX_PSEUDO_HANDLE_THREAD_SELF: HandleValue = 0xFFFF_0001;
/// Zircon pseudo-handle for the current process (`zx_process_self()`).
const ZX_PSEUDO_HANDLE_PROCESS_SELF: HandleValue = 0xFFFF_0002;
/// Zircon pseudo-handle for the root VMAR (`zx_vmar_root_self()`).
const ZX_PSEUDO_HANDLE_VMAR_ROOT_SELF: HandleValue = 0xFFFF_0003;

pub struct Syscall<'a> {
    pub thread: &'a CurrentThread,
    pub thread_fn: ThreadFn,
}

impl Syscall<'_> {
    /// Check that a user buffer is readable at the VMAR mapping level.
    ///
    /// On x86_64, hardware page tables don't have a read-disable bit — any
    /// PRESENT page is readable by the CPU. This check validates the VMAR
    /// mapping's logical permissions to reject reads from pages mapped
    /// without PERM_READ (e.g., `mmap(PROT_NONE)`).
    ///
    /// Returns:
    /// - `Ok(())` if the buffer is fully mapped with read permissions.
    /// - `Err(INVALID_ARGS)` if any part of the buffer is not mapped or
    ///   lacks read permissions. Fuchsia returns INVALID_ARGS for all
    ///   bad user pointer errors at the syscall boundary.
    #[allow(dead_code)]
    fn check_user_buffer_read(&self, addr: usize, len: usize) -> ZxResult {
        if len == 0 {
            return Ok(());
        }
        let vmar = self.thread.proc().vmar();
        match vmar.check_user_access(addr, len, MMUFlags::READ) {
            Ok(()) => Ok(()),
            Err(ZxError::ACCESS_DENIED) => Err(ZxError::INVALID_ARGS),
            #[cfg(feature = "libos")]
            Err(ZxError::NOT_FOUND) => {
                // In libos mode, addresses not in any VMAR mapping may be
                // valid host memory (stack, heap). Allow them.
                Ok(())
            }
            Err(_) => Err(ZxError::INVALID_ARGS),
        }
    }

    /// Validate that a user-space buffer is fully mapped with write
    /// permissions. Returns `INVALID_ARGS` for bad pointers.
    #[allow(dead_code)]
    fn check_user_buffer_write(&self, addr: usize, len: usize) -> ZxResult {
        if len == 0 {
            return Ok(());
        }
        let vmar = self.thread.proc().vmar();
        match vmar.check_user_access(addr, len, MMUFlags::WRITE) {
            Ok(()) => Ok(()),
            Err(ZxError::ACCESS_DENIED) => Err(ZxError::INVALID_ARGS),
            #[cfg(feature = "libos")]
            Err(ZxError::NOT_FOUND) => {
                // In libos mode, addresses not in any VMAR mapping may be
                // valid host memory (stack, heap). Allow them.
                Ok(())
            }
            Err(_) => Err(ZxError::INVALID_ARGS),
        }
    }

    /// Enforce a job basic policy, delivering a policy exception if the
    /// action requires it.
    ///
    /// This is the async wrapper around `Process::check_policy`.  When
    /// the policy action is `AllowException` or `DenyException`, this
    /// method delivers a `ZX_EXCP_POLICY_ERROR` exception on the current
    /// thread (matching Fuchsia's `SignalPolicyException` + deferred
    /// delivery in `ProcessPendingSignals`).
    #[allow(dead_code)]
    async fn enforce_policy(&self, condition: PolicyCondition) -> ZxResult {
        let (result, synth_code) = self.thread.proc().check_policy_with_exception(condition);
        if let Some(code) = synth_code {
            self.thread.handle_exception_policy(code, 0).await;
        }
        result
    }

    /// Resolve a handle value that may be a pseudo-handle.
    ///
    /// Fuchsia defines pseudo-handles for the current thread, process, and
    /// root VMAR.  These are NOT in the process handle table — they are
    /// well-known constants that the kernel maps to the caller's objects.
    /// Returns `None` if the handle is not a pseudo-handle (use normal lookup).
    /// Resolve a handle value that may be a pseudo-handle, returning the
    /// kernel object as a trait object.
    fn resolve_pseudo_handle(&self, handle_value: HandleValue) -> Option<Arc<dyn KernelObject>> {
        match handle_value {
            ZX_PSEUDO_HANDLE_THREAD_SELF => Some(self.thread.inner()),
            ZX_PSEUDO_HANDLE_PROCESS_SELF => Some(self.thread.proc()),
            ZX_PSEUDO_HANDLE_VMAR_ROOT_SELF => Some(self.thread.proc().vmar()),
            _ => None,
        }
    }

    /// Like `proc.get_dyn_object_with_rights`, but also handles pseudo-handles.
    fn get_object_with_pseudo(
        &self,
        handle_value: HandleValue,
        rights: Rights,
    ) -> ZxResult<Arc<dyn KernelObject>> {
        if let Some(obj) = self.resolve_pseudo_handle(handle_value) {
            let _ = rights; // Pseudo-handles have all rights.
            Ok(obj)
        } else {
            self.thread
                .proc()
                .get_dyn_object_with_rights(handle_value, rights)
        }
    }

    /// Like `proc.get_dyn_object_and_rights`, but also handles pseudo-handles.
    fn get_object_and_rights_with_pseudo(
        &self,
        handle_value: HandleValue,
    ) -> ZxResult<(Arc<dyn KernelObject>, Rights)> {
        if let Some(obj) = self.resolve_pseudo_handle(handle_value) {
            Ok((obj, Rights::all()))
        } else {
            self.thread.proc().get_dyn_object_and_rights(handle_value)
        }
    }

    /// Resolve a thread handle that may be the pseudo-handle for the current thread.
    fn get_thread_with_pseudo(&self, handle_value: HandleValue) -> ZxResult<Arc<Thread>> {
        if handle_value == ZX_PSEUDO_HANDLE_THREAD_SELF {
            Ok(self.thread.inner())
        } else {
            self.thread.proc().get_object::<Thread>(handle_value)
        }
    }
}

impl Syscall<'_> {
    /// Pre-fault pager-backed pages in a user buffer range.
    ///
    /// When a syscall needs to read from or write to a user buffer
    /// that might be in a pager-backed mapping, call this before the
    /// usercopy to demand-page the pages. Uses `blocking_run` so
    /// `Thread::kill()` can interrupt the wait, and sets the thread
    /// state to `BlockedPager` for `WaitForBlocked` polls.
    async fn prefault_user_buffer(&self, addr: usize, len: usize) -> ZxResult {
        use hal::{MMUFlags, PAGE_SIZE};
        use zircon_object::task::ThreadState;

        if len == 0 || addr == 0 {
            return Ok(());
        }
        let end_addr = addr.checked_add(len).ok_or(ZxError::INVALID_ARGS)?;
        let vmar = self.thread.proc().vmar();
        let start_page = addr / PAGE_SIZE;
        let end_page = end_addr.div_ceil(PAGE_SIZE);
        for page_idx in start_page..end_page {
            let vaddr = page_idx * PAGE_SIZE;
            match vmar.handle_page_fault(vaddr, MMUFlags::READ | MMUFlags::USER) {
                Ok(()) => continue,
                Err(ZxError::SHOULD_WAIT) => {
                    let pager_vmo = vmar.find_pager_vmo(vaddr).or_else(|| {
                        let mapping = vmar.find_mapping(vaddr)?;
                        if mapping.vmo.is_in_pager_tree() {
                            mapping.vmo.find_root_pager_vmo()
                        } else {
                            None
                        }
                    });
                    let offset = vmar.vaddr_to_vmo_offset(vaddr);
                    if let (Some(vmo), Some(offset)) = (pager_vmo, offset) {
                        let future = vmo.wait_for_page(offset);
                        pin_mut!(future);
                        self.thread.set_blocking_state(ThreadState::BlockedPager);
                        // Mark as non-suspendable (kernel usercopy wait).
                        self.thread.set_nonsuspendable_block(true);
                        let result = self
                            .thread
                            .blocking_run::<_, (), _>(
                                future,
                                ThreadState::BlockedPager,
                                core::time::Duration::from_nanos(u64::MAX),
                                None,
                            )
                            .await;
                        self.thread.set_nonsuspendable_block(false);
                        result?;
                        // Re-fault to create the PTE now that the page is supplied.
                        let _ = vmar.handle_page_fault(vaddr, MMUFlags::READ | MMUFlags::USER);
                    }
                }
                Err(ZxError::NOT_FOUND) => break,
                Err(_) => {} // Non-pager page, skip silently
            }
        }
        Ok(())
    }

    /// Pre-dirty TRAP_DIRTY pages before writing to a pager-backed VMO.
    ///
    /// For VMOs created with ZX_VMO_TRAP_DIRTY, writes to clean pages
    /// must send a ZX_PAGER_VMO_DIRTY request to the pager port and
    /// wait for acknowledgement. This function checks each page in the
    /// write range and sends dirty requests for clean pages.
    async fn predirty_trap_dirty_pages(
        &self,
        vmo: &Arc<zircon_object::vm::VmObject>,
        offset: usize,
        len: usize,
    ) -> ZxResult {
        use hal::PAGE_SIZE;
        use zircon_object::task::ThreadState;

        if !vmo.is_trap_dirty() || len == 0 {
            return Ok(());
        }
        // After pager detach, all writes to TRAP_DIRTY VMOs fail
        // with BAD_STATE — the pager can no longer acknowledge dirty.
        if !vmo.has_pager() {
            return Err(zircon_object::ZxError::BAD_STATE);
        }
        let start_page = offset / PAGE_SIZE;
        let end_page = offset
            .checked_add(len)
            .ok_or(ZxError::INVALID_ARGS)?
            .div_ceil(PAGE_SIZE);
        // Find contiguous runs of clean pages and send one dirty
        // request per run. The pager expects coalesced requests
        // matching the contiguous dirty range.
        let mut page_idx = start_page;
        while page_idx < end_page {
            // Skip already-dirty pages.
            if vmo.is_page_dirty(page_idx) {
                page_idx += 1;
                continue;
            }
            // Find the end of the contiguous clean run.
            let run_start = page_idx;
            while page_idx < end_page && !vmo.is_page_dirty(page_idx) {
                page_idx += 1;
            }
            let run_len = page_idx - run_start;
            let run_offset = run_start * PAGE_SIZE;
            let run_bytes = run_len * PAGE_SIZE;
            match vmo.request_dirty(run_offset, run_bytes) {
                Err(zircon_object::ZxError::SHOULD_WAIT) => {} // expected
                Err(zircon_object::ZxError::NOT_FOUND) => {
                    // Pager detached — writing to clean pages on a
                    // detached TRAP_DIRTY VMO returns BAD_STATE.
                    return Err(zircon_object::ZxError::BAD_STATE);
                }
                Err(e) => return Err(e),
                Ok(()) => {} // shouldn't happen but harmless
            }
            // Wait for the first page of the run to become dirty.
            let future = vmo.wait_for_dirty(run_offset);
            pin_mut!(future);
            self.thread.set_blocking_state(ThreadState::BlockedPager);
            self.thread
                .blocking_run::<_, (), _>(
                    future,
                    ThreadState::BlockedPager,
                    core::time::Duration::from_nanos(u64::MAX),
                    None,
                )
                .await?;
            // Re-scan from run_start: the pager may have acknowledged
            // only part of the run. The outer loop will pick up any
            // remaining clean pages and send new dirty requests.
            page_idx = run_start;
        }
        Ok(())
    }

    /// Pre-commit pages in a pager-tree VMO (clone of pager-backed VMO)
    /// before writing. When the clone's commit_page encounters an
    /// unsupplied parent page, the parent returns SHOULD_WAIT. This
    /// method uses committed_pages_in_range to check which parent pages
    /// are missing, requests them from the pager, and waits for supply.
    async fn prefault_pager_write_pages(
        &self,
        vmo: &Arc<zircon_object::vm::VmObject>,
        offset: usize,
        len: usize,
    ) -> ZxResult {
        use hal::PAGE_SIZE;
        use zircon_object::task::ThreadState;

        if len == 0 || !vmo.is_in_pager_tree() {
            return Ok(());
        }
        let pager_vmo = match vmo.find_root_pager_vmo() {
            Some(v) => v,
            None => return Ok(()),
        };
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + len).div_ceil(PAGE_SIZE);
        // Check if the root pager VMO has all pages supplied.
        // If not, request the missing pages and wait.
        // Helper: check if a page is "supplied" — has either a
        // committed frame or a zero marker from supply_pages.
        let is_page_supplied = |vmo: &Arc<zircon_object::vm::VmObject>, pg: usize| -> bool {
            vmo.committed_pages_in_range(pg, pg + 1) > 0 || vmo.has_zero_marker(pg)
        };
        loop {
            // Check if all pages in range are supplied.
            let all_supplied = (start_page..end_page).all(|pg| is_page_supplied(&pager_vmo, pg));
            if all_supplied {
                break;
            }
            // Find the first unsupplied page.
            let mut first_missing = start_page;
            for pg in start_page..end_page {
                if !is_page_supplied(&pager_vmo, pg) {
                    first_missing = pg;
                    break;
                }
            }
            // Find run of unsupplied pages.
            let mut run_end = first_missing + 1;
            while run_end < end_page && !is_page_supplied(&pager_vmo, run_end) {
                run_end += 1;
            }
            let run_offset = first_missing * PAGE_SIZE;
            let run_len = (run_end - first_missing) * PAGE_SIZE;
            let _ = pager_vmo.request_pages(run_offset, run_len);
            let future = pager_vmo.wait_for_page(run_offset);
            pin_mut!(future);
            self.thread.set_blocking_state(ThreadState::BlockedPager);
            self.thread
                .blocking_run::<_, (), _>(
                    future,
                    ThreadState::BlockedPager,
                    core::time::Duration::from_nanos(u64::MAX),
                    None,
                )
                .await?;
        }
        Ok(())
    }

    pub async fn syscall(&mut self, num: usize, args: [usize; 8]) -> isize {
        let thread_name = self.thread.name();
        let proc_name = self.thread.proc().name();
        let num32 = u32::try_from(num).unwrap_or(u32::MAX);
        let sys_type = match Sys::try_from(num32) {
            Ok(Sys::COUNT) | Err(_) => {
                error!("invalid syscall number: {}", num);
                // Signal a BAD_SYSCALL policy exception, matching Fuchsia's
                // sys_invalid_syscall which calls SignalPolicyException.
                self.thread.signal_policy_exception(
                    zircon_object::task::policy_code::BAD_SYSCALL,
                    num as u32,
                );
                self.thread.deliver_pending_policy_exception().await;
                return ZxError::BAD_SYSCALL as _;
            }
            Ok(t) => t,
        };

        debug!(
            "{}|{} {:?} => args={:x?}",
            proc_name, thread_name, sys_type, args
        );

        let [a0, a1, a2, a3, a4, a5, a6, a7] = args;
        let ret = match sys_type {
            Sys::HANDLE_CLOSE => self.sys_handle_close(a0 as _),
            Sys::HANDLE_CLOSE_MANY => self.sys_handle_close_many(a0.into(), a1 as _),
            Sys::HANDLE_DUPLICATE => self.sys_handle_duplicate(a0 as _, a1 as _, a2.into()),
            Sys::HANDLE_REPLACE => self.sys_handle_replace(a0 as _, a1 as _, a2.into()),
            Sys::OBJECT_GET_INFO => {
                // Pre-fault pager pages in user buffer (a2=buffer, a3=size).
                if let Err(e) = self.prefault_user_buffer(a2, a3).await {
                    Err(e)
                } else {
                    self.sys_object_get_info(
                        a0 as _,
                        a1 as _,
                        a2 as _,
                        a3 as _,
                        a4.into(),
                        a5.into(),
                    )
                }
            }
            Sys::OBJECT_GET_PROPERTY => {
                self.sys_object_get_property(a0 as _, a1 as _, a2 as _, a3 as _)
            }
            Sys::OBJECT_SET_PROPERTY => {
                self.sys_object_set_property(a0 as _, a1 as _, a2 as _, a3 as _)
            }
            Sys::OBJECT_SIGNAL => self.sys_object_signal(a0 as _, a1 as _, a2 as _),
            Sys::OBJECT_SIGNAL_PEER => self.sys_object_signal_peer(a0 as _, a1 as _, a2 as _),
            Sys::OBJECT_WAIT_ONE => {
                self.sys_object_wait_one(a0 as _, a1 as _, a2.into(), a3.into())
                    .await
            }
            Sys::OBJECT_WAIT_MANY => {
                self.sys_object_wait_many(a0.into(), a1 as _, a2.into())
                    .await
            }
            Sys::OBJECT_WAIT_ASYNC => {
                self.sys_object_wait_async(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _)
            }
            Sys::THREAD_CREATE => {
                self.sys_thread_create(a0 as _, a1.into(), a2 as _, a3 as _, a4.into())
            }
            Sys::THREAD_START => self.sys_thread_start(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _),
            Sys::THREAD_START_REGS => self.sys_thread_start_regs(
                a0 as _, a1 as _, a2 as _, a3 as _, a4 as _, a5 as _, a6 as _,
            ),
            Sys::THREAD_WRITE_STATE => {
                self.sys_thread_write_state(a0 as _, a1 as _, a2.into(), a3 as _)
            }
            Sys::THREAD_READ_STATE => {
                self.sys_thread_read_state(a0 as _, a1 as _, a2.into(), a3 as _)
            }
            Sys::TASK_KILL => self.sys_task_kill(a0 as _),
            Sys::THREAD_EXIT => self.sys_thread_exit(),
            Sys::PROCESS_CREATE => {
                self.sys_process_create(a0 as _, a1.into(), a2 as _, a3 as _, a4.into(), a5.into())
            }
            Sys::PROCESS_START => {
                self.sys_process_start(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _, a5 as _)
            }
            Sys::PROCESS_READ_MEMORY => {
                self.sys_process_read_memory(a0 as _, a1 as _, a2.into(), a3 as _, a4.into())
            }
            Sys::PROCESS_WRITE_MEMORY => {
                self.sys_process_write_memory(a0 as _, a1 as _, a2.into(), a3 as _, a4.into())
            }
            Sys::PROCESS_EXIT => self.sys_process_exit(a0 as _),
            Sys::JOB_CREATE => self.sys_job_create(a0 as _, a1 as _, a2.into()),
            Sys::JOB_SET_POLICY => self.sys_job_set_policy(a0 as _, a1 as _, a2 as _, a3, a4 as _),
            Sys::JOB_SET_CRITICAL => self.sys_job_set_critical(a0 as _, a1 as _, a2 as _),
            Sys::TASK_SUSPEND | Sys::TASK_SUSPEND_TOKEN => {
                self.sys_task_suspend_token(a0 as _, a1.into())
            }
            Sys::CHANNEL_CREATE => self.sys_channel_create(a0 as _, a1.into(), a2.into()),
            Sys::CHANNEL_READ => self.sys_channel_read(
                a0 as _,
                a1 as _,
                a2.into(),
                a3 as _,
                a4 as _,
                a5 as _,
                a6.into(),
                a7.into(),
                false,
            ),
            Sys::CHANNEL_READ_ETC => self.sys_channel_read(
                a0 as _,
                a1 as _,
                a2.into(),
                a3 as _,
                a4 as _,
                a5 as _,
                a6.into(),
                a7.into(),
                true,
            ),
            Sys::CHANNEL_WRITE => {
                self.sys_channel_write(a0 as _, a1 as _, a2.into(), a3 as _, a4.into(), a5 as _)
            }
            Sys::CHANNEL_WRITE_ETC => {
                self.sys_channel_write_etc(a0 as _, a1 as _, a2.into(), a3 as _, a4.into(), a5 as _)
            }
            Sys::CHANNEL_CALL_NORETRY => {
                self.sys_channel_call_noretry(
                    a0 as _,
                    a1 as _,
                    a2.into(),
                    a3.into(),
                    a4.into(),
                    a5.into(),
                )
                .await
            }
            Sys::CHANNEL_CALL_FINISH => {
                self.sys_channel_call_finish(a0.into(), a1.into(), a2.into(), a3.into())
            }
            Sys::CHANNEL_CALL_ETC_NORETRY => {
                self.sys_channel_call_etc_noretry(
                    a0 as _,
                    a1 as _,
                    a2.into(),
                    a3.into(),
                    a4.into(),
                    a5.into(),
                )
                .await
            }
            Sys::CHANNEL_CALL_ETC_FINISH => {
                self.sys_channel_call_etc_finish(a0.into(), a1.into(), a2.into(), a3.into())
            }
            Sys::SOCKET_CREATE => self.sys_socket_create(a0 as _, a1.into(), a2.into()),
            Sys::SOCKET_WRITE => {
                self.sys_socket_write(a0 as _, a1 as _, a2.into(), a3 as _, a4.into())
            }
            Sys::SOCKET_READ => {
                self.sys_socket_read(a0 as _, a1 as _, a2.into(), a3 as _, a4.into())
            }
            Sys::SOCKET_SHUTDOWN => self.sys_socket_shutdown(a0 as _, a1 as _),
            Sys::SOCKET_SET_DISPOSITION => {
                self.sys_socket_set_disposition(a0 as _, a1 as _, a2 as _)
            }
            Sys::STREAM_CREATE => self.sys_stream_create(a0 as _, a1 as _, a2 as _, a3.into()),
            Sys::STREAM_WRITEV => {
                self.sys_stream_writev(a0 as _, a1 as _, a2.into(), a3 as _, a4.into())
                    .await
            }
            Sys::STREAM_WRITEV_AT => {
                self.sys_stream_writev_at(a0 as _, a1 as _, a2 as _, a3.into(), a4 as _, a5.into())
                    .await
            }
            Sys::STREAM_READV => {
                self.sys_stream_readv(a0 as _, a1 as _, a2.into(), a3 as _, a4.into())
            }
            Sys::STREAM_READV_AT => {
                self.sys_stream_readv_at(a0 as _, a1 as _, a2 as _, a3.into(), a4 as _, a5.into())
            }
            Sys::STREAM_SEEK => self.sys_stream_seek(a0 as _, a1 as _, a2 as _, a3.into()),
            Sys::FIFO_CREATE => {
                self.sys_fifo_create(a0 as _, a1 as _, a2 as _, a3.into(), a4.into())
            }
            Sys::FIFO_READ => self.sys_fifo_read(a0 as _, a1 as _, a2.into(), a3 as _, a4.into()),
            Sys::FIFO_WRITE => self.sys_fifo_write(a0 as _, a1 as _, a2.into(), a3 as _, a4.into()),
            Sys::EVENT_CREATE => self.sys_event_create(a0 as _, a1.into()),
            Sys::EVENTPAIR_CREATE => self.sys_eventpair_create(a0 as _, a1.into(), a2.into()),
            Sys::PORT_CREATE => self.sys_port_create(a0 as _, a1.into()),
            Sys::PORT_WAIT => self.sys_port_wait(a0 as _, a1.into(), a2.into()).await,
            Sys::PORT_QUEUE => self.sys_port_queue(a0 as _, a1.into()),
            Sys::PORT_CANCEL => self.sys_port_cancel(a0 as _, a1 as _, a2 as _),
            Sys::FUTEX_WAIT => {
                self.sys_futex_wait(a0.into(), a1 as _, a2 as _, a3.into())
                    .await
            }
            Sys::FUTEX_WAKE => self.sys_futex_wake(a0.into(), a1 as _),
            Sys::FUTEX_REQUEUE => {
                self.sys_futex_requeue(a0.into(), a1 as _, a2 as _, a3.into(), a4 as _, a5 as _)
            }
            Sys::FUTEX_WAKE_SINGLE_OWNER => self.sys_futex_wake_single_owner(a0.into()),
            Sys::FUTEX_REQUEUE_SINGLE_OWNER => {
                self.sys_futex_requeue_single_owner(a0.into(), a1 as _, a2.into(), a3 as _, a4 as _)
            }
            Sys::FUTEX_GET_OWNER => self.sys_futex_get_owner(a0.into(), a1.into()),
            Sys::VMO_CREATE => self.sys_vmo_create(a0 as _, a1 as _, a2.into()),
            Sys::VMO_READ => {
                self.sys_vmo_read(a0 as _, a1.into(), a2 as _, a3 as _)
                    .await
            }
            Sys::VMO_WRITE => {
                // Pre-fault pager pages in user buffer (a1=buf, a3=size).
                if let Err(e) = self.prefault_user_buffer(a1, a3).await {
                    return e as isize;
                }
                let proc = self.thread.proc();
                if let Ok(vmo) = proc.get_object::<zircon_object::vm::VmObject>(a0 as _) {
                    // Pre-commit pager-tree pages (clone of pager-backed
                    // VMO) so the parent pages are supplied before writing.
                    if let Err(e) = self.prefault_pager_write_pages(&vmo, a2, a3).await {
                        return e as isize;
                    }
                    // Pre-dirty TRAP_DIRTY pages before writing.
                    if let Err(e) = self.predirty_trap_dirty_pages(&vmo, a2, a3).await {
                        return e as isize;
                    }
                }
                let result = self.sys_vmo_write(a0 as _, a1.into(), a2 as _, a3 as _);
                // After a successful write to a pager-backed VMO (or a slice
                // of one), set modified and mark pages dirty on the parent.
                // Note: for partial failures (some chunks succeed, later chunk
                // fails), result is Err and we skip dirty/modified tracking.
                // This matches the C++ tests where partial write failures
                // (e.g., unmappable source buffer) expect modified=false.
                if result.is_ok() {
                    if let Ok(vmo) = proc.get_object::<zircon_object::vm::VmObject>(a0 as _) {
                        // Find the pager-backed VMO: either this VMO or its
                        // parent (for slices/references only — COW clones
                        // write to their own pages, not the parent's).
                        let pager_vmo = if vmo.has_pager() {
                            Some(vmo.clone())
                        } else if vmo.is_slice_vmo() {
                            vmo.find_root_pager_vmo()
                        } else {
                            None
                        };
                        if let Some(pv) = pager_vmo {
                            // Compute parent-relative offsets for slices.
                            let parent_offset = a2 + vmo.get_slice_offset();
                            if !pv.is_trap_dirty() {
                                let start_page = parent_offset / hal::PAGE_SIZE;
                                let end_page = (parent_offset + a3).div_ceil(hal::PAGE_SIZE);
                                for pg in start_page..end_page {
                                    pv.auto_mark_dirty(pg);
                                }
                            } else {
                                pv.set_modified();
                            }
                        }
                    }
                }
                result
            }
            Sys::VMO_GET_SIZE => self.sys_vmo_get_size(a0 as _, a1.into()),
            Sys::VMO_SET_SIZE => self.sys_vmo_set_size(a0 as _, a1 as _),
            Sys::VMO_OP_RANGE => {
                // COMMIT on pager-backed VMOs needs async demand-paging.
                self.sys_vmo_op_range_async(a0 as _, a1 as _, a2 as _, a3 as _, a4.into(), a5 as _)
                    .await
            }
            Sys::VMO_REPLACE_AS_EXECUTABLE => {
                self.sys_vmo_replace_as_executable(a0 as _, a1 as _, a2.into())
            }
            Sys::VMO_CREATE_CHILD => {
                self.sys_vmo_create_child(a0 as _, a1 as _, a2 as _, a3 as _, a4.into())
            }
            Sys::VMO_CREATE_PHYSICAL => {
                self.sys_vmo_create_physical(a0 as _, a1 as _, a2 as _, a3.into())
            }
            Sys::VMO_CREATE_CONTIGUOUS => {
                self.sys_vmo_create_contiguous(a0 as _, a1 as _, a2 as _, a3.into())
            }
            Sys::VMO_SET_CACHE_POLICY => self.sys_vmo_cache_policy(a0 as _, a1 as _),
            Sys::VMAR_MAP => self.sys_vmar_map(
                a0 as _,
                a1 as _,
                a2 as _,
                a3 as _,
                a4 as _,
                a5 as _,
                a6.into(),
            ),
            Sys::VMAR_UNMAP => self.sys_vmar_unmap(a0 as _, a1 as _, a2 as _),
            Sys::VMAR_ALLOCATE => {
                self.sys_vmar_allocate(a0 as _, a1 as _, a2 as _, a3 as _, a4.into(), a5.into())
            }
            Sys::VMAR_PROTECT => self.sys_vmar_protect(a0 as _, a1 as _, a2 as _, a3 as _),
            Sys::VMAR_DESTROY => self.sys_vmar_destroy(a0 as _),
            Sys::CPRNG_DRAW_ONCE => self.sys_cprng_draw_once(a0.into(), a1 as _),
            Sys::CPRNG_ADD_ENTROPY => self.sys_cprng_add_entropy(a0.into(), a1 as _),
            Sys::NANOSLEEP => self.sys_nanosleep(a0.into()).await,
            Sys::CLOCK_CREATE => self.sys_clock_create(a0 as _, a1.into(), a2.into()),
            Sys::CLOCK_GET => self.sys_clock_get(a0 as _, a1.into()),
            // clock_get_monotonic_via_kernel returns the time value directly
            // in rax, not a zx_status_t.  Return early to bypass Ok→0 conversion.
            Sys::CLOCK_GET_MONOTONIC_VIA_KERNEL => {
                return self.sys_clock_get_monotonic_via_kernel() as isize;
            }
            // clock_get_boot_via_kernel returns the boot time directly in rax.
            // Boot time equals monotonic time because zCore does not support suspend.
            Sys::CLOCK_GET_BOOT_VIA_KERNEL => {
                return self.sys_clock_get_boot_via_kernel() as isize;
            }
            Sys::CLOCK_READ => self.sys_clock_read(a0 as _, a1.into()),
            Sys::CLOCK_GET_DETAILS => self.sys_clock_get_details(a0 as _, a1 as _, a2.into()),
            Sys::CLOCK_ADJUST => self.sys_clock_adjust(a0 as _, a1 as _, a2 as _),
            Sys::CLOCK_UPDATE => self.sys_clock_update(a0 as _, a1 as _, a2.into()),
            // ticks_get_via_kernel returns the tick count directly in rax.
            Sys::TICKS_GET_VIA_KERNEL => {
                return self.sys_ticks_get_via_kernel() as isize;
            }
            // ticks_get_boot_via_kernel returns the boot tick count directly in rax.
            // Boot ticks equal monotonic ticks because zCore does not support suspend.
            Sys::TICKS_GET_BOOT_VIA_KERNEL => {
                return self.sys_ticks_get_boot_via_kernel() as isize;
            }
            Sys::TIMER_CREATE => self.sys_timer_create(a0 as _, a1 as _, a2.into()),
            Sys::DEBUG_WRITE => self.sys_debug_write(a0.into(), a1 as _),
            Sys::DEBUG_EXEC => self.sys_debug_exec(a0.into(), a1 as _).await,
            Sys::DEBUGLOG_CREATE => self.sys_debuglog_create(a0 as _, a1 as _, a2.into()),
            Sys::DEBUGLOG_WRITE => self.sys_debuglog_write(a0 as _, a1 as _, a2.into(), a3 as _),
            Sys::DEBUGLOG_READ => {
                return match self.sys_debuglog_read(a0 as _, a1 as _, a2.into(), a3 as _) {
                    Ok(count) => count,
                    Err(err) => err as isize,
                };
            }
            Sys::RESOURCE_CREATE => self.sys_resource_create(
                a0 as _,
                a1 as _,
                a2 as _,
                a3 as _,
                a4.into(),
                a5 as _,
                a6.into(),
            ),
            Sys::SYSTEM_GET_EVENT => self.sys_system_get_event(a0 as _, a1 as _, a2.into()),
            Sys::TIMER_SET => self.sys_timer_set(a0 as _, a1.into(), a2 as _),
            Sys::TIMER_CANCEL => self.sys_timer_cancel(a0 as _),
            Sys::DEBUG_READ => {
                self.sys_debug_read(a0 as _, a1.into(), a2 as _, a3.into())
                    .await
            }
            Sys::TASK_CREATE_EXCEPTION_CHANNEL => {
                self.sys_create_exception_channel(a0 as _, a1 as _, a2.into())
            }
            Sys::IOMMU_CREATE => {
                self.sys_iommu_create(a0 as _, a1 as _, a2.into(), a3 as _, a4.into())
            }
            Sys::BTI_CREATE => self.sys_bti_create(a0 as _, a1 as _, a2 as _, a3.into()),
            Sys::BTI_PIN => {
                self.sys_bti_pin(
                    a0 as _,
                    a1 as _,
                    a2 as _,
                    a3 as _,
                    a4 as _,
                    a5.into(),
                    a6 as _,
                    a7.into(),
                )
                .await
            }
            Sys::PMT_UNPIN => self.sys_pmt_unpin(a0 as _),
            Sys::BTI_RELEASE_QUARANTINE => self.sys_bti_release_quarantine(a0 as _),
            Sys::VMAR_UNMAP_HANDLE_CLOSE_THREAD_EXIT => self
                .sys_vmar_unmap(a0 as _, a1 as _, a2 as _)
                .and_then(|_| {
                    let _ = self.sys_handle_close(a3 as _);
                    self.sys_thread_exit()
                }),
            Sys::FUTEX_WAKE_HANDLE_CLOSE_THREAD_EXIT => {
                // atomic_store_explicit(value_ptr, new_value, memory_order_release)
                // SMAP: single atomic store to user futex word.
                #[allow(unsafe_code)]
                {
                    unsafe {
                        hal_impl::user::smap_allow();
                        (*(a0 as *const AtomicI32)).store(a2 as i32, Ordering::Release);
                        hal_impl::user::smap_deny();
                    }
                }
                let _ = self.sys_futex_wake(a0.into(), a1 as _);
                let _ = self.sys_handle_close(a3 as _);
                self.sys_thread_exit()
            }
            Sys::OBJECT_GET_CHILD => {
                self.sys_object_get_child(a0 as _, a1 as _, a2 as _, a3.into())
            }
            Sys::PC_FIRMWARE_TABLES => self.sys_pc_firmware_tables(a0 as _, a1.into(), a2.into()),
            Sys::INTERRUPT_CREATE => {
                self.sys_interrupt_create(a0 as _, a1 as _, a2 as _, a3.into())
            }
            Sys::INTERRUPT_BIND => self.sys_interrupt_bind(a0 as _, a1 as _, a2 as _, a3 as _),
            Sys::INTERRUPT_TRIGGER => self.sys_interrupt_trigger(a0 as _, a1 as _, a2 as _),
            Sys::INTERRUPT_ACK => self.sys_interrupt_ack(a0 as _),
            Sys::INTERRUPT_DESTROY => self.sys_interrupt_destroy(a0 as _),
            Sys::INTERRUPT_WAIT => self.sys_interrupt_wait(a0 as _, a1.into()).await,
            Sys::EXCEPTION_GET_THREAD => self.sys_exception_get_thread(a0 as _, a1.into()),
            Sys::EXCEPTION_GET_PROCESS => self.sys_exception_get_process(a0 as _, a1.into()),
            Sys::IOPORTS_REQUEST => self.sys_ioports_request(a0 as _, a1 as _, a2 as _),
            Sys::IOPORTS_RELEASE => self.sys_ioports_release(a0 as _, a1 as _, a2 as _),
            #[cfg(feature = "hypervisor")]
            Sys::GUEST_CREATE => self.sys_guest_create(a0 as _, a1 as _, a2.into(), a3.into()),
            #[cfg(feature = "hypervisor")]
            Sys::GUEST_SET_TRAP => {
                self.sys_guest_set_trap(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _, a5 as _)
            }
            #[cfg(feature = "hypervisor")]
            Sys::VCPU_CREATE => self.sys_vcpu_create(a0 as _, a1 as _, a2 as _, a3.into()),
            #[cfg(feature = "hypervisor")]
            Sys::VCPU_ENTER => self.sys_vcpu_enter(a0 as _, a1.into()),
            #[cfg(feature = "hypervisor")]
            Sys::VCPU_INTERRUPT => self.sys_vcpu_interrupt(a0 as _, a1 as _),
            #[cfg(feature = "hypervisor")]
            Sys::VCPU_KICK => self.sys_vcpu_kick(a0 as _),
            #[cfg(feature = "hypervisor")]
            Sys::VCPU_READ_STATE => self.sys_vcpu_read_state(a0 as _, a1 as _, a2.into(), a3 as _),
            #[cfg(feature = "hypervisor")]
            Sys::VCPU_WRITE_STATE => self.sys_vcpu_write_state(a0 as _, a1 as _, a2, a3 as _),
            // Stubs for known but unimplemented syscalls
            Sys::OBJECT_SET_PROFILE => self.sys_object_set_profile(a0 as _, a1 as _, a2 as _),
            Sys::PROFILE_CREATE => self.sys_profile_create(a0 as _, a1 as _, a2.into(), a3.into()),
            Sys::VMAR_OP_RANGE => {
                self.sys_vmar_op_range(a0 as _, a1 as _, a2 as _, a3 as _, a4, a5)
                    .await
            }
            Sys::MSI_ALLOCATE => self.sys_msi_allocate(a0 as _, a1 as _, a2.into()),
            Sys::MSI_CREATE => {
                self.sys_msi_create(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _, a5.into())
            }
            Sys::MTRACE_CONTROL => {
                // Removed upstream.
                Err(ZxError::NOT_SUPPORTED)
            }
            Sys::SMC_CALL => self.sys_smc_call(a0 as _, a1, a2),
            Sys::DEBUG_SEND_COMMAND => {
                if !hal_impl::boot::cmdline()
                    .split_whitespace()
                    .any(|arg| arg == "kernel.enable-debugging-syscalls=true")
                {
                    Err(ZxError::NOT_SUPPORTED)
                } else {
                    self.sys_debug_send_command(a0 as _, a1.into(), a2 as _)
                }
            }
            Sys::SYSTEM_MEXEC => self.sys_system_mexec(a0 as _, a1 as _, a2 as _),
            Sys::SYSTEM_MEXEC_PAYLOAD_GET => {
                self.sys_system_mexec_payload_get(a0 as _, a1.into(), a2 as _)
            }
            Sys::SYSTEM_POWERCTL => self.sys_system_powerctl(a0 as _, a1 as _, a2),
            Sys::FRAMEBUFFER_GET_INFO | Sys::FRAMEBUFFER_SET_RANGE => {
                // Removed upstream -- replaced by display driver protocols.
                Err(ZxError::NOT_SUPPORTED)
            }
            Sys::INTERRUPT_BIND_VCPU => self.sys_interrupt_bind_vcpu(a0 as _, a1 as _, a2 as _),
            // Pager subsystem (demand paging)
            Sys::PAGER_CREATE => self.sys_pager_create(a0 as _, a1.into()),
            Sys::PAGER_CREATE_VMO => {
                self.sys_pager_create_vmo(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _, a5.into())
            }
            Sys::PAGER_DETACH_VMO => self.sys_pager_detach_vmo(a0 as _, a1 as _),
            Sys::PAGER_SUPPLY_PAGES => {
                self.sys_pager_supply_pages(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _, a5 as _)
            }
            Sys::PAGER_OP_RANGE => {
                self.sys_pager_op_range(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _, a5 as _)
            }
            // Kernel tracing
            // TODO: implement kernel trace ring buffer for syscall
            // entry/exit, context switches, and IRQ events
            Sys::KTRACE_READ => {
                self.sys_ktrace_read(a0 as _, a1.into(), a2 as _, a3 as _, a4.into())
            }
            Sys::KTRACE_CONTROL => self.sys_ktrace_control(a0 as _, a1 as _, a2 as _, a3),
            Sys::KTRACE_WRITE => {
                // Removed upstream.
                Err(ZxError::NOT_SUPPORTED)
            }
            Sys::SYSCALL_NEXT_1 => {
                // "Next" vDSO canary — always succeeds to indicate support.
                Ok(())
            }
            // --- Newer upstream Fuchsia syscalls ---
            Sys::IOB_CREATE => {
                self.sys_iob_create(a0 as _, a1.into(), a2 as _, a3.into(), a4.into())
            }
            Sys::IOB_WRITEV => self.sys_iob_writev(a0 as _, a1 as _, a2 as _, a3.into(), a4 as _),
            Sys::IOB_ALLOCATE_ID => {
                self.sys_iob_allocate_id(a0 as _, a1 as _, a2 as _, a3, a4 as _, a5.into())
            }
            Sys::IOB_CREATE_SHARED_REGION => {
                self.sys_iob_create_shared_region(a0 as _, a1 as _, a2.into())
            }
            Sys::COUNTER_CREATE => self.sys_counter_create(a0 as _, a1.into()),
            Sys::COUNTER_READ => self.sys_counter_read(a0 as _, a1.into()),
            Sys::COUNTER_WRITE => self.sys_counter_write(a0 as _, a1 as _),
            Sys::COUNTER_ADD => self.sys_counter_add(a0 as _, a1 as _),
            Sys::SAMPLER_CREATE => self.sys_sampler_create(a0 as _, a1 as _, a2, a3.into()),
            Sys::SAMPLER_START => self.sys_sampler_start(a0 as _),
            Sys::SAMPLER_STOP => self.sys_sampler_stop(a0 as _),
            Sys::SAMPLER_READ => self.sys_sampler_read(a0 as _, a1.into(), a2 as _, a3.into()),
            Sys::MEMBARRIER_SYNC_PROCESS_DATA => self.sys_membarrier_sync_process_data(),
            Sys::MEMBARRIER_SYNC_PROCESS_INSN => self.sys_membarrier_sync_process_insn(),
            Sys::RESTRICTED_ENTER => {
                // restricted_enter does not return normally on success.
                // On success, it returns the `context` argument as the
                // raw return value (placed in x0/rdi/a0), which the
                // normal-mode handler reads as its first argument.
                return match self.sys_restricted_enter(a0 as _, a1, a2) {
                    Ok(()) => a2 as isize, // context arg
                    Err(err) => err as isize,
                };
            }
            Sys::RESTRICTED_BIND_STATE => self.sys_restricted_bind_state(a0 as _, a1.into()),
            Sys::RESTRICTED_KICK => self.sys_restricted_kick(a0 as _, a1 as _),
            Sys::RESTRICTED_UNBIND_STATE => self.sys_restricted_unbind_state(a0 as _),
            Sys::CACHE_FLUSH => self.sys_cache_flush(a0, a1, a2 as _),
            // --- Additional upstream syscalls (stubs) ---
            Sys::THREAD_LEGACY_YIELD => {
                // Yield CPU. Options (a0) must be 0.
                if a0 != 0 {
                    Err(ZxError::INVALID_ARGS)
                } else {
                    Ok(())
                }
            }
            Sys::THREAD_RAISE_EXCEPTION => {
                self.sys_thread_raise_exception(a0 as _, a1 as _, a2).await
            }
            Sys::THREAD_SET_RSEQ => self.sys_thread_set_rseq(a0 as _, a1 as _, a2 as _),
            Sys::PROCESS_CREATE_SHARED => self.sys_process_create_shared(
                a0 as _,
                a1 as _,
                a2.into(),
                a3 as _,
                a4.into(),
                a5.into(),
            ),
            Sys::PORT_CANCEL_KEY => self.sys_port_cancel_key(a0 as _, a1 as _, a2 as _),
            Sys::VMO_GET_STREAM_SIZE => self.sys_vmo_get_stream_size(a0 as _, a1.into()),
            Sys::VMO_SET_STREAM_SIZE => self.sys_vmo_set_stream_size(a0 as _, a1),
            Sys::VMO_TRANSFER_DATA => {
                self.sys_vmo_transfer_data(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _, a5 as _)
            }
            Sys::VMAR_MAP_CLOCK => {
                self.sys_vmar_map_clock(a0 as _, a1 as _, a2 as _, a3 as _, a4 as _, a5.into())
            }
            Sys::VMAR_MAP_IOB => self.sys_vmar_map_iob(
                a0 as _,
                a1 as _,
                a2 as _,
                a3 as _,
                a4 as _,
                a5 as _,
                a6.into(),
            ),
            Sys::PAGER_QUERY_DIRTY_RANGES => self.sys_pager_query_dirty_ranges(
                a0 as _,
                a1 as _,
                a2 as _,
                a3 as _,
                a4,
                a5,
                a6.into(),
                a7.into(),
            ),
            Sys::PAGER_QUERY_VMO_STATS => {
                self.sys_pager_query_vmo_stats(a0 as _, a1 as _, a2 as _, a3, a4)
            }
            Sys::SYSTEM_GET_PERFORMANCE_INFO => {
                self.sys_system_get_performance_info(a0 as _, a1 as _, a2 as _, a3, a4.into())
            }
            Sys::SYSTEM_SET_PERFORMANCE_INFO => {
                self.sys_system_set_performance_info(a0 as _, a1 as _, a2, a3 as _)
            }
            Sys::SYSTEM_SUSPEND_ENTER => {
                self.sys_system_suspend_enter(a0 as _, a1 as _, a2 as _, a3, a4, a5 as _, a6.into())
                    .await
            }
            Sys::SYSTEM_WATCH_MEMORY_STALL => {
                self.sys_system_watch_memory_stall(a0 as _, a1 as _, a2 as _, a3 as _, a4.into())
            }
            Sys::HANDLE_CHECK_VALID => self.sys_handle_check_valid(a0 as _),
            Sys::UTC_REFERENCE_SWAP => self.sys_utc_reference_swap(a0 as _, a1.into()),
            Sys::UTC_REFERENCE_GET => {
                return self.thread.proc().utc_reference_get() as isize;
            }
            // Test-only syscalls.
            Sys::SYSCALL_TEST_HANDLE_CREATE => {
                return self
                    .sys_syscall_test_handle_create(a0 as i32, a1.into())
                    .await;
            }
            Sys::SYSCALL_TEST_RUST_HANDLE => self.sys_syscall_test_rust_handle(a0 as _, a1.into()),
            Sys::SYSCALL_TEST_RUST_INPTR => self.sys_syscall_test_rust_inptr(a0.into(), a1.into()),
            Sys::SYSCALL_TEST_RUST_OUTPTR => self.sys_syscall_test_rust_outptr(a0 as _, a1.into()),
            Sys::SYSCALL_TEST_RUST_INOUTPTR => self.sys_syscall_test_rust_inoutptr(a0.into()),
            // Test-only syscalls: return the sum of their arguments.
            // Used by SyscallGenerationTest to verify the syscall ABI.
            Sys::SYSCALL_TEST_0 | Sys::SYSCALL_TEST_RUST_0 => {
                return 0;
            }
            // Wrapper variants: 3-arg sum with validation.
            // Reject negative first arg (INVALID_ARGS), reject sum > 50 (OUT_OF_RANGE).
            Sys::SYSCALL_TEST_WRAPPER | Sys::SYSCALL_TEST_RUST_WRAPPER => {
                let a = a0 as i32;
                let b = a1 as i32;
                let c = a2 as i32;
                if a < 0 {
                    return ZxError::INVALID_ARGS as isize;
                }
                let sum = (a as i64) + (b as i64) + (c as i64);
                if sum > 50 {
                    return ZxError::OUT_OF_RANGE as isize;
                }
                return sum as isize;
            }
            Sys::SYSCALL_TEST_1
            | Sys::SYSCALL_TEST_2
            | Sys::SYSCALL_TEST_3
            | Sys::SYSCALL_TEST_4
            | Sys::SYSCALL_TEST_5
            | Sys::SYSCALL_TEST_6
            | Sys::SYSCALL_TEST_7
            | Sys::SYSCALL_TEST_8
            | Sys::SYSCALL_TEST_RUST_1
            | Sys::SYSCALL_TEST_RUST_2
            | Sys::SYSCALL_TEST_RUST_3
            | Sys::SYSCALL_TEST_RUST_4
            | Sys::SYSCALL_TEST_RUST_5
            | Sys::SYSCALL_TEST_RUST_6
            | Sys::SYSCALL_TEST_RUST_7
            | Sys::SYSCALL_TEST_RUST_8 => {
                let args: [isize; 8] = [
                    a0 as isize,
                    a1 as isize,
                    a2 as isize,
                    a3 as isize,
                    a4 as isize,
                    a5 as isize,
                    a6 as isize,
                    a7 as isize,
                ];
                let n = match sys_type {
                    Sys::SYSCALL_TEST_1 | Sys::SYSCALL_TEST_RUST_1 => 1,
                    Sys::SYSCALL_TEST_2 | Sys::SYSCALL_TEST_RUST_2 => 2,
                    Sys::SYSCALL_TEST_3 | Sys::SYSCALL_TEST_RUST_3 => 3,
                    Sys::SYSCALL_TEST_4 | Sys::SYSCALL_TEST_RUST_4 => 4,
                    Sys::SYSCALL_TEST_5 | Sys::SYSCALL_TEST_RUST_5 => 5,
                    Sys::SYSCALL_TEST_6 | Sys::SYSCALL_TEST_RUST_6 => 6,
                    Sys::SYSCALL_TEST_7 | Sys::SYSCALL_TEST_RUST_7 => 7,
                    Sys::SYSCALL_TEST_8 | Sys::SYSCALL_TEST_RUST_8 => 8,
                    _ => unreachable!(),
                };
                return args[..n].iter().sum();
            }
            // Widening test syscalls: sum args with type truncation.
            // The "wide" variant receives full 64-bit args; the kernel
            // truncates to the declared types before summing.
            // The "narrow" variant is truncated by the vDSO before the
            // kernel call, so the kernel sees already-truncated values.
            Sys::SYSCALL_TEST_WIDENING_UNSIGNED_NARROW
            | Sys::SYSCALL_TEST_WIDENING_UNSIGNED_WIDE => {
                // Args: u64, u32, u16, u8 (kernel truncates for _WIDE)
                let sum = (a0 as u64)
                    .wrapping_add(a1 as u32 as u64)
                    .wrapping_add(a2 as u16 as u64)
                    .wrapping_add(a3 as u8 as u64);
                return sum as isize;
            }
            Sys::SYSCALL_TEST_WIDENING_SIGNED_NARROW | Sys::SYSCALL_TEST_WIDENING_SIGNED_WIDE => {
                // Args: i64, i32, i16, i8 (kernel truncates for _WIDE)
                let sum = (a0 as i64)
                    .wrapping_add(a1 as i32 as i64)
                    .wrapping_add(a2 as i16 as i64)
                    .wrapping_add(a3 as i8 as i64);
                return sum as isize;
            }
            _ => {
                error!("syscall unimplemented: {:?}", sys_type);
                Err(ZxError::NOT_SUPPORTED)
            }
        };
        // Deliver any pending deferred policy exception before returning
        // to userspace, matching Fuchsia's ProcessPendingSignals check
        // for THREAD_SIGNAL_POLICY_EXCEPTION.
        self.thread.deliver_pending_policy_exception().await;

        // Log debug I/O syscalls at trace level to avoid flooding the
        // serial console during interactive shell sessions.
        // Log errors at error level, success at info/trace.
        match (&ret, &sys_type) {
            (_, Sys::DEBUG_WRITE | Sys::DEBUG_READ) => {
                trace!("{}|{} {:?} <= {:?}", proc_name, thread_name, sys_type, ret);
            }
            (Err(e), Sys::VMAR_MAP) => {
                error!(
                    "{}|{} VMAR_MAP({:#x},{:#x},{:#x},{:#x},{:#x},{:#x}) <= {:?}",
                    proc_name, thread_name, a0, a1, a2, a3, a4, a5, e
                );
            }
            (Err(_), _) => {
                error!("{}|{} {:?} <= {:?}", proc_name, thread_name, sys_type, ret);
            }
            _ => {
                info!("{}|{} {:?} <= {:?}", proc_name, thread_name, sys_type, ret);
            }
        }
        match ret {
            Ok(_) => 0,
            Err(err) => err as isize,
        }
    }
}
