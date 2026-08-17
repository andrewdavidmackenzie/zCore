//! Syscalls for time
//! - clock_gettime
//!
use crate::Syscall;
use core::convert::TryFrom;
use kernel_hal::{user::UserInPtr, user::UserOutPtr};
use linux_object::error::LxError;
use linux_object::error::SysResult;
use linux_object::signal::Signal as LinuxSignal;
use linux_object::thread::ThreadExt;
use linux_object::time::*;

const USEC_PER_TICK: usize = 10000;

impl Syscall<'_> {
    /// Returns the resolution (precision) of the specified clock.
    ///
    /// If `buf` is non-NULL, stores the resolution in the struct timespec
    /// pointed to by `buf`. The resolution is 1 nanosecond for all supported clocks.
    /// Returns `EINVAL` for unknown clock IDs.
    pub fn sys_clock_getres(&self, clock: usize, mut buf: UserOutPtr<TimeSpec>) -> SysResult {
        info!("clock_getres: id={}, buf={:?}", clock, buf);
        // Validate clock ID (0..=9 are the supported ClockId variants)
        if clock > 9 {
            return Err(LxError::EINVAL);
        }
        // All supported clocks report 1ns resolution
        if !buf.is_null() {
            buf.write(TimeSpec { sec: 0, nsec: 1 })?;
        }
        Ok(0)
    }

    /// finds the resolution (precision) of the specified clock clockid, and,
    /// if buffer is non-NULL, stores it in the struct timespec pointed to by buffer
    pub fn sys_clock_gettime(&self, clock: usize, mut buf: UserOutPtr<TimeSpec>) -> SysResult {
        info!("clock_gettime: id={:?} buf={:?}", clock, buf);
        if buf.is_null() {
            return Err(LxError::EINVAL);
        }
        let ts = TimeSpec::now();
        buf.write(ts)?;

        info!("TimeSpec: {:?}", ts);

        Ok(0)
    }

    /// Set the time of the specified clock.
    ///
    /// Setting the system clock requires `CAP_SYS_TIME` (root).
    /// Since zCore always runs as root but does not support clock
    /// modification, this always returns `EPERM`.
    pub fn sys_clock_settime(&self, clock: usize, buf: UserInPtr<TimeSpec>) -> SysResult {
        info!("clock_settime: id={}, buf={:?}", clock, buf);
        Err(LxError::EPERM)
    }

    /// get the time with second and microseconds
    pub fn sys_gettimeofday(
        &mut self,
        mut tv: UserOutPtr<TimeVal>,
        tz: UserInPtr<u8>,
    ) -> SysResult {
        info!("gettimeofday: tv: {:?}, tz: {:?}", tv, tz);
        // don't support tz
        if !tz.is_null() {
            return Err(LxError::EINVAL);
        }

        let timeval = TimeVal::now();
        tv.write(timeval)?;

        info!("TimeVal: {:?}", timeval);

        Ok(0)
    }

    /// get time in seconds
    #[cfg(target_arch = "x86_64")]
    pub fn sys_time(&mut self, mut time: UserOutPtr<u64>) -> SysResult {
        info!("time: time: {:?}", time);
        if time.is_null() {
            return Err(LxError::EINVAL);
        }
        let sec = TimeSpec::now().sec;
        time.write(sec as u64)?;
        Ok(sec)
    }

    /// JUST FOR TEST, DO NOT USE IT
    pub fn sys_block_in_kernel(&self) -> SysResult {
        // DEAD LOOP
        error!("loop in kernel");
        let mut old = TimeSpec::now().sec;
        loop {
            let sec = TimeSpec::now().sec;
            if sec == old {
                core::hint::spin_loop();
                continue;
            }
            old = sec;
            warn!("1 seconds past");
        }
    }

    /// get resource usage
    /// currently only support ru_utime and ru_stime:
    /// - `ru_utime`: user CPU time used
    /// - `ru_stime`: system CPU time used
    pub fn sys_getrusage(&mut self, who: usize, mut rusage: UserOutPtr<RUsage>) -> SysResult {
        info!("getrusage: who: {}, rusage: {:?}", who, rusage);
        if rusage.is_null() {
            return Err(LxError::EINVAL);
        }
        let new_rusage = RUsage {
            utime: TimeVal::now(),
            stime: TimeVal::now(),
        };
        rusage.write(new_rusage)?;
        Ok(0)
    }

    /// Set an interval timer that delivers signals on expiration.
    ///
    /// Only `ITIMER_REAL` (which=0) is supported. It counts wall-clock
    /// time and delivers `SIGALRM` when the timer expires.
    pub fn sys_setitimer(
        &self,
        which: usize,
        new_value: UserInPtr<ITimerVal>,
        mut old_value: UserOutPtr<ITimerVal>,
    ) -> SysResult {
        info!(
            "setitimer: which={}, new={:?}, old={:?}",
            which, new_value, old_value
        );
        if which != 0 {
            // ITIMER_VIRTUAL (1) and ITIMER_PROF (2) need per-process
            // CPU time accounting which is not implemented.
            warn!(
                "setitimer: which={} not supported, only ITIMER_REAL (0)",
                which
            );
            return Err(LxError::EINVAL);
        }
        let new = if new_value.is_null() {
            ITimerVal::default()
        } else {
            new_value.read()?
        };
        let proc = self.zircon_process();
        let old = self.linux_process().set_itimer_real(new, proc);
        old_value.write_if_not_null(old)?;
        Ok(0)
    }

    /// Get the current value of an interval timer.
    pub fn sys_getitimer(&self, which: usize, mut curr_value: UserOutPtr<ITimerVal>) -> SysResult {
        info!("getitimer: which={}, curr={:?}", which, curr_value);
        if which != 0 {
            warn!("getitimer: which={} not supported", which);
            return Err(LxError::EINVAL);
        }
        let val = self.linux_process().get_itimer_real();
        curr_value.write(val)?;
        Ok(0)
    }

    /// Create a POSIX per-process timer.
    pub fn sys_timer_create(
        &self,
        clock_id: usize,
        sevp: UserInPtr<SigEvent>,
        mut timerid: UserOutPtr<usize>,
    ) -> SysResult {
        info!(
            "timer_create: clock_id={}, sevp={:?}, timerid={:?}",
            clock_id, sevp, timerid
        );
        if timerid.is_null() {
            return Err(LxError::EINVAL);
        }
        // Only CLOCK_REALTIME (0) and CLOCK_MONOTONIC (1) supported
        if clock_id > 1 {
            return Err(LxError::EINVAL);
        }
        let sev = if sevp.is_null() {
            SigEvent::default() // SIGEV_SIGNAL + SIGALRM
        } else {
            sevp.read()?
        };
        // Validate sigev_notify
        if sev.sigev_notify != SIGEV_SIGNAL && sev.sigev_notify != SIGEV_NONE {
            warn!(
                "timer_create: unsupported sigev_notify={}",
                sev.sigev_notify
            );
            return Err(LxError::EINVAL);
        }
        // Validate signal number for SIGEV_SIGNAL
        let signal = if sev.sigev_notify == SIGEV_SIGNAL {
            LinuxSignal::try_from(sev.sigev_signo as u8).map_err(|_| LxError::EINVAL)?
        } else {
            LinuxSignal::SIGALRM // unused for SIGEV_NONE, but need a value
        };
        let id = self
            .linux_process()
            .create_posix_timer(signal, sev.sigev_notify);
        timerid.write(id)?;
        Ok(0)
    }

    /// Arm or disarm a POSIX per-process timer.
    pub fn sys_timer_settime(
        &self,
        timer_id: usize,
        flags: usize,
        new_value: UserInPtr<ITimerSpec>,
        mut old_value: UserOutPtr<ITimerSpec>,
    ) -> SysResult {
        info!(
            "timer_settime: id={}, flags={}, new={:?}, old={:?}",
            timer_id, flags, new_value, old_value
        );
        let new = new_value.read()?;
        let proc = self.zircon_process();
        let old = self
            .linux_process()
            .set_posix_timer(timer_id, flags, new, proc)?;
        old_value.write_if_not_null(old)?;
        Ok(0)
    }

    /// Get the current value of a POSIX per-process timer.
    pub fn sys_timer_gettime(
        &self,
        timer_id: usize,
        mut curr_value: UserOutPtr<ITimerSpec>,
    ) -> SysResult {
        info!("timer_gettime: id={}, curr={:?}", timer_id, curr_value);
        let val = self.linux_process().get_posix_timer(timer_id)?;
        curr_value.write(val)?;
        Ok(0)
    }

    /// Delete a POSIX per-process timer.
    pub fn sys_timer_delete(&self, timer_id: usize) -> SysResult {
        info!("timer_delete: id={}", timer_id);
        self.linux_process().delete_posix_timer(timer_id)?;
        Ok(0)
    }

    /// Get the overrun count of a POSIX timer.
    pub fn sys_timer_getoverrun(&self, timer_id: usize) -> SysResult {
        info!("timer_getoverrun: id={}", timer_id);
        // Overrun tracking is not implemented; verify timer exists then return 0.
        self.linux_process().get_posix_timer(timer_id)?;
        Ok(0)
    }

    /// stores the current process times in the struct tms that buf points to
    pub fn sys_times(&mut self, mut buf: UserOutPtr<Tms>) -> SysResult {
        info!("times: buf: {:?}", buf);

        let tv = TimeVal::now();

        let tick = (tv.sec * 1_000_000 + tv.usec) / USEC_PER_TICK;

        if !buf.is_null() {
            // Per-process CPU time accounting is not implemented.
            // Return zeros rather than wall-clock time, which would
            // be incorrect (includes sleep time and other processes).
            let new_buf = Tms {
                tms_utime: 0,
                tms_stime: 0,
                tms_cutime: 0,
                tms_cstime: 0,
            };
            buf.write(new_buf)?;
        } else {
            warn!("sys_times: Invalid buf {:x?}", buf);
        }

        info!("tick: {:?}", tick);
        Ok(tick as usize)
    }

    /// clock nanosleep
    pub async fn sys_clock_nanosleep(
        &self,
        clockid: usize,
        flags: usize,
        req: UserInPtr<TimeSpec>,
        rem: UserOutPtr<TimeSpec>,
    ) -> SysResult {
        info!(
            "clock_nanosleep: clockid={:?}, flags={:?}, req={:?}, rem={:?}",
            clockid,
            flags,
            req.read()?,
            rem
        );
        use core::time::Duration;
        use kernel_hal::{thread, timer};
        let duration: Duration = req.read()?.into();
        let clockid = ClockId::from(clockid);
        let flags = ClockFlags::from(flags);
        info!("clockid={:?}, flags={:?}", clockid, flags,);
        match clockid {
            ClockId::ClockRealTime | ClockId::ClockMonotonic => match flags {
                ClockFlags::ZeroFlag => {
                    thread::sleep_until(timer::deadline_after(duration)).await;
                }
                ClockFlags::TimerAbsTime => {
                    // Convert absolute deadline to relative duration, then
                    // re-add to the timer domain via deadline_after.
                    //
                    // Note: timer_now() supplies the same time source for all
                    // clock IDs (boot-relative on bare metal, Unix epoch in
                    // libos). Proper CLOCK_REALTIME vs CLOCK_MONOTONIC
                    // separation would require clock-specific time sources in
                    // the HAL. This is a pre-existing limitation shared with
                    // sys_clock_gettime, which also uses timer_now() for all
                    // clocks via TimeSpec::now().
                    let now = timer::timer_now();
                    let remaining = duration.saturating_sub(now);
                    if !remaining.is_zero() {
                        thread::sleep_until(timer::deadline_after(remaining)).await;
                    }
                }
            },
            ClockId::ClockProcessCpuTimeId => {}
            ClockId::ClockThreadCpuTimeId => {}
            ClockId::ClockMonotonicRaw => {}
            ClockId::ClockRealTimeCoarse => {}
            ClockId::ClockMonotonicCoarse => {}
            ClockId::ClockBootTime => {}
            ClockId::ClockRealTimeAlarm => {}
            ClockId::ClockBootTimeAlarm => {}
        }
        // Check for pending signals after wakeup.
        // Note: on EINTR, Linux writes remaining time to `rem` for
        // relative sleeps. This is not yet implemented; `rem` is ignored.
        if self.thread.lock_linux().has_pending_signal() {
            return Err(LxError::EINTR);
        }
        Ok(0)
    }
}
