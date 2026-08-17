use super::*;
use bitflags::bitflags;
use core::time::Duration;
use kernel_hal::timer::timer_now;
use linux_object::thread::ThreadExt;
use linux_object::time::*;
use zircon_object::task::ThreadState;

impl Syscall<'_> {
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
                    ctx.set_field(kernel_hal::context::UserContextField::ThreadPointer, addr)
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

        let vdso_const = kernel_hal::vdso::vdso_constants();

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
        use kernel_hal::timer;
        // Note: timer_now() is boot-relative in bare-metal mode (correct
        // for uptime) but Unix-epoch-based in libos mode.
        let uptime = timer::timer_now().as_secs();
        // Approximate fixed values; live memory/process accounting
        // is not yet implemented.
        let sysinfo = SysInfo {
            uptime,
            totalram: 128 * 1024 * 1024, // 128 MiB
            freeram: 64 * 1024 * 1024,   // 64 MiB
            mem_unit: 1,
            procs: 1,
            ..SysInfo::default()
        };
        sys_info.write(sysinfo)?;
        Ok(0)
    }

    /// provides a method for waiting until a certain condition becomes true.
    /// - `uaddr` - points to the futex word.
    /// - `op` -  the operation to perform on the futex
    /// - `val` -  a value whose meaning and purpose depends on op
    /// - `val2` - provides a timeout for the attempt or acts as val2 when op is REQUEUE
    /// - `uaddr2` - when op is REQUEUE, points to the target futex
    /// - `_val3` - is not used
    pub async fn sys_futex(
        &self,
        uaddr: usize,
        op: u32,
        val: u32,
        val2: usize,
        uaddr2: usize,
        _val3: u32,
    ) -> SysResult {
        debug!(
            "Futex uaddr: {:#x}, op: {:x}, val: {}, val2(timeout_addr): {:x}",
            uaddr, op, val, val2,
        );
        let op = FutexFlags::from_bits_truncate(op);
        if !op.contains(FutexFlags::PRIVATE) {
            warn!("process-shared futex is unimplemented");
            // return Err(LxError::ENOSYS);
        }
        let op = op - FutexFlags::PRIVATE;
        let futex = self.linux_process().get_futex(uaddr);
        match op {
            FutexFlags::WAIT => {
                // Check for pending signals before blocking
                if self.thread.lock_linux().has_pending_signal() {
                    return Err(LxError::EINTR);
                }
                let future = futex.wait(val as _);
                let timeout_addr: UserInPtr<TimeSpec> = val2.into();
                let res = if let Some(timeout) = timeout_addr.read_if_not_null().unwrap() {
                    self.thread
                        .blocking_run(
                            future,
                            ThreadState::BlockedFutex,
                            timer_now() + Duration::from(timeout),
                            None,
                        )
                        .await
                } else {
                    future.await
                };
                // Check for pending signals after wakeup
                if self.thread.lock_linux().has_pending_signal() {
                    return Err(LxError::EINTR);
                }
                match res {
                    Ok(_) => Ok(0),
                    Err(e) => Err(e.into()),
                }
            }
            FutexFlags::WAKE => Ok(futex.wake(val as _)),
            FutexFlags::REQUEUE => {
                let requeue_futex = self.linux_process().get_futex(uaddr2);
                let res = futex.requeue(0, val as _, val2, &requeue_futex, None, false);
                match res {
                    Ok(_) => Ok(0),
                    Err(e) => Err(e.into()),
                }
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
                kernel_hal::cpu::reset(); // PSCI SYSTEM_OFF
            }
            LINUX_REBOOT_CMD_RESTART => {
                warn!("system restart");
                kernel_hal::cpu::reset(); // TODO: use PSCI SYSTEM_RESET
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
        kernel_hal::rand::fill_random(&mut buffer);
        buf.write_array(&buffer[..len])?;
        Ok(len)
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
        /// (unsupported) is used after an attempt to acquire the lock via an atomic user-mode instruction failed.
        const LOCK_PI   = 6;
        /// (unsupported) is called when the user-space value at uaddr cannot be changed atomically from a TID (of the owner) to 0.
        const UNLOCK_PI = 7;
        /// can be employed with all futex operations, tells the kernel that the futex is process-private and not shared with another process
        const PRIVATE   = 0x80;
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
