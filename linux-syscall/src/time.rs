//! Syscalls for time
//! - clock_gettime
//!
use crate::Syscall;
use kernel_hal::{user::UserInPtr, user::UserOutPtr};
use linux_object::error::LxError;
use linux_object::error::SysResult;
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
        // TODO: handle clock_settime
        let ts = TimeSpec::now();
        buf.write(ts)?;

        info!("TimeSpec: {:?}", ts);

        Ok(0)
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

    /// stores the current process times in the struct tms that buf points to
    pub fn sys_times(&mut self, mut buf: UserOutPtr<Tms>) -> SysResult {
        info!("times: buf: {:?}", buf);

        let tv = TimeVal::now();

        let tick = (tv.sec * 1_000_000 + tv.usec) / USEC_PER_TICK;

        if !buf.is_null() {
            // Approximate: attribute all elapsed time as user time
            let new_buf = Tms {
                tms_utime: tick as u64,
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
                    // duration is an absolute time point; sleep_until expects
                    // an absolute deadline, so pass it directly.
                    thread::sleep_until(duration).await;
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
        Ok(0)
    }
}
