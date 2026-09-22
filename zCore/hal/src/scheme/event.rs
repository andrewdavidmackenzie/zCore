//! Event scheme trait and handler type.

use alloc::boxed::Box;

/// A type alias for the closure that handles a device event.
pub type EventHandler<T = ()> = Box<dyn Fn(&T) + Send + Sync>;

/// Trait for devices that produce events.
pub trait EventScheme {
    /// The type of event produced by this device.
    type Event;

    /// Trigger the event manually and call its handler immediately.
    fn trigger(&self, event: Self::Event);

    /// Subscribe to events, calling `handler` when an event occurs.
    /// If `once` is true, unsubscribe automatically after handling.
    fn subscribe(&self, handler: EventHandler<Self::Event>, once: bool);
}
