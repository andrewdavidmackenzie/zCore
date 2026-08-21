use super::*;
use core::fmt::Debug;
use core::mem::size_of;

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use bitflags::bitflags;

use kernel_hal::context::UserContextField;
use linux_object::thread::{CurrentThreadExt, RobustList, ThreadExt};
use linux_object::time::TimeSpec;
use linux_object::{fs::INodeExt, loader::LinuxElfLoader};
use zircon_object::vm::USER_STACK_PAGES;

/// Syscalls for process.
///
/// # Menu
///
/// - [`fork`](Self::sys_fork)
/// - [`vfork`](Self::sys_vfork)
/// - [`clone`](Self::sys_clone)
/// - [`wait4`](Self::sys_wait4)
/// - [`execve`](Self::sys_execve)
/// - [`gettid`](Self::sys_gettid)
/// - [`getpid`](Self::sys_getpid)
/// - [`getppid`](Self::sys_getppid)
/// - [`exit`](Self::sys_exit)
/// - [`exit_group`](Self::sys_exit_group)
/// - [`nanosleep`](Self::sys_nanosleep)
/// - [`set_tid_address`](Self::sys_set_tid_address)
impl Syscall<'_> {
    /// `fork` creates a new process by duplicating the calling process
    /// (see [linux man fork(2)](https://www.man7.org/linux/man-pages/man2/fork.2.html)).
    /// The new process is referred to as the child process.
    /// The calling process is referred to as the parent process.
    ///
    /// The child process and the parent process run in separate memory spaces.
    /// At the time of `fork` both memory spaces have the same content.
    /// Memory writes, file mappings ([`Self::sys_mmap`]) and unmappings ([`Self::sys_munmap`])
    /// performed by one of the processes do not affect the other.
    ///
    /// The child process is an exact duplicate of the parent process except for the following points:
    ///
    /// - The child has its own unique process ID, and this PID does not match the ID of any existing process.
    /// - The child's parent process ID is the same as the parent's process ID.
    /// - Process resource utilizations ([`Self::sys_getrusage`]) and CPU time counters ([`Self::sys_times`]) are reset to zero in the child.
    /// - The child does not inherit semaphore adjustments from its parent ([`Self::sys_semop`]).
    /// - The child does not inherit process-associated record locks from its parent ([`Self::sys_fcntl`]).
    ///   (On the other hand, it does inherit [`Self::sys_fcntl`] open file description locks and [`Self::sys_flock`] locks from its parent.)
    ///
    /// Note the following further points:
    ///
    /// - The child process is created with a single thread—the one that called fork().
    ///   The entire virtual address space of the parent is replicated in the child,
    ///   including the states of mutexes and condition variables.
    /// - After a `fork` in a multithreaded program,
    ///   the child can safely call only async-signal-safe functions
    ///   until such time as it calls [`Self::sys_execve`].
    /// - The child inherits copies of the parent's set of open file descriptors.
    ///   Each file descriptor in the child refers to the same open file description (see [`Self::sys_open`])
    ///   as the corresponding file descriptor in the parent.
    ///   This means that the two file descriptors share open file status flags and file offset.
    pub fn sys_fork(&self) -> SysResult {
        info!("fork:");
        let new_proc = Process::fork_from(self.zircon_process(), false)?; // old pt NULL here
        let new_thread = Thread::create_linux(&new_proc)?;
        let mut new_ctx = self.thread.context_cloned()?;
        new_ctx.set_field(UserContextField::ReturnValue, 0);
        new_thread.with_context(|ctx| *ctx = new_ctx)?;
        new_thread.start(self.thread_fn)?;
        info!("fork: {} -> {}", self.zircon_process().id(), new_proc.id());
        Ok(new_proc.id() as usize)
    }

    /// `sys_vfork`, just like [`Self::sys_fork`], creates a child process of the calling process
    /// (see [linux man vfork(2)](https://www.man7.org/linux/man-pages/man2/vfork.2.html)).
    /// For details, see [`Self::sys_fork`].
    ///
    /// `sys_vfork` differs from [`Self::sys_fork`] in that the calling thread is suspended until the child terminates
    /// (either normally, by calling [`Self::sys_exit`], or abnormally, after delivery of a fatal signal),
    /// or it makes a call to [`Self::sys_execve`].
    pub async fn sys_vfork(&self) -> SysResult {
        info!("vfork:");
        let new_proc = Process::fork_from(self.zircon_process(), true)?;
        let new_thread = Thread::create_linux(&new_proc)?;
        let mut new_ctx = self.thread.context_cloned()?;
        new_ctx.set_field(UserContextField::ReturnValue, 0);
        new_thread.with_context(|ctx| *ctx = new_ctx)?;
        new_thread.start(self.thread_fn)?;

        let new_proc: Arc<dyn KernelObject> = new_proc;
        info!(
            "vfork: {} -> {}. Waiting for execve or exit",
            self.zircon_process().id(),
            new_proc.id()
        );
        new_proc.wait_signal(Signal::SIGNALED).await; // wait for execve
        Ok(new_proc.id() as usize)
    }

    /// `sys_clone` create a new thread in the current process.
    /// The new thread's stack pointer will be set to `newsp`,
    /// and thread pointer will be set to `newtls`.
    /// The child TID will be stored at both `parent_tid` and `child_tid`.
    ///
    /// > **NOTE!** This system call is not exactly the same as `clone` in Linux.
    ///
    /// > **NOTE!** This is partially implemented for `musl` only.
    ///
    /// `sys_clone` create a new thread or process.
    pub fn sys_clone(
        &self,
        flags: usize,
        newsp: usize,
        mut parent_tid: UserOutPtr<i32>,
        newtls: usize,
        mut child_tid: UserOutPtr<i32>,
    ) -> SysResult {
        let clone_flags = CloneFlags::from_bits_truncate(flags);
        info!(
            "clone: flags={:#x}, newsp={:#x}, parent_tid={:?}, child_tid={:?}, newtls={:#x}",
            flags, newsp, parent_tid, child_tid, newtls
        );

        // Fork-like clone: no CLONE_VM, or CLONE_VFORK, or no CLONE_THREAD.
        if !clone_flags.contains(CloneFlags::VM)
            || clone_flags.contains(CloneFlags::VFORK)
            || !clone_flags.contains(CloneFlags::THREAD)
        {
            info!(
                "sys_clone: fork-like flags {:#x}, falling back to fork",
                flags
            );
            return self.sys_fork();
        }

        // Thread creation: requires CLONE_VM at minimum.
        // Accept any flag combination that includes CLONE_VM and create
        // a new thread in the current process.
        let new_thread = Thread::create_linux(self.zircon_process())?;
        let mut new_ctx = self.thread.context_cloned()?;
        if newsp != 0 {
            new_ctx.set_field(UserContextField::StackPointer, newsp);
        }
        if clone_flags.contains(CloneFlags::SETTLS) {
            new_ctx.set_field(UserContextField::ThreadPointer, newtls);
        }
        new_ctx.set_field(UserContextField::ReturnValue, 0);
        new_thread.with_context(|ctx| *ctx = new_ctx)?;
        new_thread.start(self.thread_fn)?;

        let tid = new_thread.id();
        info!("clone: {} -> {}", self.thread.id(), tid);
        if clone_flags.contains(CloneFlags::PARENT_SETTID) {
            parent_tid.write(tid as i32)?;
        }
        if clone_flags.contains(CloneFlags::CHILD_SETTID)
            || clone_flags.contains(CloneFlags::CHILD_CLEARTID)
        {
            child_tid.write(tid as i32)?;
            new_thread.set_tid_address(child_tid);
        }
        Ok(tid as usize)
    }

    /// `sys_wait4` suspends execution of the calling thread
    /// until a child specified by `pid` argument has changed state
    /// (see [linux man wait4(2)](https://www.man7.org/linux/man-pages/man2/wait4.2.html)).
    /// By default, `sys_wait4` waits only for terminated children,
    /// but this behavior is modifiable via the options argument, as described below.
    ///
    /// The value of `pid` can be:
    ///
    /// - **-1**: meaning wait for any child process.
    /// - **0**: meaning wait for any child in the calling process's group.
    ///   **Note:** `sys_wait4` does not yet filter by process group,
    ///   so `pid == 0` is treated the same as `-1` (wait for any child).
    /// - **>0**: meaning wait for the child whose process ID is equal to the value of `pid`.
    ///
    /// The value of options is an OR of zero or more of the following constants:
    ///
    /// - **NOHANG**    = 0x000_0001;
    ///
    ///   Return immediately if no child has exited.
    ///
    /// - **STOPPED**   = 0x000_0002;
    ///
    ///   Also report children stopped by a signal (not yet implemented).
    ///
    /// - **EXITED**    = 0x000_0004;
    ///
    ///   Wait for children that have terminated.
    ///
    /// - **CONTINUED** = 0x000_0008;
    ///
    ///   Also report stopped children that have been resumed by `SIGCONT` (not yet implemented).
    ///
    /// - **NOWAIT**    = 0x100_0000;
    ///
    ///   Leave the child in a waitable state; a later wait call can be used to again retrieve the status (not yet implemented).
    ///
    /// On success, returns the process ID of the child whose state has changed;
    /// if `NOHANG` flag was specified and one or more child(ren) specified by pid exist,
    /// but have not yet changed state, then 0 is returned.
    /// On failure, -1 is returned.
    pub async fn sys_wait4(
        &self,
        pid: i32,
        mut wstatus: UserOutPtr<i32>,
        options: u32,
    ) -> SysResult {
        #[derive(Debug)]
        enum WaitTarget {
            AnyChild,
            Pid(KoID),
        }
        bitflags! {
            struct WaitFlags: u32 {
                const NOHANG    = 1;
                const STOPPED   = 2;
                const EXITED    = 4;
                const CONTINUED = 8;
                const NOWAIT    = 0x100_0000;
            }
        }
        let target = match pid {
            -1 => WaitTarget::AnyChild,
            // pid=0: wait for any child in process group. Since zCore has
            // no process groups, treat the same as pid=-1 (any child).
            0 => WaitTarget::AnyChild,
            p if p > 0 => WaitTarget::Pid(p as KoID),
            _ => {
                warn!("wait4: process group wait (pid={}) not implemented", pid);
                return Err(LxError::ECHILD);
            }
        };
        let flags = WaitFlags::from_bits_truncate(options);
        let nohang = flags.contains(WaitFlags::NOHANG);
        info!(
            "wait4: target={:?}, wstatus={:?}, options={:?}",
            target, wstatus, flags,
        );
        let result = match target {
            WaitTarget::AnyChild => {
                wait_child_any(self.zircon_process(), nohang, self.thread).await
            }
            WaitTarget::Pid(pid) => wait_child(self.zircon_process(), pid, nohang, self.thread)
                .await
                .map(|code| (pid, code)),
        };
        match result {
            Ok((pid, code)) => {
                // Encode as Linux wait status: exited = (code & 0xff) << 8
                // so WIFEXITED(status) and WEXITSTATUS(status) work correctly.
                let status = (code & 0xff) << 8;
                wstatus.write_if_not_null(status)?;
                Ok(pid as usize)
            }
            // WNOHANG: children exist but none have changed state — return 0
            Err(LxError::EAGAIN) if nohang => Ok(0),
            Err(e) => Err(e),
        }
    }

    /// `sys_execve` executes the program referred to by `path`
    /// (see [linux man execve(2)](https://www.man7.org/linux/man-pages/man2/execve.2.html)).
    /// This causes the program that is currently being run
    /// by the calling process to be replaced with a new program,
    /// with newly initialized stack, heap, and (initialized and uninitialized) data segments.
    ///
    /// `path` argument must be a binary executable file.
    ///
    /// `argv` is an array of argument strings passed to the new program.
    /// By convention, the first of these strings (i.e., `argv[0]`)
    /// should contain the filename associated with the file being executed.
    ///
    /// `envp` is an array of strings, conventionally of the form `key=value`,
    /// which are passed as environment to the new program.
    ///
    /// > **NOTE!** Differ from linux, `argv` & `envp` can not be NULL.
    ///
    /// > **NOTE!** For multi-thread programs, POSIX requires that all sibling
    /// > threads are terminated before the new image is loaded. zCore does not
    /// > yet explicitly terminate sibling threads during exec; it only clears
    /// > the shared VMAR.
    pub fn sys_execve(
        &mut self,
        path: UserInPtr<u8>,
        argv: UserInPtr<UserInPtr<u8>>,
        envp: UserInPtr<UserInPtr<u8>>,
    ) -> SysResult {
        let path = path.as_c_str()?;
        let args = argv.read_cstring_array()?;
        let mut envs: Vec<String> = Vec::new();
        if !envp.is_null() {
            envs = envp.read_cstring_array()?;
        }
        info!(
            "execve: path: {:?}, argv: {:?}, envs: {:?}",
            path, argv, envs
        );
        if args.is_empty() {
            error!("execve: args is null");
            return Err(LxError::EINVAL);
        }

        // Note: in a multi-threaded process, sibling threads should be
        // killed before loading the new program. Currently Process::exit()
        // handles thread cleanup, but execve does not explicitly kill
        // siblings — this is safe as long as vmar.clear() below unmaps
        // the old address space they share.

        // Read program file
        let proc = self.linux_process();
        let inode = proc.lookup_inode(path)?;
        let data = inode.read_as_vec()?;

        proc.remove_cloexec_files();

        // WARNING: About to destroy the old application's user space.
        // Copy all necessary information into kernel space now!
        let path = path.to_string();
        let vmar = self.zircon_process().vmar();
        vmar.clear()?;

        // Modify exec path
        proc.set_execute_path(&path);

        let (entry, sp, initial_brk) = LinuxElfLoader {
            syscall_entry: self.syscall_entry,
            stack_pages: USER_STACK_PAGES,
            root_inode: proc.root_inode().clone(),
        }
        .load(&vmar, &data, args, envs, path)?;
        proc.set_brk(initial_brk);

        // Reset signal dispositions on exec (SIG_IGN preserved, others to SIG_DFL)
        proc.reset_signal_actions_on_exec();

        // Note: signal_set(SIGNALED) to release vfork parent is intentionally
        // omitted. Setting SIGNALED on the child process persists across the
        // process lifetime and causes regressions in subsequent fork/exec
        // operations. The vfork parent resumes via other mechanisms (child
        // exit sets PROCESS_TERMINATED, or the busybox shell doesn't use
        // true vfork semantics).

        self.thread
            .with_context(|ctx| ctx.setup_uspace(entry, sp, &[0, 0, 0]))?;
        Ok(0)
    }

    //    pub fn sys_yield(&self) -> SysResult {
    //        thread::yield_now();
    //        Ok(0)
    //    }
    //

    /// `sys_gettid` returns the caller's thread ID (TID)
    /// (see [linux man gettid(2)](https://www.man7.org/linux/man-pages/man2/gettid.2.html)).
    /// In a single-threaded process, the thread ID is equal to the process ID (PID, as returned by [`Self::sys_getpid`]).
    /// In a multithreaded process, all threads have the same PID, but each one has a unique TID.
    pub fn sys_gettid(&self) -> SysResult {
        info!("gettid:");
        let tid = self.thread.id();
        Ok(tid as usize)
    }

    /// `sys_getpid` returns the process ID (PID) of the calling process
    /// (see [linux man getpid(2)](https://www.man7.org/linux/man-pages/man2/getpid.2.html)).
    pub fn sys_getpid(&self) -> SysResult {
        info!("getpid:");
        let proc = self.zircon_process();
        let pid = proc.id();
        Ok(pid as usize)
    }

    /// `sys_getppid` returns the process ID of the parent of the calling process
    /// (see [linux man getppid(2)](https://www.man7.org/linux/man-pages/man2/getpid.2.html)).
    /// This will be either the ID of the process that created this process using fork(),
    /// or, if that process has already terminated, 0.
    pub fn sys_getppid(&self) -> SysResult {
        info!("getppid:");
        let proc = self.linux_process();
        let ppid = proc.parent().map(|p| p.id()).unwrap_or(0);
        Ok(ppid as usize)
    }

    /// `sys_exit` system call terminates only the calling thread
    /// (see [linux man _exit(2)](https://www.man7.org/linux/man-pages/man2/exit.2.html),
    /// this syscall is same as a raw `_exit` in glibc),
    /// and actions such as reparenting child processes or sending
    /// SIGCHLD to the parent process are performed only if this is the
    /// last thread in the thread group.
    pub fn sys_exit(&mut self, exit_code: i32) -> SysResult {
        info!("exit: code={}", exit_code);
        self.thread.exit_linux(exit_code);
        Err(LxError::ENOSYS)
    }

    /// `sys_exit_group` is equivalent to [`Self::sys_exit`]
    /// except that it terminates not only the calling thread
    /// (see [linux man exit_group(2)](https://www.man7.org/linux/man-pages/man2/exit_group.2.html),
    /// but all threads in the calling process's thread group.
    /// As a result, the entire calling process will exit.
    pub fn sys_exit_group(&mut self, exit_code: i32) -> SysResult {
        info!("exit_group: code={}", exit_code);
        let proc = self.zircon_process();
        proc.exit(exit_code as i64);
        Err(LxError::ENOSYS)
    }

    /// Allows the calling thread to sleep for
    /// an interval specified with nanosecond precision
    /// (see [linux man nanosleep(2)](https://www.man7.org/linux/man-pages/man2/nanosleep.2.html).
    ///
    /// `nanosleep` suspends the execution of the calling thread
    /// until either at least the time specified in `req` has elapsed,
    /// or the delivery of a signal that triggers the invocation of a handler
    /// in the calling thread or that terminates the process.
    ///
    /// To represent a duration, see TimeSpec.
    pub async fn sys_nanosleep(&self, req: UserInPtr<TimeSpec>) -> SysResult {
        info!("nanosleep: deadline={:?}", req);
        let duration = req.read()?.into();
        use kernel_hal::{thread, timer};
        thread::sleep_until(timer::deadline_after(duration)).await;
        // Check for pending signals after wakeup.
        // Note: the Linux nanosleep rem pointer (a1) is not yet
        // plumbed through; on EINTR the remaining time is not written.
        if self.thread.lock_linux().has_pending_signal() {
            return Err(LxError::EINTR);
        }
        Ok(0)
    }

    /// `set_tid_address` sets the clear_child_tid value for the calling thread to `tidptr`,
    /// and return the caller's thread ID
    /// (see [linux man set_tid_address(2)](https://www.man7.org/linux/man-pages/man2/set_tid_address.2.html).
    pub fn sys_set_tid_address(&self, tidptr: UserOutPtr<i32>) -> SysResult {
        info!("set_tid_address: {:?}", tidptr);
        self.thread.set_tid_address(tidptr);
        let tid = self.thread.id();
        Ok(tid as usize)
    }

    /// Get robust list.
    pub fn sys_get_robust_list(
        &self,
        pid: i32,
        head_ptr: UserOutPtr<UserOutPtr<RobustList>>,
        len_ptr: UserOutPtr<usize>,
    ) -> SysResult {
        if pid == 0 {
            return self.thread.get_robust_list(head_ptr, len_ptr);
        }
        Ok(0)
    }

    /// Set robust list.
    pub fn sys_set_robust_list(&self, head: UserInPtr<RobustList>, len: usize) -> SysResult {
        if len != size_of::<RobustList>() {
            return Err(LxError::EINVAL);
        }
        self.thread.set_robust_list(head, len);
        Ok(0)
    }
}

bitflags! {
    pub struct CloneFlags: usize {
        ///
        const CSIGNAL =         0xff;
        /// the calling process and the child process run in the same memory space
        const VM =              1 << 8;
        /// the caller and the child process share the same filesystem information
        const FS =              1 << 9;
        /// the calling process and the child process share the same file descriptor table
        const FILES =           1 << 10;
        /// the calling process and the child process share the same table of signal handlers.
        const SIGHAND =         1 << 11;
        /// the calling process is being traced
        const PTRACE =          1 << 13;
        /// the execution of the calling process is suspended until the child releases its virtual memory resources
        const VFORK =           1 << 14;
        /// the parent of the new child will be the same as that of the call‐ing process.
        const PARENT =          1 << 15;
        /// the child is placed in the same thread group as the calling process.
        const THREAD =          1 << 16;
        /// cloned child is started in a new mount namespace
        const NEWNS	=           1 << 17;
        /// the child and the calling process share a single list of System V semaphore adjustment values.
        const SYSVSEM =         1 << 18;
        /// architecture dependent, The TLS (Thread Local Storage) descriptor is set to tls.
        const SETTLS =          1 << 19;
        /// Store the child thread ID at the location in the parent's memory.
        const PARENT_SETTID =   1 << 20;
        /// Clear (zero) the child thread ID
        const CHILD_CLEARTID =  1 << 21;
        /// the parent not to receive a signal when the child terminated
        const DETACHED =        1 << 22;
        /// a tracing process cannot force CLONE_PTRACE on this child process.
        const UNTRACED =        1 << 23;
        /// Store the child thread ID
        const CHILD_SETTID =    1 << 24;
        /// Create the process in a new cgroup namespace.
        const NEWCGROUP =       1 << 25;
        /// create the process in a new UTS namespace
        const NEWUTS =          1 << 26;
        /// create the process in a new IPC namespace.
        const NEWIPC =          1 << 27;
        /// create the process in a new user namespace
        const NEWUSER =         1 << 28;
        /// create the process in a new PID namespace
        const NEWPID =          1 << 29;
        /// create the process in a new net‐work namespace.
        const NEWNET =          1 << 30;
        /// the new process shares an I/O context with the calling process.
        const IO =              1 << 31;
    }
}
