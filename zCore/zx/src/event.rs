//! Zircon Event and EventPair wrappers.

use crate::handle::RawHandle;
use crate::{impl_handle_based, Handle, Status};
use zircon_abi::syscall;

/// A Zircon event object.
#[derive(Debug)]
pub struct Event(Handle);
impl_handle_based!(Event);

impl Event {
    /// Create a new event.
    pub fn create() -> Result<Event, Status> {
        let mut handle: RawHandle = 0;
        Status::ok(unsafe { syscall::zx_event_create(0, &mut handle) })?;
        unsafe { Ok(Event(Handle::from_raw(handle))) }
    }
}

/// A Zircon event pair (two linked events).
#[derive(Debug)]
pub struct EventPair(Handle);
impl_handle_based!(EventPair);

impl EventPair {
    /// Create a new event pair.
    pub fn create() -> Result<(EventPair, EventPair), Status> {
        let mut h0: RawHandle = 0;
        let mut h1: RawHandle = 0;
        Status::ok(unsafe { syscall::zx_eventpair_create(0, &mut h0, &mut h1) })?;
        unsafe {
            Ok((
                EventPair(Handle::from_raw(h0)),
                EventPair(Handle::from_raw(h1)),
            ))
        }
    }
}
