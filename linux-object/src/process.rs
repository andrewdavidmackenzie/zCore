//! Linux Process

use crate::{
    error::{LxError, LxResult},
    fs::{File, FileDesc, FileLike, OpenFlags, STDIN, STDOUT},
    ipc::*,
    net::SOCKET_FD,
    signal::{Signal as LinuxSignal, SignalAction},
    thread::ThreadExt,
    time::ITimerVal,
};
use alloc::{
    boxed::Box,
    string::String,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::sync::atomic::AtomicI32;
use core::time::Duration;
use hashbrown::HashMap;
use kernel_hal::VirtAddr;
use lock::{Mutex, MutexGuard};
use rcore_fs::vfs::{FileSystem, INode};

use zircon_object::{
    object::{KernelObject, KoID, Signal},
    signal::Futex,
    task::{Job, Process, Status},
    ZxResult,
};

pub use rcore_fs::vfs::FsInfo;

/// Process extension for linux
pub trait ProcessExt {
    /// create Linux process
    fn create_linux(job: &Arc<Job>, rootfs: Arc<dyn FileSystem>) -> ZxResult<Arc<Self>>;
    /// get linux process
    fn linux(&self) -> &LinuxProcess;
    /// fork from current linux process
    fn fork_from(parent: &Arc<Self>, vfork: bool) -> ZxResult<Arc<Self>>;
}

impl ProcessExt for Process {
    fn create_linux(job: &Arc<Job>, rootfs: Arc<dyn FileSystem>) -> ZxResult<Arc<Self>> {
        let linux_proc = LinuxProcess::new(rootfs);
        Process::create_with_ext(job, "root", linux_proc)
    }

    fn linux(&self) -> &LinuxProcess {
        self.ext().downcast_ref::<LinuxProcess>().unwrap()
    }

    /// [Fork] the process.
    ///
    /// [Fork]: http://man7.org/linux/man-pages/man2/fork.2.html
    fn fork_from(parent: &Arc<Self>, vfork: bool) -> ZxResult<Arc<Self>> {
        let linux_parent = parent.linux();
        let mut linux_parent_inner = linux_parent.inner.lock();
        let new_linux_proc = LinuxProcess {
            root_inode: linux_parent.root_inode.clone(),
            parent: Arc::downgrade(parent),
            inner: Mutex::new(LinuxProcessInner {
                execute_path: linux_parent_inner.execute_path.clone(),
                current_working_directory: linux_parent_inner.current_working_directory.clone(),
                files: linux_parent_inner.files.clone(),
                signal_actions: linux_parent_inner.signal_actions.clone(),
                brk_addr: linux_parent_inner.brk_addr,
                ..Default::default()
            }),
        };
        let new_proc = Process::create_with_ext(&parent.job(), "", new_linux_proc)?;
        linux_parent_inner
            .children
            .insert(new_proc.id(), new_proc.clone());
        if !vfork {
            new_proc.vmar().fork_from(&parent.vmar())?;
        }

        // notify parent on terminated
        let parent = parent.clone();
        new_proc.add_signal_callback(Box::new(move |signal| {
            if signal.contains(Signal::PROCESS_TERMINATED) {
                info!("Received signal: {:?}", signal);
                parent.signal_set(Signal::SIGCHLD);
            }
            false
        }));
        Ok(new_proc)
    }
}

/// Wait for state changes in a child of the calling process, and obtain information about
/// the child whose state has changed.
///
/// A state change is considered to be:
/// - the child terminated.
/// - the child was stopped by a signal. TODO
/// - the child was resumed by a signal. TODO
///
/// Returns `Err(EINTR)` if a signal is delivered to the calling thread
/// while waiting.
pub async fn wait_child(
    proc: &Arc<Process>,
    pid: KoID,
    nonblock: bool,
    thread: &zircon_object::task::Thread,
) -> LxResult<ExitCode> {
    loop {
        // Check for pending signals before blocking
        if thread.lock_linux().has_pending_signal() {
            return Err(LxError::EINTR);
        }
        let mut inner = proc.linux().inner.lock();
        let child = inner.children.get(&pid).ok_or(LxError::ECHILD)?;
        if let Status::Exited(code) = child.status() {
            inner.children.remove(&pid);
            return Ok(code as ExitCode);
        }
        if nonblock {
            return Err(LxError::EAGAIN);
        }
        drop(inner);
        // Wait for SIGCHLD on the parent process. This is woken by:
        // - child exit (Process::exit sets SIGCHLD on parent)
        // - SIGALRM timer callback (sets SIGCHLD to wake wait)
        // - any signal delivery via insert_signal()
        let proc_obj: Arc<dyn KernelObject> = proc.clone();
        proc_obj.signal_clear(Signal::SIGCHLD);
        proc_obj.wait_signal(Signal::SIGCHLD).await;
    }
}

/// Wait for state changes in a child of the calling process.
///
/// Returns `Err(EINTR)` if a signal is delivered to the calling thread
/// while waiting.
pub async fn wait_child_any(
    proc: &Arc<Process>,
    nonblock: bool,
    thread: &zircon_object::task::Thread,
) -> LxResult<(KoID, ExitCode)> {
    loop {
        // Check for pending signals before blocking
        if thread.lock_linux().has_pending_signal() {
            return Err(LxError::EINTR);
        }
        let mut inner = proc.linux().inner.lock();
        if inner.children.is_empty() {
            return Err(LxError::ECHILD);
        }
        for (&pid, child) in inner.children.iter() {
            if let Status::Exited(code) = child.status() {
                inner.children.remove(&pid);
                return Ok((pid, code as ExitCode));
            }
        }
        drop(inner);
        if nonblock {
            return Err(LxError::EAGAIN);
        }
        let proc_obj: Arc<dyn KernelObject> = proc.clone();
        proc_obj.signal_clear(Signal::SIGCHLD);
        proc_obj.wait_signal(Signal::SIGCHLD).await;
    }
}

/// Linux specific process information.
pub struct LinuxProcess {
    /// The root INode of file system
    root_inode: Arc<dyn INode>,
    /// Parent process
    parent: Weak<Process>,
    /// Inner
    inner: Mutex<LinuxProcessInner>,
}

/// Linux process mut inner data
#[derive(Default)]
struct LinuxProcessInner {
    /// Execute path
    execute_path: String,
    /// Current Working Directory
    ///
    /// Omit leading '/'.
    current_working_directory: String,
    /// file open number limit
    file_limit: RLimit,
    /// Opened files
    files: HashMap<FileDesc, Arc<dyn FileLike>>,
    /// Semaphore
    semaphores: SemProc,
    /// Share Memory
    shm_identifiers: ShmProc,
    /// Futexes
    futexes: HashMap<VirtAddr, Arc<Futex>>,
    /// Child processes
    children: HashMap<KoID, Arc<Process>>,
    /// Signal actions
    signal_actions: SignalActions,
    /// Program break (end of heap)
    brk_addr: VirtAddr,
    /// ITIMER_REAL: repeat interval (zero = one-shot)
    itimer_real_interval: Duration,
    /// ITIMER_REAL: absolute deadline (None = disarmed)
    itimer_real_deadline: Option<Duration>,
    /// ITIMER_REAL: generation counter for cancelling stale callbacks
    itimer_real_generation: u64,
    /// POSIX timers created by timer_create
    posix_timers: HashMap<usize, PosixTimer>,
    /// Next POSIX timer ID to allocate
    next_timer_id: usize,
}

/// Per-process POSIX timer state.
struct PosixTimer {
    /// Signal to deliver on expiry (e.g. SIGALRM)
    signal: LinuxSignal,
    /// Notification type (SIGEV_SIGNAL, SIGEV_NONE)
    notify: i32,
    /// Repeat interval (zero = one-shot)
    interval: Duration,
    /// Absolute deadline (None = disarmed)
    deadline: Option<Duration>,
    /// Generation counter for stale callback cancellation
    generation: u64,
}

impl Default for PosixTimer {
    fn default() -> Self {
        PosixTimer {
            signal: LinuxSignal::SIGALRM,
            notify: 0, // SIGEV_SIGNAL
            interval: Duration::default(),
            deadline: None,
            generation: 0,
        }
    }
}

#[derive(Clone)]
struct SignalActions {
    table: [SignalAction; LinuxSignal::RTMAX + 1],
}

impl Default for SignalActions {
    fn default() -> Self {
        Self {
            table: [SignalAction::default(); LinuxSignal::RTMAX + 1],
        }
    }
}

/// resource limit
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct RLimit {
    /// soft limit
    pub cur: u64,
    /// hard limit
    pub max: u64,
}

impl Default for RLimit {
    fn default() -> Self {
        RLimit {
            cur: 1024,
            max: 1024,
        }
    }
}

/// The type of process exit code.
pub type ExitCode = i32;

impl LinuxProcess {
    /// Create a new process.
    pub fn new(rootfs: Arc<dyn FileSystem>) -> Self {
        let stdin = File::new(
            STDIN.clone(), // FIXME: stdin
            OpenFlags::RDONLY,
            String::from("/dev/stdin"),
        ) as Arc<dyn FileLike>;
        let stdout = File::new(
            STDOUT.clone(), // TODO: open from '/dev/stdout'
            OpenFlags::WRONLY,
            String::from("/dev/stdout"),
        ) as Arc<dyn FileLike>;
        let stderr = File::new(
            STDOUT.clone(), // TODO: open from '/dev/stderr'
            OpenFlags::WRONLY,
            String::from("/dev/stderr"),
        ) as Arc<dyn FileLike>;
        let mut files = HashMap::new();
        files.insert(0.into(), stdin);
        files.insert(1.into(), stdout);
        files.insert(2.into(), stderr);

        LinuxProcess {
            root_inode: crate::fs::create_root_fs(rootfs), //Arc::clone(&ROOT_INODE),访问磁盘可能更快？
            parent: Weak::default(),
            inner: Mutex::new(LinuxProcessInner {
                files,
                ..Default::default()
            }),
        }
    }

    /// Get futex object.
    #[allow(unsafe_code)]
    pub fn get_futex(&self, uaddr: VirtAddr) -> Arc<Futex> {
        let mut inner = self.inner.lock();
        inner
            .futexes
            .entry(uaddr)
            .or_insert_with(|| {
                let value = unsafe { &*(uaddr as *const AtomicI32) };
                Futex::new(value)
            })
            .clone()
    }

    /// Get lowest free fd
    pub fn get_free_fd(&self) -> FileDesc {
        self.inner.lock().get_free_fd()
    }

    /// get the lowest available fd great than or equal to `start`.
    pub fn get_free_fd_from(&self, start: usize) -> FileDesc {
        self.inner.lock().get_free_fd_from(start)
    }

    /// Add a file to the file descriptor table.
    pub fn add_file(&self, file: Arc<dyn FileLike>) -> LxResult<FileDesc> {
        let inner = self.inner.lock();
        let fd = inner.get_free_fd();
        self.insert_file(inner, fd, file)
    }

    /// Add a socket to the fd table.
    pub fn add_socket(&self, file: Arc<dyn FileLike>) -> LxResult<FileDesc> {
        let inner = self.inner.lock();
        let fd = inner.get_free_fd_from(SOCKET_FD);
        self.insert_file(inner, fd, file)
    }

    /// Add a file to the file descriptor table at given `fd`.
    pub fn add_file_at(&self, fd: FileDesc, file: Arc<dyn FileLike>) -> LxResult<FileDesc> {
        let inner = self.inner.lock();
        self.insert_file(inner, fd, file)
    }

    /// insert a file and fd into the file descriptor table
    fn insert_file(
        &self,
        mut inner: MutexGuard<LinuxProcessInner>,
        fd: FileDesc,
        file: Arc<dyn FileLike>,
    ) -> LxResult<FileDesc> {
        if inner.files.len() < inner.file_limit.cur as usize {
            inner.files.insert(fd, file);
            Ok(fd)
        } else {
            Err(LxError::EMFILE)
        }
    }

    /// get and set file limit number
    pub fn file_limit(&self, new_limit: Option<RLimit>) -> RLimit {
        let mut inner = self.inner.lock();
        let old = inner.file_limit;
        if let Some(limit) = new_limit {
            inner.file_limit = limit;
        }
        old
    }

    /// Get the `File` with given `fd`.
    pub fn get_file(&self, fd: FileDesc) -> LxResult<Arc<File>> {
        let file = self
            .get_file_like(fd)?
            .downcast_arc::<File>()
            .map_err(|_| LxError::EBADF)?;
        Ok(file)
    }

    /*
        /// Get the `Socket` with given `fd`.
        pub fn get_socket(&self, fd: FileDesc) -> LxResult<Arc<dyn Socket>> {
            let socket = self
                .get_file_like(fd)?
                .as_socket()
            .map_err(|_| LxError::EBADF)?;
            Ok(Arc::new(socket))
        }
    */

    /// Get the `FileLike` with given `fd`.
    pub fn get_file_like(&self, fd: FileDesc) -> LxResult<Arc<dyn FileLike>> {
        let inner = self.inner.lock();
        trace!("get_file_like: {:x?}", inner.files);
        inner.files.get(&fd).cloned().ok_or(LxError::EBADF)
    }

    /// get all files
    pub fn get_files(&self) -> LxResult<HashMap<FileDesc, Arc<dyn FileLike>>> {
        let inner = self.inner.lock();
        Ok(inner.files.clone())
    }

    /// Close file descriptor `fd`.
    pub fn close_file(&self, fd: FileDesc) -> LxResult {
        let mut inner = self.inner.lock();
        inner.files.remove(&fd).map(|_| ()).ok_or(LxError::EBADF)
    }

    /// Get root INode of the process.
    pub fn root_inode(&self) -> &Arc<dyn INode> {
        &self.root_inode
    }

    /// Get the current program break address.
    pub fn get_brk(&self) -> VirtAddr {
        self.inner.lock().brk_addr
    }

    /// Set the program break address.
    pub fn set_brk(&self, addr: VirtAddr) {
        self.inner.lock().brk_addr = addr;
    }

    /// Get parent process.
    pub fn parent(&self) -> Option<Arc<Process>> {
        self.parent.upgrade()
    }

    /// Get current working directory.
    pub fn current_working_directory(&self) -> String {
        String::from("/") + &self.inner.lock().current_working_directory
    }

    /// Change working directory.
    pub fn change_directory(&self, path: &str) {
        if path.is_empty() {
            return;
        }
        let mut inner = self.inner.lock();
        let cwd = match path.as_bytes()[0] {
            b'/' => String::new(),
            _ => inner.current_working_directory.clone(),
        };
        let mut cwd_vec: Vec<_> = cwd.split('/').filter(|x| !x.is_empty()).collect();
        for seg in path.split('/') {
            match seg {
                ".." => {
                    cwd_vec.pop();
                }
                "." | "" => {} // nothing to do here.
                _ => cwd_vec.push(seg),
            }
        }
        inner.current_working_directory = cwd_vec.join("/");
    }

    /// Get execute path.
    pub fn execute_path(&self) -> String {
        self.inner.lock().execute_path.clone()
    }

    /// Set execute path.
    pub fn set_execute_path(&self, path: &str) {
        self.inner.lock().execute_path = String::from(path);
    }

    /// Get signal action.
    pub fn signal_action(&self, signal: LinuxSignal) -> SignalAction {
        self.inner.lock().signal_actions.table[signal as u8 as usize]
    }

    /// Set signal action.
    pub fn set_signal_action(&self, signal: LinuxSignal, action: SignalAction) {
        self.inner.lock().signal_actions.table[signal as u8 as usize] = action;
    }

    /// Close file that FD_CLOEXEC is set
    pub fn remove_cloexec_files(&self) {
        let mut inner = self.inner.lock();
        let close_fds = inner
            .files
            .iter()
            .filter_map(|(fd, file_like)| {
                if let Ok(file) = file_like.clone().downcast_arc::<File>() {
                    if file.flags().close_on_exec() {
                        Some(*fd)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for fd in close_fds {
            inner.files.remove(&fd).map(|_| ()).unwrap();
        }
    }

    /// Insert a `SemArray` and return its ID
    pub fn semaphores_add(&self, array: Arc<SemArray>) -> usize {
        self.inner.lock().semaphores.add(array)
    }

    /// Get an semaphore set by `id`
    pub fn semaphores_get(&self, id: usize) -> Option<Arc<SemArray>> {
        self.inner.lock().semaphores.get(id)
    }

    /// Add an undo operation
    pub fn semaphores_add_undo(&self, id: usize, num: u16, op: i16) {
        self.inner.lock().semaphores.add_undo(id, num, op)
    }

    /// Remove an `SemArray` by ID
    pub fn semaphores_remove(&self, id: usize) {
        self.inner.lock().semaphores.remove(id)
    }

    /// get ShmId from Virtual Addr
    pub fn shm_get_id(&self, id: usize) -> Option<usize> {
        self.inner.lock().shm_identifiers.get_id(id)
    }

    /// get the ShmIdentifier from shm_identifiers
    pub fn shm_get(&self, id: usize) -> Option<ShmIdentifier> {
        self.inner.lock().shm_identifiers.get(id)
    }

    /// Delete the ShmIdentifier from shm_identifiers
    pub fn shm_pop(&self, id: usize) {
        self.inner.lock().shm_identifiers.pop(id)
    }

    /// Insert the `SharedGuard` and return its ID
    pub fn shm_add(&self, shared_guard: Arc<Mutex<ShmGuard>>) -> usize {
        self.inner.lock().shm_identifiers.add(shared_guard)
    }

    /// Set Virtual Addr for shared memory
    pub fn shm_set(&self, id: usize, shm_id: ShmIdentifier) {
        self.inner.lock().shm_identifiers.set(id, shm_id)
    }

    /// Get the current ITIMER_REAL value (remaining time + interval).
    pub fn get_itimer_real(&self) -> ITimerVal {
        let inner = self.inner.lock();
        let it_value = match inner.itimer_real_deadline {
            Some(deadline) => {
                let now = kernel_hal::timer::timer_now();
                let remaining = deadline.saturating_sub(now);
                crate::time::TimeVal::from_duration(remaining)
            }
            None => crate::time::TimeVal::default(),
        };
        ITimerVal {
            it_interval: crate::time::TimeVal::from_duration(inner.itimer_real_interval),
            it_value,
        }
    }

    /// Set the ITIMER_REAL timer. Returns the previous value.
    /// If `it_value` is non-zero, arms the timer to deliver SIGALRM.
    /// If `it_value` is zero, disarms the timer.
    pub fn set_itimer_real(&self, new: ITimerVal, proc: &Arc<Process>) -> ITimerVal {
        let old = self.get_itimer_real();
        let mut inner = self.inner.lock();

        // Increment generation to invalidate any pending callback
        inner.itimer_real_generation += 1;
        let generation = inner.itimer_real_generation;

        let value_dur = new.it_value.to_duration();
        let interval_dur = new.it_interval.to_duration();
        inner.itimer_real_interval = interval_dur;

        if value_dur.is_zero() {
            // Disarm
            inner.itimer_real_deadline = None;
        } else {
            // Arm
            let deadline = kernel_hal::timer::deadline_after(value_dur);
            inner.itimer_real_deadline = Some(deadline);
            let weak_proc = Arc::downgrade(proc);
            drop(inner); // release lock before scheduling
            Self::schedule_itimer_real(weak_proc, deadline, interval_dur, generation);
        }

        old
    }

    /// Schedule the ITIMER_REAL callback via the kernel timer.
    fn schedule_itimer_real(
        proc: Weak<Process>,
        deadline: Duration,
        interval: Duration,
        generation: u64,
    ) {
        use kernel_hal::timer;

        let callback: Box<dyn FnOnce(Duration) + Send + Sync> = Box::new(move |_now| {
            let proc = match proc.upgrade() {
                Some(p) => p,
                None => return, // process already exited
            };
            let linux = proc.linux();

            // Check generation — if mismatched, this callback is stale
            {
                let inner = linux.inner.lock();
                if inner.itimer_real_generation != generation {
                    return;
                }
            }

            // Deliver SIGALRM to first eligible thread
            info!(
                "ITIMER_REAL fired: delivering SIGALRM to process {}",
                proc.id()
            );
            let tids = proc.thread_ids();
            for tid in tids {
                if let Ok(thread_obj) = proc.get_child(tid) {
                    if let Ok(thread) = thread_obj.downcast_arc::<zircon_object::task::Thread>() {
                        let mut thread_linux = thread.lock_linux();
                        if !thread_linux.signal_mask.contains(LinuxSignal::SIGALRM) {
                            thread_linux.insert_signal(LinuxSignal::SIGALRM);
                            break;
                        }
                    }
                }
            }

            // Wake any wait_signal futures on this process (e.g. wait4
            // blocked on SIGCHLD). Setting SIGCHLD will cause the
            // wait_signal future to wake up and re-check, enabling
            // EINTR return from blocking waits.
            proc.signal_set(Signal::SIGCHLD);

            // Re-arm if interval is non-zero
            if !interval.is_zero() {
                let mut inner = linux.inner.lock();
                if inner.itimer_real_generation == generation {
                    let new_deadline = timer::deadline_after(interval);
                    inner.itimer_real_deadline = Some(new_deadline);
                    let weak = Arc::downgrade(&proc);
                    drop(inner);
                    Self::schedule_itimer_real(weak, new_deadline, interval, generation);
                }
            } else {
                let mut inner = linux.inner.lock();
                if inner.itimer_real_generation == generation {
                    inner.itimer_real_deadline = None;
                }
            }
        });
        timer::timer_set(deadline, callback);
    }

    /// Create a POSIX timer. Returns the timer ID.
    pub fn create_posix_timer(&self, signal: LinuxSignal, notify: i32) -> usize {
        let mut inner = self.inner.lock();
        let id = inner.next_timer_id;
        inner.next_timer_id += 1;
        inner.posix_timers.insert(
            id,
            PosixTimer {
                signal,
                notify,
                ..PosixTimer::default()
            },
        );
        id
    }

    /// Set a POSIX timer. Returns the old timer spec.
    pub fn set_posix_timer(
        &self,
        id: usize,
        flags: usize,
        new_value: crate::time::ITimerSpec,
        proc: &Arc<Process>,
    ) -> LxResult<crate::time::ITimerSpec> {
        let old = self.get_posix_timer(id)?;
        let mut inner = self.inner.lock();
        let timer = inner.posix_timers.get_mut(&id).ok_or(LxError::EINVAL)?;

        // Bump generation to cancel any pending callback
        timer.generation += 1;
        let generation = timer.generation;

        let value_dur = new_value.it_value.to_duration();
        let interval_dur = new_value.it_interval.to_duration();
        timer.interval = interval_dur;

        if value_dur.is_zero() {
            timer.deadline = None;
        } else {
            let deadline = if flags & 1 != 0 {
                // TIMER_ABSTIME: convert to relative then to timer domain
                let now = kernel_hal::timer::timer_now();
                kernel_hal::timer::deadline_after(value_dur.saturating_sub(now))
            } else {
                kernel_hal::timer::deadline_after(value_dur)
            };
            timer.deadline = Some(deadline);
            let signal = timer.signal;
            let notify = timer.notify;
            let weak_proc = Arc::downgrade(proc);
            drop(inner);
            Self::schedule_posix_timer(
                weak_proc,
                id,
                deadline,
                interval_dur,
                signal,
                notify,
                generation,
            );
        }

        Ok(old)
    }

    /// Get the current POSIX timer spec.
    pub fn get_posix_timer(&self, id: usize) -> LxResult<crate::time::ITimerSpec> {
        let inner = self.inner.lock();
        let timer = inner.posix_timers.get(&id).ok_or(LxError::EINVAL)?;
        let it_value = match timer.deadline {
            Some(deadline) => {
                let now = kernel_hal::timer::timer_now();
                crate::time::TimeSpec::from_duration(deadline.saturating_sub(now))
            }
            None => crate::time::TimeSpec::default(),
        };
        Ok(crate::time::ITimerSpec {
            it_interval: crate::time::TimeSpec::from_duration(timer.interval),
            it_value,
        })
    }

    /// Delete a POSIX timer.
    pub fn delete_posix_timer(&self, id: usize) -> LxResult {
        let mut inner = self.inner.lock();
        let timer = inner.posix_timers.get_mut(&id).ok_or(LxError::EINVAL)?;
        // Bump generation to cancel any pending callback
        timer.generation += 1;
        timer.deadline = None;
        inner.posix_timers.remove(&id);
        Ok(())
    }

    /// Schedule a POSIX timer callback.
    fn schedule_posix_timer(
        proc: Weak<Process>,
        timer_id: usize,
        deadline: Duration,
        interval: Duration,
        signal: LinuxSignal,
        notify: i32,
        generation: u64,
    ) {
        use kernel_hal::timer;

        let callback: Box<dyn FnOnce(Duration) + Send + Sync> = Box::new(move |_now| {
            let proc = match proc.upgrade() {
                Some(p) => p,
                None => return,
            };
            let linux = proc.linux();

            // Check generation
            {
                let inner = linux.inner.lock();
                match inner.posix_timers.get(&timer_id) {
                    Some(t) if t.generation == generation => {}
                    _ => return, // stale or deleted
                }
            }

            // Deliver signal if SIGEV_SIGNAL
            if notify == crate::time::SIGEV_SIGNAL {
                let tids = proc.thread_ids();
                for tid in tids {
                    if let Ok(thread_obj) = proc.get_child(tid) {
                        if let Ok(thread) = thread_obj.downcast_arc::<zircon_object::task::Thread>()
                        {
                            let mut thread_linux = thread.lock_linux();
                            if !thread_linux.signal_mask.contains(signal) {
                                thread_linux.signals.insert(signal);
                                break;
                            }
                        }
                    }
                }
                // Wake any wait_signal futures
                proc.signal_set(Signal::SIGCHLD);
            }

            // Re-arm if interval is non-zero
            if !interval.is_zero() {
                let mut inner = linux.inner.lock();
                if let Some(t) = inner.posix_timers.get_mut(&timer_id) {
                    if t.generation == generation {
                        let new_deadline = timer::deadline_after(interval);
                        t.deadline = Some(new_deadline);
                        let weak = Arc::downgrade(&proc);
                        drop(inner);
                        Self::schedule_posix_timer(
                            weak,
                            timer_id,
                            new_deadline,
                            interval,
                            signal,
                            notify,
                            generation,
                        );
                    }
                }
            } else {
                let mut inner = linux.inner.lock();
                if let Some(t) = inner.posix_timers.get_mut(&timer_id) {
                    if t.generation == generation {
                        t.deadline = None;
                    }
                }
            }
        });
        timer::timer_set(deadline, callback);
    }
}

impl LinuxProcessInner {
    fn get_free_fd(&self) -> FileDesc {
        self.get_free_fd_from(0)
    }

    fn get_free_fd_from(&self, start: usize) -> FileDesc {
        (start..)
            .map(|i| i.into())
            .find(|fd| !self.files.contains_key(fd))
            .unwrap()
    }
}
