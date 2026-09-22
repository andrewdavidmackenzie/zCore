use super::*;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::time::Duration;
use hal_impl::timer::timer_now;
use linux_object::process::LinuxProcess;
use linux_object::thread::ThreadExt;
use linux_object::time::*;

/// PR_SET_NAME: set the name of the calling thread.
const PR_SET_NAME: usize = 15;
/// PR_GET_NAME: get the name of the calling thread.
const PR_GET_NAME: usize = 16;

impl Syscall<'_> {
    /// Process control operations.
    pub fn sys_prctl(&self, option: usize, arg: usize) -> SysResult {
        info!("prctl: option={}, arg={:#x}", option, arg);
        match option {
            PR_SET_NAME => {
                let name_ptr: UserInPtr<u8> = arg.into();
                // PR_SET_NAME: read at most 16 bytes (including NUL)
                let buf = name_ptr.read_array(16)?;
                let len = buf.iter().position(|&b| b == 0).unwrap_or(15).min(15);
                let name = core::str::from_utf8(&buf[..len]).unwrap_or("?");
                self.thread.set_name(name);
                Ok(0)
            }
            PR_GET_NAME => {
                let mut name_ptr: UserOutPtr<u8> = arg.into();
                let name = self.thread.name();
                let bytes = name.as_bytes();
                let len = bytes.len().min(15);
                name_ptr.write_array(&bytes[..len])?;
                // Write NUL terminator
                name_ptr.add(len).write(0u8)?;
                Ok(0)
            }
            _ => {
                warn!("prctl: unsupported option {}", option);
                Err(LxError::EINVAL)
            }
        }
    }

    /// Get the CPU affinity mask of a process.
    /// For single-CPU zCore, returns a mask with only CPU 0 set.
    pub fn sys_sched_getaffinity(
        &self,
        _pid: usize,
        cpusetsize: usize,
        mut mask: UserOutPtr<u8>,
    ) -> SysResult {
        info!("sched_getaffinity: pid={}, cpusetsize={}", _pid, cpusetsize);
        if cpusetsize == 0 {
            return Err(LxError::EINVAL);
        }
        // Write a bitmask with only CPU 0 set
        let mut buf = alloc::vec![0u8; cpusetsize];
        buf[0] = 1; // CPU 0
        mask.write_array(&buf)?;
        // Return the number of bytes written (Linux returns the size of cpumask_t)
        Ok(cpusetsize.min(core::mem::size_of::<usize>()))
    }

    #[cfg(target_arch = "x86_64")]
    /// set architecture-specific thread state
    /// for x86_64 currently
    pub fn sys_arch_prctl(&mut self, code: i32, addr: usize) -> SysResult {
        const ARCH_SET_FS: i32 = 0x1002;
        match code {
            ARCH_SET_FS => {
                info!("sys_arch_prctl: set FSBASE to {:#x}", addr);
                self.thread.with_context(|ctx| {
                    ctx.set_field(hal_impl::context::UserContextField::ThreadPointer, addr)
                })?;
                Ok(0)
            }
            _ => Err(LxError::EINVAL),
        }
    }

    /// get name and information about current kernel
    pub fn sys_uname(&self, buf: UserOutPtr<u8>) -> SysResult {
        info!("uname: buf={:?}", buf);

        let release = alloc::string::String::from(concat!(env!("CARGO_PKG_VERSION"), "-zcore"));
        #[cfg(not(target_os = "none"))]
        let release = release + "-libos";

        let vdso_const = hal_impl::vdso::vdso_constants();

        let arch = if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else if cfg!(target_arch = "riscv64") {
            "riscv64"
        } else {
            "unknown"
        };

        let strings = [
            "Linux",                            // sysname
            "zcore",                            // nodename
            release.as_str(),                   // release
            vdso_const.version_string.as_str(), // version
            arch,                               // machine
            "rcore-os",                         // domainname
        ];

        for (i, &s) in strings.iter().enumerate() {
            const OFFSET: usize = 65;
            buf.add(i * OFFSET).write_cstring(s)?;
        }
        Ok(0)
    }

    /// provides a simple way of getting overall system statistics
    pub fn sys_sysinfo(&mut self, mut sys_info: UserOutPtr<SysInfo>) -> SysResult {
        use hal_impl::timer;
        // timer_now() returns monotonic (boot-relative) time in both
        // bare-metal and libos modes, which is correct for uptime.
        let uptime = timer::timer_now().as_secs();

        // Compute total RAM from boot-time free physical memory regions
        let totalram: u64 = hal_impl::mem::free_pmem_regions()
            .iter()
            .map(|r| (r.end - r.start) as u64)
            .sum();
        // freeram: live free-frame tracking requires per-platform allocator
        // changes (see issue #46). Use totalram * 3/4 as a rough estimate
        // that is better than 0 (which programs interpret as "no memory").
        let freeram = totalram * 3 / 4;

        // Count live processes by walking the job tree (usize to avoid u16 overflow)
        let procs = self.count_processes().min(u16::MAX as usize) as u16;

        let sysinfo = SysInfo {
            uptime,
            totalram,
            freeram,
            mem_unit: 1,
            procs,
            ..SysInfo::default()
        };
        sys_info.write(sysinfo)?;
        Ok(0)
    }

    /// Count all live processes by walking the job tree from the root.
    fn count_processes(&self) -> usize {
        let mut job = self.zircon_process().job();
        // Walk up to the root job
        while let Some(parent) = job.parent() {
            job = parent;
        }
        Self::count_processes_in_job(&job)
    }

    /// Recursively count processes in a job and its child jobs.
    fn count_processes_in_job(job: &zircon_object::task::Job) -> usize {
        let direct = job.process_ids().len();
        let from_children: usize = job
            .children_ids()
            .iter()
            .filter_map(|&id| job.get_child(id).ok())
            .filter_map(|obj| obj.downcast_arc::<zircon_object::task::Job>().ok())
            .map(|child_job| Self::count_processes_in_job(&child_job))
            .sum();
        direct + from_children
    }

    /// provides a method for waiting until a certain condition becomes true.
    /// - `uaddr` - points to the futex word.
    /// - `op` -  the operation to perform on the futex
    /// - `val` -  a value whose meaning and purpose depends on op
    /// - `val2` - provides a timeout for WAIT, or requeue count for REQUEUE/CMP_REQUEUE
    /// - `uaddr2` - when op is REQUEUE/CMP_REQUEUE, points to the target futex
    /// - `val3` - for CMP_REQUEUE, the expected value at `*uaddr`
    pub async fn sys_futex(
        &self,
        uaddr: usize,
        op: u32,
        val: u32,
        val2: usize,
        uaddr2: usize,
        val3: u32,
    ) -> SysResult {
        debug!(
            "Futex uaddr: {:#x}, op: {:x}, val: {}, val2(timeout_addr): {:x}",
            uaddr, op, val, val2,
        );
        let op = FutexFlags::from_bits_truncate(op);
        let is_private = op.contains(FutexFlags::PRIVATE);
        let op = op - FutexFlags::PRIVATE;
        let futex = if is_private {
            self.linux_process().get_futex(uaddr)
        } else {
            LinuxProcess::get_shared_futex(uaddr)
        };
        match op {
            FutexFlags::WAIT => {
                use linux_object::thread::Interruptible;
                let future = futex.wait(val as _);
                let timeout_addr: UserInPtr<TimeSpec> = val2.into();
                let res = if let Some(timeout) = timeout_addr.read_if_not_null().unwrap() {
                    // Timeout path: race the futex wait against a sleep timer,
                    // both interruptible by signals.
                    let deadline = timer_now() + Duration::from(timeout);
                    let timeout_future = async {
                        hal_impl::thread::SleepFuture::new(deadline).await;
                        Err::<(), _>(zircon_object::ZxError::TIMED_OUT)
                    };
                    // Pin both futures since select! needs them.
                    let mut wait_fut = core::pin::pin!(future);
                    let mut timeout_fut = core::pin::pin!(timeout_future);

                    // Simple select: poll both, return whichever completes first.
                    use core::future::Future;
                    use core::task::Poll;
                    let combined = core::future::poll_fn(|cx| {
                        // Check signals first
                        {
                            let mut linux = self.thread.lock_linux();
                            if linux.has_pending_signal() {
                                linux.clear_signal_waker();
                                return Poll::Ready(Err(zircon_object::ZxError::CANCELED));
                            }
                            linux.set_signal_waker(cx.waker().clone());
                        }
                        if let Poll::Ready(r) = wait_fut.as_mut().poll(cx) {
                            return Poll::Ready(r);
                        }
                        if let Poll::Ready(r) = timeout_fut.as_mut().poll(cx) {
                            return Poll::Ready(r);
                        }
                        Poll::Pending
                    });
                    combined.await
                } else {
                    match future.interruptible(self.thread).await {
                        Ok(zx_result) => zx_result,
                        Err(_) => return Err(LxError::EINTR),
                    }
                };
                match res {
                    Ok(_) => {
                        // Check for pending signals after a successful wait.
                        if self.thread.lock_linux().has_pending_signal() {
                            return Err(LxError::EINTR);
                        }
                        Ok(0)
                    }
                    Err(e) => Err(e.into()),
                }
            }
            FutexFlags::WAKE => Ok(futex.wake(val as _)),
            FutexFlags::REQUEUE => {
                if uaddr == uaddr2 {
                    return Err(LxError::EINVAL);
                }
                let requeue_futex = if is_private {
                    self.linux_process().get_futex(uaddr2)
                } else {
                    LinuxProcess::get_shared_futex(uaddr2)
                };
                futex
                    .requeue(0, val as _, val2, &requeue_futex, None, false)
                    .map_err(|e| e.into())
            }
            FutexFlags::CMP_REQUEUE => {
                if uaddr == uaddr2 {
                    return Err(LxError::EINVAL);
                }
                let requeue_futex = if is_private {
                    self.linux_process().get_futex(uaddr2)
                } else {
                    LinuxProcess::get_shared_futex(uaddr2)
                };
                futex
                    .requeue(val3 as _, val as _, val2, &requeue_futex, None, true)
                    .map_err(|e| e.into())
            }
            _ => {
                warn!("unsupported futex operation: {:?}", op);
                Err(LxError::ENOSYS)
            }
        }
    }

    /// Combines and extends the functionality of setrlimit() and getrlimit()
    pub fn sys_prlimit64(
        &mut self,
        pid: usize,
        resource: usize,
        new_limit: UserInPtr<RLimit>,
        mut old_limit: UserOutPtr<RLimit>,
    ) -> SysResult {
        info!(
            "prlimit64: pid: {}, resource: {}, new_limit: {:x?}, old_limit: {:x?}",
            pid, resource, new_limit, old_limit
        );
        let proc = self.linux_process();
        match resource {
            RLIMIT_STACK => {
                old_limit.write_if_not_null(RLimit {
                    cur: USER_STACK_SIZE as u64,
                    max: USER_STACK_SIZE as u64,
                })?;
                Ok(0)
            }
            RLIMIT_NOFILE => {
                let new_limit = new_limit.read_if_not_null()?;
                old_limit.write_if_not_null(proc.file_limit(new_limit))?;
                Ok(0)
            }
            RLIMIT_RSS | RLIMIT_AS => {
                old_limit.write_if_not_null(RLimit {
                    cur: 1024 * 1024 * 1024,
                    max: 1024 * 1024 * 1024,
                })?;
                Ok(0)
            }
            _ => Err(LxError::ENOSYS),
        }
    }

    /// Reboot the system.
    ///
    /// The `magic1` and `magic2` arguments must match the Linux-defined values,
    /// otherwise `EINVAL` is returned. The `cmd` argument selects the action:
    /// power off, restart, or halt.
    pub fn sys_reboot(&self, magic1: u32, magic2: u32, cmd: u32) -> SysResult {
        info!(
            "reboot: magic1={:#x}, magic2={:#x}, cmd={:#x}",
            magic1, magic2, cmd
        );

        // Linux requires these magic values to prevent accidental reboots
        const LINUX_REBOOT_MAGIC1: u32 = 0xfee1dead;
        const LINUX_REBOOT_MAGIC2: u32 = 672274793; // 0x28121969
        const LINUX_REBOOT_MAGIC2A: u32 = 85072278; // 0x05121996
        const LINUX_REBOOT_MAGIC2B: u32 = 369367448; // 0x16041998
        const LINUX_REBOOT_MAGIC2C: u32 = 537993216; // 0x20112000

        if magic1 != LINUX_REBOOT_MAGIC1 {
            return Err(LxError::EINVAL);
        }
        match magic2 {
            LINUX_REBOOT_MAGIC2 | LINUX_REBOOT_MAGIC2A | LINUX_REBOOT_MAGIC2B
            | LINUX_REBOOT_MAGIC2C => {}
            _ => return Err(LxError::EINVAL),
        }

        const LINUX_REBOOT_CMD_POWER_OFF: u32 = 0x4321FEDC;
        const LINUX_REBOOT_CMD_RESTART: u32 = 0x01234567;
        const LINUX_REBOOT_CMD_HALT: u32 = 0xCDEF0123;

        match cmd {
            LINUX_REBOOT_CMD_POWER_OFF | LINUX_REBOOT_CMD_HALT => {
                warn!("system power off");
                hal_impl::cpu::reset(); // PSCI SYSTEM_OFF
            }
            LINUX_REBOOT_CMD_RESTART => {
                warn!("system restart");
                hal_impl::cpu::reset(); // TODO: use PSCI SYSTEM_RESET
            }
            _ => {
                warn!("reboot: unsupported cmd {:#x}", cmd);
                Err(LxError::EINVAL)
            }
        }
    }

    #[allow(unsafe_code)]
    /// fills the buffer pointed to by `buf` with up to `buflen` random bytes.
    /// - `buf` - buffer that needed to fill
    /// - `buflen` - length of buffer
    /// - `flag` - a bit mask that can contain zero or more of the following values ORed together:
    ///   - GRND_RANDOM
    ///   - GRND_NONBLOCK
    /// - returns the number of bytes that were copied to the buffer buf.
    pub fn sys_getrandom(&mut self, mut buf: UserOutPtr<u8>, len: usize, flag: u32) -> SysResult {
        info!("getrandom: buf: {:?}, len: {:?}, flag {:?}", buf, len, flag);
        let mut buffer = vec![0u8; len];
        hal_impl::rand::fill_random(&mut buffer);
        buf.write_array(&buffer[..len])?;
        Ok(len)
    }

    // --- User/group identity syscalls ---

    /// Get the real user ID of the calling process.
    pub fn sys_getuid(&self) -> SysResult {
        let uid = self.linux_process().uid();
        info!("getuid => {}", uid);
        Ok(uid as usize)
    }

    /// Get the real group ID of the calling process.
    pub fn sys_getgid(&self) -> SysResult {
        let gid = self.linux_process().gid();
        info!("getgid => {}", gid);
        Ok(gid as usize)
    }

    /// Get the effective user ID of the calling process.
    pub fn sys_geteuid(&self) -> SysResult {
        let euid = self.linux_process().euid();
        info!("geteuid => {}", euid);
        Ok(euid as usize)
    }

    /// Get the effective group ID of the calling process.
    pub fn sys_getegid(&self) -> SysResult {
        let egid = self.linux_process().egid();
        info!("getegid => {}", egid);
        Ok(egid as usize)
    }

    /// Set the user ID of the calling process.
    /// If privileged (euid == 0), sets real and effective UID.
    /// Otherwise, only sets effective UID if it matches the
    /// real UID.
    pub fn sys_setuid(&self, uid: u32) -> SysResult {
        info!("setuid: uid={}", uid);
        self.linux_process()
            .set_uid(uid)
            .map(|_| 0)
            .map_err(|_| LxError::EPERM)
    }

    /// Set the real group ID.
    pub fn sys_setgid(&self, gid: u32) -> SysResult {
        info!("setgid: gid={}", gid);
        self.linux_process()
            .set_gid(gid)
            .map(|_| 0)
            .map_err(|_| LxError::EPERM)
    }

    /// Set real and effective user IDs.
    pub fn sys_setreuid(&self, ruid: i32, euid: i32) -> SysResult {
        info!("setreuid: ruid={}, euid={}", ruid, euid);
        self.linux_process()
            .set_reuid(ruid, euid)
            .map(|_| 0)
            .map_err(|_| LxError::EPERM)
    }

    /// Set real and effective group IDs.
    pub fn sys_setregid(&self, rgid: i32, egid: i32) -> SysResult {
        info!("setregid: rgid={}, egid={}", rgid, egid);
        self.linux_process()
            .set_regid(rgid, egid)
            .map(|_| 0)
            .map_err(|_| LxError::EPERM)
    }

    /// Set real, effective, and saved user IDs.
    pub fn sys_setresuid(&self, ruid: i32, euid: i32, suid: i32) -> SysResult {
        info!("setresuid: ruid={}, euid={}, suid={}", ruid, euid, suid);
        self.linux_process()
            .set_resuid(ruid, euid, suid)
            .map(|_| 0)
            .map_err(|_| LxError::EPERM)
    }

    /// Get real, effective, and saved user IDs.
    pub fn sys_getresuid(
        &self,
        mut ruid: UserOutPtr<u32>,
        mut euid: UserOutPtr<u32>,
        mut suid: UserOutPtr<u32>,
    ) -> SysResult {
        let (r, e, s) = self.linux_process().get_resuid();
        info!("getresuid: ruid={}, euid={}, suid={}", r, e, s);
        ruid.write(r)?;
        euid.write(e)?;
        suid.write(s)?;
        Ok(0)
    }

    /// Set real, effective, and saved group IDs.
    pub fn sys_setresgid(&self, rgid: i32, egid: i32, sgid: i32) -> SysResult {
        info!("setresgid: rgid={}, egid={}, sgid={}", rgid, egid, sgid);
        self.linux_process()
            .set_resgid(rgid, egid, sgid)
            .map(|_| 0)
            .map_err(|_| LxError::EPERM)
    }

    /// Get real, effective, and saved group IDs.
    pub fn sys_getresgid(
        &self,
        mut rgid: UserOutPtr<u32>,
        mut egid: UserOutPtr<u32>,
        mut sgid: UserOutPtr<u32>,
    ) -> SysResult {
        let (r, e, s) = self.linux_process().get_resgid();
        info!("getresgid: rgid={}, egid={}, sgid={}", r, e, s);
        rgid.write(r)?;
        egid.write(e)?;
        sgid.write(s)?;
        Ok(0)
    }

    /// Set filesystem UID (for access checks). Returns previous fsuid.
    pub fn sys_setfsuid(&self, fsuid: u32) -> SysResult {
        let old = self.linux_process().set_fsuid(fsuid);
        info!("setfsuid: fsuid={} => old={}", fsuid, old);
        Ok(old as usize)
    }

    /// Set filesystem GID (for access checks). Returns previous fsgid.
    pub fn sys_setfsgid(&self, fsgid: u32) -> SysResult {
        let old = self.linux_process().set_fsgid(fsgid);
        info!("setfsgid: fsgid={} => old={}", fsgid, old);
        Ok(old as usize)
    }

    // --- Linux capability syscalls ---

    /// Get process capabilities.
    ///
    /// The `hdrp` points to a `cap_user_header` (version + pid),
    /// and `datap` points to a `cap_user_data` (effective, permitted,
    /// inheritable bitmasks). Since all processes run as root, we
    /// report full capabilities.
    pub fn sys_capget(&self, hdrp: UserInPtr<u8>, mut datap: UserOutPtr<u8>) -> SysResult {
        // Read header to get version
        let hdr = hdrp.read_array(8)?;
        let version = u32::from_ne_bytes(hdr[0..4].try_into().unwrap());
        info!("capget: version={:#x}", version);

        if datap.is_null() {
            return Ok(0); // just checking version support
        }

        // All capabilities granted (root).
        // Version 3 uses two 32-bit halves per set (3 sets * 2 = 6 u32s = 24 bytes)
        // Version 1 uses one 32-bit word per set (3 sets = 3 u32s = 12 bytes)
        const V3: u32 = 0x2008_0522;
        const V1: u32 = 0x1998_0330;
        let full: u32 = 0xFFFF_FFFF;

        match version {
            V3 => {
                // effective[0], effective[1], permitted[0], permitted[1],
                // inheritable[0], inheritable[1]
                let mut data = [0u8; 24];
                // cap_user_data has: effective, permitted, inheritable (each u32)
                // For V3: two structs of {effective, permitted, inheritable}
                // Struct 1 (low 32 bits):
                data[0..4].copy_from_slice(&full.to_ne_bytes()); // effective
                data[4..8].copy_from_slice(&full.to_ne_bytes()); // permitted
                data[8..12].copy_from_slice(&full.to_ne_bytes()); // inheritable
                                                                  // Struct 2 (high 32 bits):
                data[12..16].copy_from_slice(&full.to_ne_bytes()); // effective
                data[16..20].copy_from_slice(&full.to_ne_bytes()); // permitted
                data[20..24].copy_from_slice(&full.to_ne_bytes()); // inheritable
                datap.write_array(&data)?;
            }
            V1 => {
                let mut data = [0u8; 12];
                data[0..4].copy_from_slice(&full.to_ne_bytes()); // effective
                data[4..8].copy_from_slice(&full.to_ne_bytes()); // permitted
                data[8..12].copy_from_slice(&full.to_ne_bytes()); // inheritable
                datap.write_array(&data)?;
            }
            _ => {
                // Unknown version — return EINVAL
                return Err(LxError::EINVAL);
            }
        }
        Ok(0)
    }

    /// Set process capabilities.
    ///
    /// Since all processes run as root with full capabilities, we
    /// accept any capset request silently.
    pub fn sys_capset(&self, hdrp: UserInPtr<u8>, _datap: UserInPtr<u8>) -> SysResult {
        let hdr = hdrp.read_array(8)?;
        let version = u32::from_ne_bytes(hdr[0..4].try_into().unwrap());
        info!("capset: version={:#x}", version);
        // Accept silently — all processes have full capabilities
        Ok(0)
    }

    /// Set the file mode creation mask. Returns the
    /// previous value.
    pub fn sys_umask(&self, mask: u32) -> SysResult {
        let old = self.linux_process().set_umask(mask);
        info!("umask: mask={:#o} => old={:#o}", mask, old);
        Ok(old as usize)
    }

    // --- Scheduler and resource priority syscalls ---

    /// Set process/user priority (nice value).
    pub fn sys_setpriority(&self, which: usize, who: usize, prio: i32) -> SysResult {
        info!("setpriority: which={}, who={}, prio={}", which, who, prio);
        // Accept silently — single-priority system
        Ok(0)
    }

    /// Get process/user priority.
    /// Linux encodes nice as 20-nice, so default nice 0 returns 20.
    pub fn sys_getpriority(&self, which: usize, who: usize) -> SysResult {
        info!("getpriority: which={}, who={}", which, who);
        // Return default nice value (20 = nice 0, per Linux convention)
        Ok(20)
    }

    /// Set scheduling parameters (priority).
    pub fn sys_sched_setparam(&self, pid: usize, _param: UserInPtr<u8>) -> SysResult {
        info!("sched_setparam: pid={}", pid);
        // Only priority 0 is valid for SCHED_OTHER
        Ok(0)
    }

    /// Get scheduling parameters.
    pub fn sys_sched_getparam(&self, pid: usize, mut param: UserOutPtr<u32>) -> SysResult {
        info!("sched_getparam: pid={}", pid);
        // Return sched_priority = 0 (only valid for SCHED_OTHER)
        param.write(0)?;
        Ok(0)
    }

    /// Set scheduling policy and parameters.
    pub fn sys_sched_setscheduler(
        &self,
        pid: usize,
        policy: usize,
        _param: UserInPtr<u8>,
    ) -> SysResult {
        info!("sched_setscheduler: pid={}, policy={}", pid, policy);
        const SCHED_OTHER: usize = 0;
        if policy != SCHED_OTHER {
            // Only SCHED_OTHER is supported
            return Err(LxError::EINVAL);
        }
        Ok(0)
    }

    /// Get scheduling policy.
    pub fn sys_sched_getscheduler(&self, pid: usize) -> SysResult {
        info!("sched_getscheduler: pid={}", pid);
        // Always SCHED_OTHER (0)
        Ok(0)
    }

    /// Get maximum priority for a scheduling policy.
    pub fn sys_sched_get_priority_max(&self, policy: usize) -> SysResult {
        info!("sched_get_priority_max: policy={}", policy);
        const SCHED_OTHER: usize = 0;
        const SCHED_FIFO: usize = 1;
        const SCHED_RR: usize = 2;
        match policy {
            SCHED_OTHER => Ok(0),
            SCHED_FIFO | SCHED_RR => Ok(99),
            _ => Err(LxError::EINVAL),
        }
    }

    /// Get minimum priority for a scheduling policy.
    pub fn sys_sched_get_priority_min(&self, policy: usize) -> SysResult {
        info!("sched_get_priority_min: policy={}", policy);
        const SCHED_OTHER: usize = 0;
        const SCHED_FIFO: usize = 1;
        const SCHED_RR: usize = 2;
        match policy {
            SCHED_OTHER => Ok(0),
            SCHED_FIFO | SCHED_RR => Ok(1),
            _ => Err(LxError::EINVAL),
        }
    }

    /// Get the round-robin time quantum for a process.
    pub fn sys_sched_rr_get_interval(
        &self,
        pid: usize,
        mut interval: UserOutPtr<[u64; 2]>,
    ) -> SysResult {
        info!("sched_rr_get_interval: pid={}", pid);
        // Return 100ms quantum (timespec: sec=0, nsec=100_000_000)
        interval.write([0, 100_000_000])?;
        Ok(0)
    }

    /// Set process group ID. If pid==0, uses the calling
    /// process. If pgid==0, the target becomes its own
    /// process group leader. Negative pgid values are rejected.
    pub fn sys_setpgid(&self, pid: usize, pgid: isize) -> SysResult {
        if pgid < 0 {
            return Err(LxError::EINVAL);
        }
        let proc = self.zircon_process();
        let target_pid = if pid == 0 { proc.id() } else { pid as u64 };
        let new_pgid = if pgid == 0 { target_pid } else { pgid as u64 };
        info!(
            "setpgid: pid={} pgid={} => target_pid={} new_pgid={}",
            pid, pgid, target_pid, new_pgid
        );
        if target_pid == proc.id() {
            // Setting own PGID
            self.linux_process().set_pgid(new_pgid);
        } else {
            // Setting a child process's PGID
            self.linux_process().set_child_pgid(target_pid, new_pgid)?;
        }
        Ok(0)
    }

    /// Get process group ID. If pid==0, returns the PGID of
    /// the calling process. Cross-process lookup is not yet
    /// supported and returns ENOSYS.
    pub fn sys_getpgid(&self, pid: usize) -> SysResult {
        if pid != 0 && pid as u64 != self.zircon_process().id() {
            warn!("getpgid: cross-process lookup not supported (pid={})", pid);
            return Err(LxError::ENOSYS);
        }
        let pgid = self.linux_process().pgid();
        info!("getpgid: pid={} => {}", pid, pgid);
        Ok(pgid as usize)
    }

    /// Create a new session. Sets session_id and pgid to the
    /// calling process's PID. Returns the new session ID.
    pub fn sys_setsid(&self) -> SysResult {
        let pid = self.zircon_process().id();
        self.linux_process().setsid(pid);
        info!("setsid: pid={} => sid={}", pid, pid);
        Ok(pid as usize)
    }

    /// Get the session ID. If pid==0, returns the session ID
    /// of the calling process. Cross-process lookup is not yet
    /// supported and returns ENOSYS.
    pub fn sys_getsid(&self, pid: usize) -> SysResult {
        if pid != 0 && pid as u64 != self.zircon_process().id() {
            warn!("getsid: cross-process lookup not supported (pid={})", pid);
            return Err(LxError::ENOSYS);
        }
        let sid = self.linux_process().session_id();
        info!("getsid: pid={} => {}", pid, sid);
        Ok(sid as usize)
    }

    /// Get the supplementary group IDs. If size==0, returns
    /// the number of groups. Otherwise copies up to `size`
    /// group IDs to the user buffer.
    pub fn sys_getgroups(&self, size: i32, mut list: UserOutPtr<u32>) -> SysResult {
        let groups = self.linux_process().groups();
        info!("getgroups: size={} ngroups={}", size, groups.len());
        if size == 0 {
            return Ok(groups.len());
        }
        if size < 0 {
            return Err(LxError::EINVAL);
        }
        if (size as usize) < groups.len() {
            return Err(LxError::EINVAL);
        }
        list.write_array(&groups)?;
        Ok(groups.len())
    }

    /// Set the supplementary group IDs. Requires privilege
    /// (euid == 0).
    pub fn sys_setgroups(&self, size: usize, list: UserInPtr<u32>) -> SysResult {
        info!("setgroups: size={}", size);
        if self.linux_process().euid() != 0 {
            return Err(LxError::EPERM);
        }
        // Linux NGROUPS_MAX
        if size > 65_536 {
            return Err(LxError::EINVAL);
        }
        let groups = if size > 0 {
            list.read_array(size)?
        } else {
            Vec::new()
        };
        self.linux_process().set_groups(groups);
        Ok(0)
    }
}

bitflags! {
    /// for op argument in futex()
    struct FutexFlags: u32 {
        /// tests that the value at the futex word pointed
        /// to by the address uaddr still contains the expected value val,
        /// and if so, then sleeps waiting for a FUTEX_WAKE operation on the futex word.
        const WAIT      = 0;
        /// wakes at most val of the waiters that are waiting on the futex word at the address uaddr.
        const WAKE      = 1;
        /// wakes up a maximum of val waiters that are waiting on the futex at uaddr.  If there are more than val waiters, then the remaining waiters are removed from the wait queue of the source futex at uaddr and added to the wait queue of the target futex at uaddr2.  The val2 argument specifies an upper limit on the number of waiters that are requeued to the futex at uaddr2.
        const REQUEUE   = 3;
        /// like REQUEUE, but first checks that the value at uaddr matches val3.
        const CMP_REQUEUE = 4;
        /// (unsupported) is used after an attempt to acquire the lock via an atomic user-mode instruction failed.
        const LOCK_PI   = 6;
        /// (unsupported) is called when the user-space value at uaddr cannot be changed atomically from a TID (of the owner) to 0.
        const UNLOCK_PI = 7;
        /// can be employed with all futex operations, tells the kernel that the futex is process-private and not shared with another process
        const PRIVATE   = 0x80;
    }
}

impl Syscall<'_> {
    /// Create a pair of connected Unix domain sockets.
    ///
    /// Only `AF_UNIX` (domain=1) with `SOCK_STREAM` is supported.
    /// Returns two file descriptors in `sv` that are bidirectional.
    pub fn sys_socketpair(
        &self,
        domain: usize,
        socket_type: usize,
        _protocol: usize,
        mut sv: UserOutPtr<[i32; 2]>,
    ) -> SysResult {
        const AF_UNIX: usize = 1;
        const SOCK_STREAM: usize = 1;
        #[allow(dead_code)]
        const SOCK_CLOEXEC: usize = 0o2000000;
        const SOCK_NONBLOCK: usize = 0o4000;

        info!(
            "socketpair: domain={}, type={:#x}, protocol={}",
            domain, socket_type, _protocol
        );

        if domain != AF_UNIX {
            return Err(LxError::EAFNOSUPPORT);
        }
        let base_type = socket_type & 0xf;
        if base_type != SOCK_STREAM {
            warn!(
                "socketpair: only SOCK_STREAM is supported, got {}",
                base_type
            );
            return Err(LxError::ENOSYS);
        }

        let (a, b) = linux_object::fs::unix_socket::UnixSocketEnd::create_pair();

        // Apply flags
        if socket_type & SOCK_NONBLOCK != 0 {
            use linux_object::fs::{FileLike, OpenFlags};
            let _ = FileLike::set_flags(a.as_ref(), OpenFlags::RDWR | OpenFlags::NON_BLOCK);
            let _ = FileLike::set_flags(b.as_ref(), OpenFlags::RDWR | OpenFlags::NON_BLOCK);
        }

        let proc = self.linux_process();
        let fd_a: i32 = proc.add_file(a)?.into();
        let fd_b: i32 = proc.add_file(b)?.into();

        info!("socketpair: fd_a={}, fd_b={}", fd_a, fd_b);
        sv.write([fd_a, fd_b])?;
        Ok(0)
    }

    /// Create an endpoint for communication.
    ///
    /// Only `AF_UNIX` (domain=1) with `SOCK_STREAM` or `SOCK_DGRAM` is
    /// supported. Returns a file descriptor for the new socket.
    pub fn sys_socket(&self, domain: usize, socket_type: usize, protocol: usize) -> SysResult {
        const AF_UNIX: usize = 1;
        const SOCK_STREAM: usize = 1;
        const SOCK_DGRAM: usize = 2;
        const SOCK_NONBLOCK: usize = 0o4000;

        info!(
            "socket: domain={}, type={:#x}, protocol={}",
            domain, socket_type, protocol
        );

        if domain != AF_UNIX {
            warn!("socket: only AF_UNIX is supported, got domain={}", domain);
            return Err(LxError::EAFNOSUPPORT);
        }
        let base_type = socket_type & 0xf;
        if base_type != SOCK_STREAM && base_type != SOCK_DGRAM {
            warn!("socket: unsupported type {}", base_type);
            return Err(LxError::ENOSYS);
        }

        // Create an unconnected socket (a pair where the peer side is
        // not yet attached). We create a self-connected pair and return
        // one end; the other end is discarded (will show as peer-closed
        // until connect() is implemented).
        let (a, _b) = linux_object::fs::unix_socket::UnixSocketEnd::create_pair();

        if socket_type & SOCK_NONBLOCK != 0 {
            use linux_object::fs::{FileLike, OpenFlags};
            let _ = FileLike::set_flags(a.as_ref(), OpenFlags::RDWR | OpenFlags::NON_BLOCK);
        }

        let proc = self.linux_process();
        let fd: i32 = proc.add_file(a)?.into();
        info!("socket: fd={}", fd);
        Ok(fd as usize)
    }

    /// Shut down part of a full-duplex connection.
    pub fn sys_shutdown(&self, fd: FileDesc, how: usize) -> SysResult {
        info!("shutdown: fd={:?}, how={}", fd, how);
        let proc = self.linux_process();
        // Just close the fd — full shutdown semantics not implemented.
        proc.close_file(fd)?;
        Ok(0)
    }

    /// Get socket name (local address).
    pub fn sys_getsockname(
        &self,
        fd: FileDesc,
        mut addr: UserOutPtr<u8>,
        mut addrlen: UserOutPtr<u32>,
    ) -> SysResult {
        info!("getsockname: fd={:?}", fd);
        // Return AF_UNIX with empty path (unnamed socket).
        let sa_family: u16 = 1; // AF_UNIX
        addr.write_array(&sa_family.to_ne_bytes())?;
        addrlen.write(2)?; // sizeof(sa_family_t)
        Ok(0)
    }

    /// Get peer socket name.
    pub fn sys_getpeername(
        &self,
        fd: FileDesc,
        mut addr: UserOutPtr<u8>,
        mut addrlen: UserOutPtr<u32>,
    ) -> SysResult {
        info!("getpeername: fd={:?}", fd);
        let sa_family: u16 = 1; // AF_UNIX
        addr.write_array(&sa_family.to_ne_bytes())?;
        addrlen.write(2)?;
        Ok(0)
    }

    /// Set socket options.
    pub fn sys_setsockopt(
        &self,
        fd: FileDesc,
        level: usize,
        optname: usize,
        _optval: UserInPtr<u8>,
        _optlen: usize,
    ) -> SysResult {
        info!(
            "setsockopt: fd={:?}, level={}, optname={}",
            fd, level, optname
        );
        // Accept silently — most socket options are irrelevant for AF_UNIX.
        Ok(0)
    }

    /// Get socket options.
    pub fn sys_getsockopt(
        &self,
        fd: FileDesc,
        level: usize,
        optname: usize,
        mut optval: UserOutPtr<u32>,
        mut optlen: UserOutPtr<u32>,
    ) -> SysResult {
        info!(
            "getsockopt: fd={:?}, level={}, optname={}",
            fd, level, optname
        );
        const SOL_SOCKET: usize = 1;
        const SO_ERROR: usize = 4;
        const SO_TYPE: usize = 3;
        const SOCK_STREAM: u32 = 1;

        if level == SOL_SOCKET {
            match optname {
                SO_ERROR => {
                    optval.write(0)?; // no error
                    optlen.write(4)?;
                }
                SO_TYPE => {
                    optval.write(SOCK_STREAM)?;
                    optlen.write(4)?;
                }
                _ => {
                    optval.write(0)?;
                    optlen.write(4)?;
                }
            }
        } else {
            optval.write(0)?;
            optlen.write(4)?;
        }
        Ok(0)
    }

    /// Bind a socket to a local address (path for AF_UNIX).
    pub fn sys_bind(&self, fd: FileDesc, addr: UserInPtr<u8>, addrlen: usize) -> SysResult {
        // Parse sockaddr_un: sa_family (2 bytes) + sun_path (up to 108 bytes)
        if addrlen < 3 {
            return Err(LxError::EINVAL);
        }
        let buf = addr.read_array(addrlen.min(110))?;
        let _sa_family = u16::from_ne_bytes([buf[0], buf[1]]);
        // Extract path (null-terminated)
        let path_bytes = &buf[2..];
        let path_len = path_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(path_bytes.len());
        let path = core::str::from_utf8(&path_bytes[..path_len]).map_err(|_| LxError::EINVAL)?;
        info!("bind: fd={:?}, path={:?}", fd, path);

        // Verify fd is a socket
        let proc = self.linux_process();
        let _file = proc.get_file_like(fd)?;

        // Register the listener (bind doesn't create the queue yet,
        // but we register early for simplicity — listen() is a no-op).
        linux_object::fs::unix_socket::bind_listener(
            alloc::string::String::from(path),
            128, // default backlog
        )?;
        Ok(0)
    }

    /// Mark a socket as passive (willing to accept connections).
    pub fn sys_listen(&self, fd: FileDesc, backlog: usize) -> SysResult {
        info!("listen: fd={:?}, backlog={}", fd, backlog);
        // The listener was already created in bind(). listen() is a no-op.
        let proc = self.linux_process();
        let _file = proc.get_file_like(fd)?;
        Ok(0)
    }

    /// Accept a connection on a listening socket.
    ///
    /// Returns a new file descriptor for the accepted connection.
    pub fn sys_accept(
        &self,
        fd: FileDesc,
        mut addr: UserOutPtr<u8>,
        mut addrlen: UserOutPtr<u32>,
    ) -> SysResult {
        info!("accept: fd={:?}", fd);
        let proc = self.linux_process();
        let _file = proc.get_file_like(fd)?;

        // Find the listener associated with this fd.
        // For simplicity, check all listeners for pending connections.
        let listeners = linux_object::fs::unix_socket::UNIX_LISTENERS.lock();
        for listener in listeners.values() {
            if let Some(conn) = listener.pop_connection() {
                let new_fd: i32 = proc.add_file(conn)?.into();
                // Write peer address if requested
                if !addr.is_null() {
                    let sa_family: u16 = 1; // AF_UNIX
                    let _ = addr.write_array(&sa_family.to_ne_bytes());
                    if !addrlen.is_null() {
                        let _ = addrlen.write(2);
                    }
                }
                info!("accept: new_fd={}", new_fd);
                return Ok(new_fd as usize);
            }
        }
        // No pending connections — would block.
        // TODO: async wait for connections.
        Err(LxError::EAGAIN)
    }

    /// Connect a socket to a remote address (path for AF_UNIX).
    pub fn sys_connect(&self, fd: FileDesc, addr: UserInPtr<u8>, addrlen: usize) -> SysResult {
        if addrlen < 3 {
            return Err(LxError::EINVAL);
        }
        let buf = addr.read_array(addrlen.min(110))?;
        let _sa_family = u16::from_ne_bytes([buf[0], buf[1]]);
        let path_bytes = &buf[2..];
        let path_len = path_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(path_bytes.len());
        let path = core::str::from_utf8(&path_bytes[..path_len]).map_err(|_| LxError::EINVAL)?;
        info!("connect: fd={:?}, path={:?}", fd, path);

        let proc = self.linux_process();
        // Close the old unconnected socket end
        proc.close_file(fd)?;

        // Connect to the listener and get the client end
        let client_end = linux_object::fs::unix_socket::connect_to(path)?;

        // Add the connected socket as the same fd
        proc.add_file_at(fd, client_end)?;
        Ok(0)
    }

    /// Send data on a socket.
    ///
    /// For connected AF_UNIX sockets, `dest_addr` is ignored (must be
    /// null or the destination is already established by `connect`).
    pub fn sys_sendto(
        &self,
        fd: FileDesc,
        buf: UserInPtr<u8>,
        len: usize,
        flags: usize,
        _dest_addr: UserInPtr<u8>,
        _addrlen: usize,
    ) -> SysResult {
        info!("sendto: fd={:?}, len={}, flags={:#x}", fd, len, flags);
        let proc = self.linux_process();
        let file = proc.get_file_like(fd)?;
        let data = buf.read_array(len)?;
        let written = file.write(&data)?;
        Ok(written)
    }

    /// Receive data from a socket.
    ///
    /// For connected AF_UNIX sockets, `src_addr` is filled with
    /// AF_UNIX family if non-null.
    pub async fn sys_recvfrom(
        &self,
        fd: FileDesc,
        mut buf: UserOutPtr<u8>,
        len: usize,
        flags: usize,
        mut src_addr: UserOutPtr<u8>,
        mut addrlen: UserOutPtr<u32>,
    ) -> SysResult {
        info!("recvfrom: fd={:?}, len={}, flags={:#x}", fd, len, flags);
        let proc = self.linux_process();
        let file = proc.get_file_like(fd)?;

        const MSG_PEEK: usize = 2;
        if flags & MSG_PEEK != 0 {
            warn!("recvfrom: MSG_PEEK not supported");
        }

        let mut data = vec![0u8; len];
        let n = file.read(&mut data).await?;
        buf.write_array(&data[..n])?;

        // Fill source address if requested
        if !src_addr.is_null() {
            let sa_family: u16 = 1; // AF_UNIX
            let _ = src_addr.write_array(&sa_family.to_ne_bytes());
            if !addrlen.is_null() {
                let _ = addrlen.write(2);
            }
        }
        Ok(n)
    }

    /// Send a message on a socket with structured data (iovec + ancillary).
    ///
    /// Supports SCM_RIGHTS for passing file descriptors over AF_UNIX.
    pub fn sys_sendmsg(&self, fd: FileDesc, msg_ptr: UserInPtr<u8>, flags: usize) -> SysResult {
        info!("sendmsg: fd={:?}, flags={:#x}", fd, flags);

        // Read the msghdr structure from userspace.
        // Layout (LP64): msg_name(8) + msg_namelen(4) + pad(4) +
        //   msg_iov(8) + msg_iovlen(8) + msg_control(8) +
        //   msg_controllen(8) + msg_flags(4)
        let hdr_bytes = msg_ptr.read_array(56)?;
        let msg_iov_ptr = usize::from_ne_bytes(hdr_bytes[16..24].try_into().unwrap());
        let msg_iovlen = usize::from_ne_bytes(hdr_bytes[24..32].try_into().unwrap());
        let msg_control_ptr = usize::from_ne_bytes(hdr_bytes[32..40].try_into().unwrap());
        let msg_controllen = usize::from_ne_bytes(hdr_bytes[40..48].try_into().unwrap());

        // Gather data from iovec array
        let iov_in: UserInPtr<IoVecIn> = msg_iov_ptr.into();
        let iovs = iov_in.read_iovecs(msg_iovlen)?;
        let data = iovs.read_to_vec()?;

        // Parse ancillary data (control messages) for SCM_RIGHTS
        let ancillary = if msg_control_ptr != 0 && msg_controllen > 0 {
            self.parse_cmsg_scm_rights(msg_control_ptr, msg_controllen)?
        } else {
            None
        };

        // Send via the socket
        let proc = self.linux_process();
        let file = proc.get_file_like(fd)?;
        let socket = file
            .as_socket()?
            .downcast_ref::<linux_object::fs::unix_socket::UnixSocketEnd>()
            .ok_or(LxError::ENOTSOCK)?;
        let written = socket.send_with_fds(&data, ancillary)?;
        Ok(written)
    }

    /// Receive a message from a socket with structured data (iovec + ancillary).
    ///
    /// Supports SCM_RIGHTS for receiving file descriptors over AF_UNIX.
    pub async fn sys_recvmsg(
        &self,
        fd: FileDesc,
        msg_ptr: UserInPtr<u8>,
        flags: usize,
    ) -> SysResult {
        info!("recvmsg: fd={:?}, flags={:#x}", fd, flags);

        // Read the msghdr structure from userspace
        let hdr_bytes = msg_ptr.read_array(56)?;
        let msg_iov_ptr = usize::from_ne_bytes(hdr_bytes[16..24].try_into().unwrap());
        let msg_iovlen = usize::from_ne_bytes(hdr_bytes[24..32].try_into().unwrap());
        let msg_control_ptr = usize::from_ne_bytes(hdr_bytes[32..40].try_into().unwrap());
        let msg_controllen = usize::from_ne_bytes(hdr_bytes[40..48].try_into().unwrap());

        // Read iovec array to determine buffer size
        let iov_out: UserInPtr<IoVecOut> = msg_iov_ptr.into();
        let mut iovs = iov_out.read_iovecs(msg_iovlen)?;
        let total_len = iovs.total_len();

        // Receive data + ancillary fds from the socket
        let proc = self.linux_process();
        let file = proc.get_file_like(fd)?;
        let socket = file
            .as_socket()?
            .downcast_ref::<linux_object::fs::unix_socket::UnixSocketEnd>()
            .ok_or(LxError::ENOTSOCK)?;

        let mut data = vec![0u8; total_len];
        let (n, ancillary) = socket.recv_with_fds(&mut data).await?;

        // Scatter data into iovec buffers
        iovs.write_from_buf(&data[..n])?;

        // Build ancillary response (SCM_RIGHTS) if fds were received
        let controllen_out = if let Some(ref anc) = ancillary {
            if !anc.fds.is_empty() && msg_control_ptr != 0 && msg_controllen > 0 {
                self.build_cmsg_scm_rights(msg_control_ptr, msg_controllen, &anc.fds)?
            } else {
                0usize
            }
        } else {
            0usize
        };

        // Update msg_controllen in the msghdr to reflect actual ancillary data written
        let controllen_bytes = controllen_out.to_ne_bytes();
        // msg_controllen is at offset 40 in msghdr
        let mut controllen_ptr: UserOutPtr<u8> = (msg_ptr.as_addr() + 40).into();
        controllen_ptr.write_array(&controllen_bytes)?;

        // Set msg_flags to 0 (offset 48 in msghdr)
        let mut flags_ptr: UserOutPtr<u8> = (msg_ptr.as_addr() + 48).into();
        flags_ptr.write_array(&0u32.to_ne_bytes())?;

        Ok(n)
    }

    /// Parse SCM_RIGHTS control messages from userspace.
    ///
    /// Extracts file descriptors from `cmsghdr` with `cmsg_type == SCM_RIGHTS`.
    fn parse_cmsg_scm_rights(
        &self,
        control_ptr: usize,
        controllen: usize,
    ) -> Result<Option<linux_object::fs::unix_socket::AncillaryFds>, LxError> {
        use linux_object::fs::unix_socket::AncillaryFds;

        const SOL_SOCKET: u32 = 1;
        const SCM_RIGHTS: u32 = 1;
        // cmsghdr: cmsg_len(8) + cmsg_level(4) + cmsg_type(4) = 16 bytes header
        const CMSG_HDR_SIZE: usize = 16;
        // cmsghdr alignment (8 bytes on LP64)
        const CMSG_ALIGN: usize = 8;

        let ctrl_in: UserInPtr<u8> = control_ptr.into();
        let ctrl_bytes = ctrl_in.read_array(controllen)?;

        let proc = self.linux_process();
        let mut fds_out = alloc::vec::Vec::new();
        let mut offset = 0;

        while offset + CMSG_HDR_SIZE <= controllen {
            let cmsg_len = usize::from_ne_bytes(ctrl_bytes[offset..offset + 8].try_into().unwrap());
            if cmsg_len < CMSG_HDR_SIZE || offset + cmsg_len > controllen {
                break;
            }
            let cmsg_level =
                u32::from_ne_bytes(ctrl_bytes[offset + 8..offset + 12].try_into().unwrap());
            let cmsg_type =
                u32::from_ne_bytes(ctrl_bytes[offset + 12..offset + 16].try_into().unwrap());

            if cmsg_level == SOL_SOCKET && cmsg_type == SCM_RIGHTS {
                let data_len = cmsg_len - CMSG_HDR_SIZE;
                let fd_count = data_len / 4; // each fd is an i32
                for i in 0..fd_count {
                    let fd_offset = offset + CMSG_HDR_SIZE + i * 4;
                    let raw_fd = i32::from_ne_bytes(
                        ctrl_bytes[fd_offset..fd_offset + 4].try_into().unwrap(),
                    );
                    let fd = FileDesc::from(raw_fd);
                    let file = proc.get_file_like(fd)?;
                    fds_out.push(file);
                }
            }
            // Advance to next cmsghdr (aligned)
            offset += (cmsg_len + CMSG_ALIGN - 1) & !(CMSG_ALIGN - 1);
        }

        if fds_out.is_empty() {
            Ok(None)
        } else {
            Ok(Some(AncillaryFds { fds: fds_out }))
        }
    }

    /// Build SCM_RIGHTS control message in userspace buffer.
    ///
    /// Returns the total number of bytes written to the control buffer.
    fn build_cmsg_scm_rights(
        &self,
        control_ptr: usize,
        controllen: usize,
        fds: &[Arc<dyn linux_object::fs::FileLike>],
    ) -> Result<usize, LxError> {
        const SOL_SOCKET: u32 = 1;
        const SCM_RIGHTS: u32 = 1;
        const CMSG_HDR_SIZE: usize = 16;

        let data_len = fds.len() * 4; // each fd is an i32
        let cmsg_len = CMSG_HDR_SIZE + data_len;
        if cmsg_len > controllen {
            // Not enough space — silently truncate (MSG_CTRUNC)
            return Ok(0);
        }

        let proc = self.linux_process();

        // Build the cmsghdr + fd array
        let mut cmsg = alloc::vec![0u8; cmsg_len];
        cmsg[0..8].copy_from_slice(&(cmsg_len as u64).to_ne_bytes());
        cmsg[8..12].copy_from_slice(&SOL_SOCKET.to_ne_bytes());
        cmsg[12..16].copy_from_slice(&SCM_RIGHTS.to_ne_bytes());

        for (i, file) in fds.iter().enumerate() {
            // Dup the file into the receiving process's fd table
            let duped = file.dup()?;
            let new_fd: i32 = proc.add_file(duped)?.into();
            let offset = CMSG_HDR_SIZE + i * 4;
            cmsg[offset..offset + 4].copy_from_slice(&new_fd.to_ne_bytes());
        }

        let mut ctrl_out: UserOutPtr<u8> = control_ptr.into();
        ctrl_out.write_array(&cmsg)?;
        Ok(cmsg_len)
    }
}

const USER_STACK_SIZE: usize = 8 * 1024 * 1024; // 8 MB, the default config of Linux

const RLIMIT_STACK: usize = 3;
const RLIMIT_RSS: usize = 5;
const RLIMIT_NOFILE: usize = 7;
const RLIMIT_AS: usize = 9;

/// sysinfo() return information sturct
#[repr(C)]
#[derive(Debug, Default)]
pub struct SysInfo {
    /// Seconds since boot
    uptime: u64,
    /// 1, 5, and 15 minute load averages
    loads: [u64; 3],
    /// Total usable main memory size
    totalram: u64,
    /// Available memory size
    freeram: u64,
    /// Amount of shared memory
    sharedram: u64,
    /// Memory used by buffers
    bufferram: u64,
    /// Total swa Total swap space sizep space size
    totalswap: u64,
    /// swap space still available
    freeswap: u64,
    /// Number of current processes
    procs: u16,
    /// Total high memory size
    totalhigh: u64,
    /// Available high memory size
    freehigh: u64,
    /// Memory unit size in bytes
    mem_unit: u32,
}
