use {
    super::*,
    zircon_object::task::{Thread, ThreadState},
};

impl Syscall<'_> {
    /// Wait on a futex.
    pub async fn sys_futex_wait(
        &self,
        value_ptr: UserInPtr<AtomicI32>,
        current_value: i32,
        new_futex_owner: HandleValue,
        deadline: Deadline,
    ) -> ZxResult {
        info!(
            "futex.wait: value_ptr={:#x?}, current_value={:#x}, new_futex_owner={:#x}, deadline={:?}",
            value_ptr, current_value, new_futex_owner, deadline
        );
        if value_ptr.is_null() || !value_ptr.as_addr().is_multiple_of(4) {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let futex = proc.get_futex(value_ptr.as_addr());
        let new_owner = if new_futex_owner == INVALID_HANDLE {
            None
        } else {
            Some(proc.get_object::<Thread>(new_futex_owner)?)
        };
        let future = futex.wait_with_owner(current_value, Some(self.thread.inner()), new_owner);
        self.thread
            .blocking_run::<_, (), _>(future, ThreadState::BlockedFutex, deadline.into(), None)
            .await?;
        Ok(())
    }

    /// Wake some waiters and requeue other waiters.
    pub fn sys_futex_requeue(
        &self,
        value_ptr: UserInPtr<AtomicI32>,
        wake_count: u32,
        current_value: i32,
        requeue_ptr: UserInPtr<AtomicI32>,
        requeue_count: u32,
        new_requeue_owner: HandleValue,
    ) -> ZxResult {
        info!(
            "futex.requeue: value_ptr={:?}, wake_count={:#x}, current_value={:#x}, requeue_ptr={:?}, requeue_count={:#x}, new_requeue_owner={:?}",
            value_ptr, wake_count, current_value, requeue_ptr, requeue_count, new_requeue_owner
        );
        if value_ptr.is_null() || !value_ptr.as_addr().is_multiple_of(4) {
            return Err(ZxError::INVALID_ARGS);
        }
        if value_ptr.as_addr() == requeue_ptr.as_addr() {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let new_requeue_owner = if new_requeue_owner == INVALID_HANDLE {
            None
        } else {
            Some(proc.get_object::<Thread>(new_requeue_owner)?)
        };
        let wake_futex = proc.get_futex(value_ptr.as_addr());
        let requeue_futex = proc.get_futex(requeue_ptr.as_addr());
        wake_futex.requeue(
            current_value,
            wake_count as usize,
            requeue_count as usize,
            &requeue_futex,
            new_requeue_owner,
            true,
        )?;
        Ok(())
    }

    /// Wake some number of threads waiting on a futex.
    pub fn sys_futex_wake(&self, value_ptr: UserInPtr<AtomicI32>, count: u32) -> ZxResult {
        info!("futex.wake: value_ptr={:?}, count={:#x}", value_ptr, count);
        if value_ptr.is_null() || !value_ptr.as_addr().is_multiple_of(4) {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let futex = proc.get_futex(value_ptr.as_addr());
        futex.wake(count as usize);
        Ok(())
    }

    /// Like `futex_requeue`, but assigns ownership of the requeue futex
    /// to the thread that was woken.
    pub fn sys_futex_requeue_single_owner(
        &self,
        value_ptr: UserInPtr<AtomicI32>,
        current_value: i32,
        requeue_ptr: UserInPtr<AtomicI32>,
        requeue_count: u32,
        new_requeue_owner: HandleValue,
    ) -> ZxResult {
        info!(
            "futex.requeue_single_owner: value_ptr={:?}, current_value={:#x}, requeue_ptr={:?}, requeue_count={:#x}, new_requeue_owner={:?}",
            value_ptr, current_value, requeue_ptr, requeue_count, new_requeue_owner
        );
        if value_ptr.is_null() || !value_ptr.as_addr().is_multiple_of(4) {
            return Err(ZxError::INVALID_ARGS);
        }
        if requeue_ptr.is_null() || !requeue_ptr.as_addr().is_multiple_of(4) {
            return Err(ZxError::INVALID_ARGS);
        }
        if value_ptr.as_addr() == requeue_ptr.as_addr() {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let new_requeue_owner = if new_requeue_owner == INVALID_HANDLE {
            None
        } else {
            Some(proc.get_object::<Thread>(new_requeue_owner)?)
        };
        let wake_futex = proc.get_futex(value_ptr.as_addr());
        let requeue_futex = proc.get_futex(requeue_ptr.as_addr());
        wake_futex.requeue(
            current_value,
            1,
            requeue_count as usize,
            &requeue_futex,
            new_requeue_owner,
            true,
        )?;
        Ok(())
    }

    /// Query the owner of a futex.
    ///
    /// Returns the koid of the thread that owns the futex, or
    /// `ZX_KOID_INVALID` (0) if the futex has no owner.
    pub fn sys_futex_get_owner(
        &self,
        value_ptr: UserInPtr<AtomicI32>,
        mut koid: UserOutPtr<u64>,
    ) -> ZxResult {
        info!("futex.get_owner: value_ptr={:?}", value_ptr);
        if value_ptr.is_null() || !value_ptr.as_addr().is_multiple_of(4) {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let futex = proc.get_futex(value_ptr.as_addr());
        let owner_koid = futex.owner().map_or(0u64, |t| t.id());
        koid.write(owner_koid)?;
        Ok(())
    }

    /// Wake one waiter and transfer ownership of the futex to it.
    pub fn sys_futex_wake_single_owner(&self, value_ptr: UserInPtr<AtomicI32>) -> ZxResult {
        info!("futex.wake_single_owner: value_ptr={:?}", value_ptr);
        if value_ptr.is_null() || !value_ptr.as_addr().is_multiple_of(4) {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        proc.get_futex(value_ptr.as_addr()).wake_single_owner();
        Ok(())
    }
}
