use {
    super::*,
    alloc::sync::Arc,
    core::{
        fmt::{Debug, Formatter, Result},
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    },
    kernel_hal::timer::timer_now,
    zircon_object::{dev::*, signal::Clock, task::*},
};

static UTC_OFFSET: AtomicU64 = AtomicU64::new(0);

const ZX_CLOCK_MONOTONIC: u32 = 0;
const ZX_CLOCK_UTC: u32 = 1;
const ZX_CLOCK_THREAD: u32 = 2;

impl Syscall<'_> {
    /// Create a new clock object.
    pub fn sys_clock_create(
        &self,
        options: u64,
        _user_args: UserInPtr<u8>,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!("clock.create: options={:#x}", options);
        let clock = Clock::new(options)?;
        let proc = self.thread.proc();
        let handle = proc.add_handle(Handle::new(
            Arc::new(clock),
            Rights::READ | Rights::WRITE | Rights::DUPLICATE | Rights::TRANSFER | Rights::INSPECT,
        ));
        out.write(handle)?;
        Ok(())
    }

    /// Read the monotonic clock (vDSO fallback path).
    ///
    /// This is the kernel-side implementation of the vDSO function
    /// `zx_clock_get_monotonic`. In a full Fuchsia system this is
    /// served by the vDSO directly; this path is used when the vDSO
    /// is not available.
    pub fn sys_clock_get_monotonic_via_kernel(&self, mut out: UserOutPtr<i64>) -> ZxResult {
        info!("clock.get_monotonic_via_kernel");
        out.write(timer_now().as_nanos() as i64)?;
        Ok(())
    }

    /// Read the hardware tick counter (vDSO fallback path).
    pub fn sys_ticks_get_via_kernel(&self, _out: UserOutPtr<i64>) -> ZxResult {
        // TODO: add a HAL raw-counter accessor and return raw ticks
        // with a matching ticks_per_second rate. timer_now() returns
        // nanoseconds which would give incorrect results when divided
        // by zx_ticks_per_second().
        info!("ticks.get_via_kernel: not yet implemented");
        Err(ZxError::NOT_SUPPORTED)
    }

    /// Acquire the current time.
    ///
    /// + Returns the current time of clock_id via `time`.
    /// + Returns whether `clock_id` was valid.
    pub fn sys_clock_get(&self, clock_id: u32, mut time: UserOutPtr<u64>) -> ZxResult {
        info!("clock.get: id={}", clock_id);
        match clock_id {
            ZX_CLOCK_MONOTONIC => {
                time.write(timer_now().as_nanos() as u64)?;
                Ok(())
            }
            ZX_CLOCK_UTC => {
                time.write(timer_now().as_nanos() as u64 + UTC_OFFSET.load(Ordering::Relaxed))?;
                Ok(())
            }
            ZX_CLOCK_THREAD => {
                time.write(self.thread.get_time())?;
                Ok(())
            }
            _ => Err(ZxError::NOT_SUPPORTED),
        }
    }

    /// Perform a basic read of the clock.
    ///
    /// Currently returns monotonic time regardless of the clock handle.
    /// A proper implementation would look up the clock object and read
    /// its transformed timeline.
    pub fn sys_clock_read(&self, handle: HandleValue, mut now: UserOutPtr<u64>) -> ZxResult {
        info!("clock.read: handle={:#x?}", handle);
        let proc = self.thread.proc();
        let clock = proc.get_object_with_rights::<Clock>(handle, Rights::READ)?;
        let value = clock.read()?;
        now.write(value as u64)?;
        Ok(())
    }

    /// Adjust the clock.
    pub fn sys_clock_adjust(&self, resource: HandleValue, clock_id: u32, offset: u64) -> ZxResult {
        info!(
            "clock.adjust: resource={:#x?}, id={:#x}, offset={:#x}",
            resource, clock_id, offset
        );
        let proc = self.thread.proc();
        proc.get_object::<Resource>(resource)?
            .validate(ResourceKind::ROOT)?;
        match clock_id {
            ZX_CLOCK_MONOTONIC => Err(ZxError::ACCESS_DENIED),
            ZX_CLOCK_UTC => {
                UTC_OFFSET.store(offset, Ordering::Relaxed);
                Ok(())
            }
            _ => Err(ZxError::INVALID_ARGS),
        }
    }

    /// Get detailed information about a clock object.
    pub fn sys_clock_get_details(
        &self,
        handle: HandleValue,
        _options: u64,
        mut details: UserOutPtr<u8>,
    ) -> ZxResult {
        info!("clock.get_details: handle={:#x}", handle);
        let proc = self.thread.proc();
        let clock = proc.get_object_with_rights::<Clock>(handle, Rights::READ)?;
        let data = clock.get_details()?;
        details.write_array(&data)?;
        Ok(())
    }

    /// Make adjustments to a clock object.
    pub fn sys_clock_update(
        &self,
        handle: HandleValue,
        options: u64,
        user_args: UserInPtr<u8>,
    ) -> ZxResult {
        info!("clock.update: handle={:#x}, options={:#x}", handle, options);
        let proc = self.thread.proc();
        let clock = proc.get_object_with_rights::<Clock>(handle, Rights::WRITE)?;
        let args = user_args.read_array(32)?;
        clock.update(options, &args)
    }

    /// Sleep for some number of nanoseconds.
    ///
    /// A `deadline` value less than or equal to 0 immediately yields the thread.
    pub async fn sys_nanosleep(&self, deadline: Deadline) -> ZxResult {
        info!("nanosleep: deadline={:?}", deadline);
        if deadline.0 <= 0 {
            kernel_hal::thread::yield_now().await;
        } else {
            let future = kernel_hal::thread::sleep_until(deadline.into());
            pin_mut!(future);
            self.thread
                .blocking_run(
                    future,
                    ThreadState::BlockedSleeping,
                    Deadline::forever().into(),
                    None,
                )
                .await?;
        }
        Ok(())
    }
}

#[repr(transparent)]
pub struct Deadline(i64);

impl From<usize> for Deadline {
    fn from(x: usize) -> Self {
        Deadline(x as i64)
    }
}

impl Deadline {
    pub fn is_positive(&self) -> bool {
        self.0.is_positive()
    }

    pub fn forever() -> Self {
        Deadline(i64::MAX)
    }
}

impl From<Deadline> for Duration {
    fn from(deadline: Deadline) -> Self {
        Duration::from_nanos(deadline.0.max(0) as u64)
    }
}

impl Debug for Deadline {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.0 <= 0 {
            write!(f, "NoWait")
        } else if self.0 == i64::MAX {
            write!(f, "Forever")
        } else {
            write!(f, "At({:?})", Duration::from_nanos(self.0 as u64))
        }
    }
}
