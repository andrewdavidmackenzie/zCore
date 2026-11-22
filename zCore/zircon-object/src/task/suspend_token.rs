use {
    super::*,
    crate::object::*,
    alloc::sync::{Arc, Weak},
};

/// The task a SuspendToken can suspend — either a thread or a process.
/// Replaces the former `Weak<dyn Task>` (the only dyn Task usage),
/// enabling the Task trait to use `self: &Arc<Self>` (#871).
///
/// Holds weak references to avoid reference cycles: a strong Arc
/// would create `Process → handle → token → Thread → Process`
/// (or `Process → token → Process`), preventing destruction.
enum SuspendTarget {
    Thread(Weak<super::Thread>),
    Process(Weak<super::Process>),
}

impl SuspendTarget {
    fn resume(&self) {
        match self {
            // If the upgrade fails, the target was destroyed while
            // suspended — nothing to resume.
            SuspendTarget::Thread(t) => {
                if let Some(t) = t.upgrade() {
                    t.resume();
                }
            }
            SuspendTarget::Process(p) => {
                if let Some(p) = p.upgrade() {
                    p.resume();
                }
            }
        }
    }
}

/// Suspend the given task.
///
/// Currently only thread or process handles may be suspended.
pub struct SuspendToken {
    base: KObjectBase,
    task: Option<SuspendTarget>,
}

impl_kobject!(SuspendToken);

impl SuspendToken {
    /// Create a `SuspendToken` which can suspend the given thread.
    pub fn create_for_thread(thread: &Arc<super::Thread>) -> Arc<Self> {
        thread.suspend();
        Arc::new(SuspendToken {
            base: KObjectBase::new(),
            task: Some(SuspendTarget::Thread(Arc::downgrade(thread))),
        })
    }

    /// Create a `SuspendToken` which can suspend the given process.
    pub fn create_for_process(process: &Arc<super::Process>) -> Arc<Self> {
        process.suspend();
        Arc::new(SuspendToken {
            base: KObjectBase::new(),
            task: Some(SuspendTarget::Process(Arc::downgrade(process))),
        })
    }
}

impl Drop for SuspendToken {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.resume();
        }
    }
}
