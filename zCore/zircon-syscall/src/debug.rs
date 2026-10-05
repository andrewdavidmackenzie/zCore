use alloc::string::String;
use core::fmt::Write;

use super::*;
use zircon_object::dev::*;
use zircon_object::task::{Flavour, Job, Process, ThreadState};

impl Syscall<'_> {
    /// Write debug info to the serial port.
    pub fn sys_debug_write(&self, buf: UserInPtr<u8>, len: usize) -> ZxResult {
        trace!("debug.write: buf=({:?}; {:#x})", buf, len);
        hal_impl::console::console_write_str(&buf.read_string(len)?);
        Ok(())
    }

    /// Read debug info from the serial port.
    pub async fn sys_debug_read(
        &self,
        handle: HandleValue,
        mut buf: UserOutPtr<u8>,
        buf_size: u32,
        mut actual: UserOutPtr<u32>,
    ) -> ZxResult {
        trace!(
            "debug.read: handle={:#x}, buf=({:?}; {:#x})",
            handle,
            buf,
            buf_size
        );
        let proc = self.thread.proc();
        proc.get_resource(handle)?.validate(ResourceKind::ROOT)?;
        let mut vec = vec![0u8; buf_size as usize];
        let len = hal_impl::console::console_read(&mut vec).await;
        buf.write_array(&vec[..len])?;
        actual.write(len as u32)?;
        Ok(())
    }

    /// Execute a program from the rootfs by path.
    ///
    /// Reads the binary, detects its flavour from the ELF header,
    /// spawns it, and waits for it to terminate before returning.
    pub async fn sys_debug_exec(&self, path_ptr: UserInPtr<u8>, path_len: usize) -> ZxResult {
        if path_len == 0 || path_len > 256 {
            return Err(ZxError::INVALID_ARGS);
        }
        let path = path_ptr.read_string(path_len)?;
        if !path.starts_with('/') {
            return Err(ZxError::INVALID_ARGS);
        }
        debug!("debug.exec: path={:?}", path);

        // Read the binary from the rootfs.
        let data = zircon_object::task::spawn::read_rootfs_file(&path).ok_or(ZxError::NOT_FOUND)?;

        let flavour = Flavour::from_elf(&data);
        debug!("debug.exec: detected flavour {:?}", flavour);

        // Spawn by flavour — unified dispatch.
        let job = self.thread.proc().job();
        let proc = zircon_object::task::spawn::spawn_by_flavour(flavour, &job, &path, &data)?;

        // Wait for the process to terminate. Use timed sleeps to
        // yield the executor — wait_signal doesn't reliably wake
        // across executor tasks on different CPUs.
        // Wait for the process to terminate. Use timed sleeps to
        // yield the executor — wait_signal doesn't reliably wake
        // across executor tasks on different CPUs.
        while proc.exit_code().is_none() {
            hal_impl::thread::sleep_until(
                hal_impl::timer::timer_now() + core::time::Duration::from_millis(1),
            )
            .await;
        }
        Ok(())
    }

    /// Send a command string to the kernel debug console.
    ///
    /// Implements `zx_debug_send_command`. The resource handle must be
    /// a system resource with `ZX_RSRC_SYSTEM_DEBUG_BASE` access, or the
    /// root resource. The command string is parsed and dispatched to
    /// built-in handlers.
    ///
    /// Supported commands:
    /// - `help` — list available commands
    /// - `ps` — list all processes in the job tree
    /// - `threads` — list all threads grouped by process
    /// - `kill <koid>` — kill a process by its kernel object ID
    pub fn sys_debug_send_command(
        &self,
        resource: HandleValue,
        buf: UserInPtr<u8>,
        buf_size: usize,
    ) -> ZxResult {
        trace!(
            "debug.send_command: resource={:#x}, buf=({:?}; {:#x})",
            resource,
            buf,
            buf_size
        );
        if buf_size == 0 || buf_size > 1024 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let res = proc.get_resource(resource)?;
        // Accept root resource or a SYSTEM resource covering DEBUG_BASE.
        if res.validate(ResourceKind::ROOT).is_err() {
            res.validate_ranged_resource(ResourceKind::SYSTEM, ZX_RSRC_SYSTEM_DEBUG_BASE, 1)?;
        }

        let cmd_str = buf.read_string(buf_size)?;
        let cmd = cmd_str.trim();

        // Split into command and arguments.
        let (verb, args) = match cmd.split_once(char::is_whitespace) {
            Some((v, a)) => (v, a.trim()),
            None => (cmd, ""),
        };

        match verb {
            "help" => {
                let output = "Available kernel debug commands:\n  \
                              help       — show this help\n  \
                              ps         — list all processes\n  \
                              threads    — list all threads by process\n  \
                              kill <pid> — kill a process by koid\n";
                hal_impl::console::console_write_str(output);
                Ok(())
            }
            "ps" => {
                self.debug_cmd_ps();
                Ok(())
            }
            "threads" => {
                self.debug_cmd_threads();
                Ok(())
            }
            "kill" => self.debug_cmd_kill(args),
            _ => {
                let mut msg = String::new();
                let _ = writeln!(msg, "unknown debug command: '{}'", verb);
                hal_impl::console::console_write_str(&msg);
                Err(ZxError::INVALID_ARGS)
            }
        }
    }

    /// Walk to the root job from the current thread.
    fn root_job(&self) -> Arc<Job> {
        let mut job = self.thread.proc().job();
        while let Some(parent) = job.parent() {
            job = parent;
        }
        job
    }

    /// `ps` — list all processes in the job tree.
    fn debug_cmd_ps(&self) {
        let root = self.root_job();
        let mut out = String::with_capacity(512);
        let _ = writeln!(
            out,
            "{:<8} {:<8} {:<10} {:<6} NAME",
            "KOID", "PARENT", "STATUS", "#THR"
        );
        Self::collect_ps(&root, &mut out);
        hal_impl::console::console_write_str(&out);
    }

    /// Recursively collect process info from a job and its children.
    fn collect_ps(job: &Job, out: &mut String) {
        let job_id = job.id();
        for pid in job.process_ids() {
            if let Ok(obj) = job.get_child(pid) {
                if let Ok(proc) = obj.downcast_arc::<Process>() {
                    let name = proc.name();
                    let status = proc.status();
                    let n_threads = proc.thread_ids().len();
                    let status_str = match status {
                        zircon_object::task::Status::Init => "init",
                        zircon_object::task::Status::Running => "running",
                        zircon_object::task::Status::Exited(_) => "exited",
                    };
                    let _ = writeln!(
                        out,
                        "{:<8} {:<8} {:<10} {:<6} {}",
                        pid, job_id, status_str, n_threads, name
                    );
                }
            }
        }
        // Recurse into child jobs.
        for child_id in job.children_ids() {
            if let Ok(child_obj) = job.get_child(child_id) {
                if let Ok(child_job) = child_obj.downcast_arc::<Job>() {
                    Self::collect_ps(&child_job, out);
                }
            }
        }
    }

    /// `threads` — list all threads grouped by process.
    fn debug_cmd_threads(&self) {
        let root = self.root_job();
        let mut out = String::with_capacity(512);
        Self::collect_threads(&root, &mut out);
        hal_impl::console::console_write_str(&out);
    }

    /// Recursively collect thread info from all processes in a job tree.
    fn collect_threads(job: &Job, out: &mut String) {
        for pid in job.process_ids() {
            if let Ok(obj) = job.get_child(pid) {
                if let Ok(proc) = obj.downcast_arc::<Process>() {
                    let _ = writeln!(out, "process {} '{}':", proc.id(), proc.name());
                    for tid in proc.thread_ids() {
                        if let Ok(tobj) = proc.get_child(tid) {
                            let name = tobj.name();
                            // Thread state from ThreadInfo.
                            let state_str = if let Ok(thread) =
                                tobj.downcast_arc::<zircon_object::task::Thread>()
                            {
                                match thread.state() {
                                    ThreadState::New => "new",
                                    ThreadState::Running => "running",
                                    ThreadState::Suspended => "suspended",
                                    ThreadState::Blocked
                                    | ThreadState::BlockedException
                                    | ThreadState::BlockedSleeping
                                    | ThreadState::BlockedFutex
                                    | ThreadState::BlockedPort
                                    | ThreadState::BlockedChannel
                                    | ThreadState::BlockedWaitOne
                                    | ThreadState::BlockedWaitMany
                                    | ThreadState::BlockedInterrupt
                                    | ThreadState::BlockedPager => "blocked",
                                    ThreadState::Dying => "dying",
                                    ThreadState::Dead => "dead",
                                }
                            } else {
                                "unknown"
                            };
                            let _ = writeln!(out, "  thread {} '{}' [{}]", tid, name, state_str);
                        }
                    }
                }
            }
        }
        // Recurse into child jobs.
        for child_id in job.children_ids() {
            if let Ok(child_obj) = job.get_child(child_id) {
                if let Ok(child_job) = child_obj.downcast_arc::<Job>() {
                    Self::collect_threads(&child_job, out);
                }
            }
        }
    }

    /// `kill <koid>` — kill a process by its kernel object ID.
    fn debug_cmd_kill(&self, args: &str) -> ZxResult {
        let koid: KoID = args.parse().map_err(|_| {
            hal_impl::console::console_write_str("usage: kill <koid>\n");
            ZxError::INVALID_ARGS
        })?;

        let root = self.root_job();
        if Self::kill_process_by_koid(&root, koid) {
            let mut msg = String::new();
            let _ = writeln!(msg, "killed process {}", koid);
            hal_impl::console::console_write_str(&msg);
            Ok(())
        } else {
            let mut msg = String::new();
            let _ = writeln!(msg, "process {} not found", koid);
            hal_impl::console::console_write_str(&msg);
            Err(ZxError::NOT_FOUND)
        }
    }

    /// Search the job tree for a process with the given koid and kill it.
    fn kill_process_by_koid(job: &Job, koid: KoID) -> bool {
        use zircon_object::task::Task;
        for pid in job.process_ids() {
            if pid == koid {
                if let Ok(obj) = job.get_child(pid) {
                    if let Ok(proc) = obj.downcast_arc::<Process>() {
                        proc.kill();
                        return true;
                    }
                }
            }
        }
        for child_id in job.children_ids() {
            if let Ok(child_obj) = job.get_child(child_id) {
                if let Ok(child_job) = child_obj.downcast_arc::<Job>() {
                    if Self::kill_process_by_koid(&child_job, koid) {
                        return true;
                    }
                }
            }
        }
        false
    }
}
