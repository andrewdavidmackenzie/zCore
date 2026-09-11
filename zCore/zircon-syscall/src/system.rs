use super::*;
use zircon_object::{signal::Event, task::Job};

impl Syscall<'_> {
    /// Retrieve a handle to a system event.
    ///
    /// `root_job: HandleValue`, must be a handle to the root job of the system.
    /// `kind: u32`, must be one of the following:
    /// ```rust
    ///     const EVENT_OUT_OF_MEMORY: u32 = 1;
    ///     const EVENT_MEMORY_PRESSURE_CRITICAL: u32 = 2;
    ///     const EVENT_MEMORY_PRESSURE_WARNING: u32 = 3;
    ///     const EVENT_MEMORY_PRESSURE_NORMAL: u32 = 4;
    /// ```
    pub fn sys_system_get_event(
        &self,
        root_job: HandleValue,
        kind: u32,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "system.get_event: root_job={:#x}, kind={:#x}, out_ptr={:#x?}",
            root_job, kind, out
        );
        match kind {
            EVENT_OUT_OF_MEMORY => {
                let proc = self.thread.proc();
                proc.get_object_with_rights::<Job>(root_job, Rights::MANAGE_PROCESS)?
                    .check_root_job()?;
                // TODO: out-of-memory event
                let event = Event::new();
                let event_handle = proc.add_handle(Handle::new(event, Rights::DEFAULT_EVENT));
                out.write(event_handle)?;
                Ok(())
            }
            EVENT_MEMORY_PRESSURE_CRITICAL
            | EVENT_MEMORY_PRESSURE_WARNING
            | EVENT_MEMORY_PRESSURE_NORMAL => {
                let proc = self.thread.proc();
                proc.get_object_with_rights::<Job>(root_job, Rights::MANAGE_PROCESS)?
                    .check_root_job()?;
                // TODO: implement real memory pressure event monitoring.
                // Returning a stub Event would cause callers to block
                // indefinitely waiting for a signal that never fires.
                warn!(
                    "system.get_event: memory pressure event kind={} not yet implemented",
                    kind
                );
                Err(ZxError::NOT_SUPPORTED)
            }
            _ => {
                warn!("system.get_event: unknown event kind {:#x}", kind);
                Err(ZxError::INVALID_ARGS)
            }
        }
    }
}

const EVENT_OUT_OF_MEMORY: u32 = 1;
const EVENT_MEMORY_PRESSURE_CRITICAL: u32 = 2;
const EVENT_MEMORY_PRESSURE_WARNING: u32 = 3;
const EVENT_MEMORY_PRESSURE_NORMAL: u32 = 4;
