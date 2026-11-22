use {super::*, crate::object::*, alloc::sync::Arc};

/// The task a SuspendToken can suspend — either a thread or a process.
/// Replaces the former `Weak<dyn Task>` (the only dyn Task usage),
/// enabling the Task trait to use `self: &Arc<Self>` (#871).
enum SuspendTarget {
    Thread(Arc<super::Thread>),
    Process(Arc<super::Process>),
}

impl SuspendTarget {
    fn resume(&self) {
        match self {
            SuspendTarget::Thread(t) => t.resume(),
            SuspendTarget::Process(p) => p.resume(),
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
            task: Some(SuspendTarget::Thread(thread.clone())),
        })
    }

    /// Create a `SuspendToken` which can suspend the given process.
    pub fn create_for_process(process: &Arc<super::Process>) -> Arc<Self> {
        process.suspend();
        Arc::new(SuspendToken {
            base: KObjectBase::new(),
            task: Some(SuspendTarget::Process(process.clone())),
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
