use {
    self::event_interrupt::*,
    self::virtual_interrupt::*,
    crate::object::*,
    crate::signal::*,
    alloc::{boxed::Box, sync::Arc, vec::Vec},
    bitflags::bitflags,
    core::{
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicU64, Ordering},
        task::{Context, Poll},
    },
    lock::Mutex,
};

/// Global registry of wake sources (interrupt objects with WAKE_VECTOR).
static WAKE_SOURCES: Mutex<Vec<WakeSourceEntry>> = Mutex::new(Vec::new());

/// A registered wake source entry.
pub struct WakeSourceEntry {
    /// KoID of the interrupt object.
    pub koid: KoID,
    /// Name of the interrupt object (for debugging, "VirtIRQ <koid>").
    pub name: [u8; 32],
    /// Boot time of the first trigger (0 if never triggered).
    pub initial_signal_time: AtomicU64,
    /// Boot time of the most recent trigger.
    pub last_signal_time: AtomicU64,
    /// Boot time of the most recent ack (u64::MAX if never acked).
    pub last_ack_time: AtomicU64,
    /// Number of times triggered.
    pub signal_count: AtomicU64,
    /// Whether the source is currently signaled (not yet acked).
    pub signaled: core::sync::atomic::AtomicBool,
    /// Whether this source has been reported in a previous suspend report.
    pub reported: core::sync::atomic::AtomicBool,
}

impl WakeSourceEntry {
    fn new(koid: KoID) -> Self {
        let mut name = [0u8; 32];
        let s = alloc::format!("VirtIRQ {koid}");
        let len = s.len().min(31);
        name[..len].copy_from_slice(&s.as_bytes()[..len]);
        WakeSourceEntry {
            koid,
            name,
            initial_signal_time: AtomicU64::new(0),
            last_signal_time: AtomicU64::new(0),
            last_ack_time: AtomicU64::new(u64::MAX),
            signal_count: AtomicU64::new(0),
            signaled: core::sync::atomic::AtomicBool::new(false),
            reported: core::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// Register an interrupt as a wake source.
pub fn register_wake_source(koid: KoID) {
    let mut sources = WAKE_SOURCES.lock();
    if !sources.iter().any(|s| s.koid == koid) {
        sources.push(WakeSourceEntry::new(koid));
    }
}

/// Unregister a wake source.
pub fn unregister_wake_source(koid: KoID) {
    let mut sources = WAKE_SOURCES.lock();
    sources.retain(|s| s.koid != koid);
}

/// Record a trigger event on a wake source.
pub fn wake_source_triggered(koid: KoID, time: u64) {
    let sources = WAKE_SOURCES.lock();
    if let Some(entry) = sources.iter().find(|s| s.koid == koid) {
        entry
            .initial_signal_time
            .compare_exchange(0, time, Ordering::Relaxed, Ordering::Relaxed)
            .ok();
        entry.last_signal_time.store(time, Ordering::Relaxed);
        entry.signal_count.fetch_add(1, Ordering::Relaxed);
        entry.signaled.store(true, Ordering::Relaxed);
    }
}

/// Record an ack event on a wake source.
pub fn wake_source_acked(koid: KoID, time: u64) {
    let sources = WAKE_SOURCES.lock();
    if let Some(entry) = sources.iter().find(|s| s.koid == koid) {
        entry.last_ack_time.store(time, Ordering::Relaxed);
        entry.signaled.store(false, Ordering::Relaxed);
    }
}

/// Get the number of active (registered) wake sources.
pub fn wake_source_count() -> u32 {
    WAKE_SOURCES.lock().len() as u32
}

/// Wake report entry: (koid, name, initial_signal_time, last_signal_time, last_ack_time, signal_count, flags).
pub type WakeReportEntry = (KoID, [u8; 32], u64, u64, u64, u32, u32);

/// Collect wake source entries for the suspend report.
///
/// Returns entries that have been triggered since the last report.
/// After collection, acked entries are cleared (they won't appear
/// in the next report). Still-signaled entries are marked as
/// "previously reported" and will appear again with that flag set.
pub fn collect_wake_entries() -> Vec<WakeReportEntry> {
    let sources = WAKE_SOURCES.lock();
    let mut result = Vec::new();
    for s in sources.iter() {
        let count = s.signal_count.load(Ordering::Relaxed);
        if count == 0 {
            continue;
        }
        let still_signaled = s.signaled.load(Ordering::Relaxed);
        let previously_reported = s.reported.load(Ordering::Relaxed);
        let mut flags = 0u32;
        if still_signaled {
            flags |= 1; // ZX_SYSTEM_WAKE_REPORT_ENTRY_FLAG_SIGNALED
        }
        if previously_reported {
            flags |= 2; // ZX_SYSTEM_WAKE_REPORT_ENTRY_FLAG_PREVIOUSLY_REPORTED
        }
        result.push((
            s.koid,
            s.name,
            s.initial_signal_time.load(Ordering::Relaxed),
            s.last_signal_time.load(Ordering::Relaxed),
            s.last_ack_time.load(Ordering::Relaxed),
            count as u32,
            flags,
        ));
        // Mark as reported. If acked, clear for next time.
        s.reported.store(true, Ordering::Relaxed);
        if !still_signaled {
            // Acked — clear for next report.
            s.initial_signal_time.store(0, Ordering::Relaxed);
            s.last_signal_time.store(0, Ordering::Relaxed);
            s.last_ack_time.store(u64::MAX, Ordering::Relaxed);
            s.signal_count.store(0, Ordering::Relaxed);
            s.reported.store(false, Ordering::Relaxed);
        }
    }
    result
}

/// Clear all wake source signal/ack state (for DISCARD option).
pub fn discard_wake_entries() {
    let sources = WAKE_SOURCES.lock();
    for s in sources.iter() {
        s.initial_signal_time.store(0, Ordering::Relaxed);
        s.last_signal_time.store(0, Ordering::Relaxed);
        s.last_ack_time.store(u64::MAX, Ordering::Relaxed);
        s.signal_count.store(0, Ordering::Relaxed);
        s.signaled.store(false, Ordering::Relaxed);
        s.reported.store(false, Ordering::Relaxed);
    }
}

/// A future that waits for `SIGNALED` on a `KObjectBase`.
///
/// Used internally by `Interrupt::wait()` to block on the internal
/// `wait_event` without using `dyn KernelObject::wait_signal`.
struct WaitEventFuture<'a> {
    event: &'a KObjectBase,
    registered: bool,
}

impl<'a> WaitEventFuture<'a> {
    fn new(event: &'a KObjectBase) -> Self {
        Self {
            event,
            registered: false,
        }
    }
}

impl Future for WaitEventFuture<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let current = self.event.signal();
        if current.contains(Signal::SIGNALED) {
            return Poll::Ready(());
        }
        if !self.registered {
            let waker = cx.waker().clone();
            self.event.add_signal_callback(Box::new(move |s| {
                if !s.contains(Signal::SIGNALED) {
                    return false;
                }
                waker.wake_by_ref();
                true
            }));
            self.registered = true;
        }
        Poll::Pending
    }
}

mod event_interrupt;
mod virtual_interrupt;

trait InterruptTrait: Sync + Send {
    /// Mask the interrupt.
    fn mask(&self);
    /// Unmask the interrupt.
    fn unmask(&self);
    /// Register the interrupt to the given handler.
    fn register_handler(&self, handler: Box<dyn Fn() + Send + Sync>) -> ZxResult;
    /// Unregister the interrupt to the given handler.
    fn unregister_handler(&self) -> ZxResult;
}

impl_kobject!(Interrupt);

/// Interrupts - Usermode I/O interrupt delivery.
///
/// ## SYNOPSIS
///
/// Interrupt objects allow userspace to create, signal, and wait on hardware interrupts.
pub struct Interrupt {
    base: KObjectBase,
    has_vcpu: bool,
    flags: InterruptFlags,
    inner: Mutex<InterruptInner>,
    trait_: Box<dyn InterruptTrait>,
    /// Internal event used to wake `wait()` without affecting observable
    /// signal state. Uses `SIGNALED` (bit 3) as the wakeup signal.
    wait_event: KObjectBase,
}

#[derive(Default)]
struct InterruptInner {
    state: InterruptState,
    port: Option<Arc<Port>>,
    key: u64,
    /// Pending timestamp for re-trigger coalescing. When a second trigger
    /// arrives while in NeedAck state, this records the timestamp to be
    /// delivered on the next ack().
    timestamp: i64,
    defer_unmask: bool,
    packet_id: u64,
    /// The original trigger timestamp, preserved across unbind/rebind so
    /// it can be re-queued to a new port.
    trigger_timestamp: i64,
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        // Unregister from the global wake source registry.
        unregister_wake_source(self.base.id);
        // Ignore errors during cleanup — the interrupt may already
        // be destroyed or in an inconsistent state.
        let _ = self.destroy();
    }
}

impl Interrupt {
    /// Create a new virtual interrupt.
    ///
    /// Virtual interrupts start in the UNTRIGGERED state with
    /// `INTERRUPT_SIGNAL` asserted (= `ZX_VIRTUAL_INTERRUPT_UNTRIGGERED`).
    pub fn new_virtual() -> Arc<Self> {
        let base = KObjectBase::new();
        // Assert UNTRIGGERED signal immediately after creation.
        base.signal_set(Signal::INTERRUPT_SIGNAL);
        Arc::new(Interrupt {
            base,
            has_vcpu: false,
            flags: InterruptFlags::VIRTUAL,
            inner: Default::default(),
            trait_: VirtualInterrupt::new(),
            wait_event: KObjectBase::new(),
        })
    }

    /// Create a new physical interrupt.
    pub fn new_physical(vector: usize, options: InterruptOptions) -> ZxResult<Arc<Self>> {
        let mode = options.to_mode();
        if mode != InterruptOptions::MODE_DEFAULT && mode != InterruptOptions::MODE_EDGE_HIGH {
            // TODO: support MODE_EDGE_LOW, MODE_LEVEL_LOW, MODE_LEVEL_HIGH interrupt modes
            unimplemented!();
        }
        if options.contains(InterruptOptions::REMAP_IRQ) {
            warn!("Skip Interrupt.Remap");
        }
        let interrupt = Arc::new(Interrupt {
            base: KObjectBase::new(),
            has_vcpu: false,
            flags: InterruptFlags::empty(),
            inner: Default::default(),
            trait_: EventInterrupt::new(vector),
            wait_event: KObjectBase::new(),
        });
        let interrupt_clone = interrupt.clone();
        interrupt
            .trait_
            .register_handler(Box::new(move || interrupt_clone.handle_interrupt()))?;
        interrupt.trait_.unmask();
        Ok(interrupt)
    }

    /// Bind the interrupt object to a port.
    pub fn bind(&self, port: &Arc<Port>, key: u64) -> ZxResult {
        let mut inner = self.inner.lock();
        match inner.state {
            InterruptState::Destroy => return Err(ZxError::CANCELED),
            InterruptState::Waiting => return Err(ZxError::BAD_STATE),
            _ => (),
        }
        if inner.port.is_some() || self.has_vcpu {
            return Err(ZxError::ALREADY_BOUND);
        }
        if self
            .flags
            .contains(InterruptFlags::UNMASK_PREWAIT_UNLOCKED | InterruptFlags::MASK_POSTWAIT)
        {
            return Err(ZxError::INVALID_ARGS);
        }
        inner.port = Some(port.clone());
        inner.key = key;
        if inner.state == InterruptState::Triggered {
            // Use trigger_timestamp which survives unbind (inner.timestamp
            // may have been cleared when the original packet was sent).
            let ts = if inner.trigger_timestamp != 0 {
                inner.trigger_timestamp
            } else {
                inner.timestamp
            };
            inner.packet_id = port.as_ref().push_interrupt(ts, inner.key);
            inner.timestamp = 0;
            inner.state = InterruptState::NeedAck;
            // Clear UNTRIGGERED signal.
            if self.flags.contains(InterruptFlags::VIRTUAL) {
                self.base.signal_clear(Signal::INTERRUPT_SIGNAL);
            }
        }
        Ok(())
    }

    /// Unbind the interrupt object from a port.
    ///
    /// Unbinding the port removes previously queued packets to the port.
    /// The triggered/NeedAck state is preserved so that a subsequent rebind
    /// will re-queue the packet to the new port.
    pub fn unbind(&self, port: &Arc<Port>) -> ZxResult {
        let mut inner = self.inner.lock();
        if inner.port.is_none() || inner.port.as_ref().unwrap().id() != port.id() {
            return Err(ZxError::NOT_FOUND);
        }
        if inner.state == InterruptState::Destroy {
            return Err(ZxError::CANCELED);
        }
        let was_queued = port.remove_interrupt(inner.packet_id);
        inner.port = None;
        inner.key = 0;
        // If the packet was still in the queue (not yet consumed by the user),
        // map NeedAck → Triggered so bind() will re-queue it to the new port.
        // If the packet was already consumed, keep NeedAck — the user still
        // needs to ack() before the interrupt can re-trigger.
        if inner.state == InterruptState::NeedAck && was_queued {
            inner.state = InterruptState::Triggered;
        }
        Ok(())
    }

    /// Triggers a virtual interrupt object.
    ///
    /// If the interrupt is already in NeedAck state with a port bound, the
    /// timestamp is recorded as pending but no second packet is queued.
    /// The pending timestamp will be delivered when `ack()` is called.
    pub fn trigger(&self, timestamp: i64) -> ZxResult {
        if !self.flags.contains(InterruptFlags::VIRTUAL) {
            return Err(ZxError::BAD_STATE);
        }
        let mut inner = self.inner.lock();
        if inner.state == InterruptState::Destroy {
            return Err(ZxError::CANCELED);
        }
        // Only record the timestamp if this is the first signal since we
        // started waiting (matches Fuchsia: `if (!timestamp_) timestamp_ = ts`)
        if inner.timestamp == 0 {
            inner.timestamp = timestamp;
        }
        if inner.state == InterruptState::NeedAck && inner.port.is_some() {
            // Coalesce: the pending timestamp is already recorded above.
            // No second packet is queued; it will be sent on ack().
            // Clear UNTRIGGERED since we are (still) triggered.
            if self.flags.contains(InterruptFlags::VIRTUAL) {
                self.base.signal_clear(Signal::INTERRUPT_SIGNAL);
            }
            return Ok(());
        }
        // Save the trigger timestamp for rebind scenarios.
        inner.trigger_timestamp = timestamp;
        if let Some(port) = &inner.port {
            inner.packet_id = port.push_interrupt(timestamp, inner.key);
            if self.flags.contains(InterruptFlags::MASK_POSTWAIT) {
                self.trait_.mask();
            }
            inner.timestamp = 0;
            inner.state = InterruptState::NeedAck;
        } else {
            // No port: wake the wait() via the internal event.
            if inner.state != InterruptState::NeedAck {
                inner.state = InterruptState::Triggered;
            }
            self.wait_event.signal_set(Signal::SIGNALED);
        }
        // For virtual interrupts, clear UNTRIGGERED — we are now triggered.
        if self.flags.contains(InterruptFlags::VIRTUAL) {
            self.base.signal_clear(Signal::INTERRUPT_SIGNAL);
        }
        Ok(())
    }

    /// Acknowledge the interrupt and re-arm it.
    ///
    /// Returns `BAD_STATE` if no port is bound, or if the interrupt packet
    /// has not yet been consumed from the port.
    pub fn ack(&self) -> ZxResult {
        let mut inner = self.inner.lock();
        if inner.port.is_none() {
            return Err(ZxError::BAD_STATE);
        }
        if inner.state == InterruptState::Destroy {
            return Err(ZxError::CANCELED);
        }
        if inner.state == InterruptState::NeedAck {
            // Check packet consumption before any side effects.
            // Fuchsia's SendPacketLocked returns false (→ BAD_STATE) when
            // the packet is still in the port queue (InContainer).
            if inner
                .port
                .as_ref()
                .unwrap()
                .is_interrupt_packet_queued(inner.packet_id)
            {
                return Err(ZxError::BAD_STATE);
            }

            // Packet was consumed — safe to proceed with side effects.
            if self.flags.contains(InterruptFlags::UNMASK_PREWAIT) {
                self.trait_.unmask();
            } else if self.flags.contains(InterruptFlags::UNMASK_PREWAIT_UNLOCKED) {
                inner.defer_unmask = true;
            }
            if inner.timestamp > 0 {
                // Queue the pending re-trigger packet.
                inner.packet_id = inner
                    .port
                    .as_ref()
                    .unwrap()
                    .push_interrupt(inner.timestamp, inner.key);
                if self.flags.contains(InterruptFlags::MASK_POSTWAIT) {
                    self.trait_.mask();
                }
                inner.timestamp = 0;
                // Assert UNTRIGGERED then immediately clear it (strobe).
                // This satisfies any async waiters watching for UNTRIGGERED.
                if self.flags.contains(InterruptFlags::VIRTUAL) {
                    self.base.signal_set(Signal::INTERRUPT_SIGNAL);
                    self.base.signal_clear(Signal::INTERRUPT_SIGNAL);
                }
                // State stays NeedAck (re-triggered).
            } else {
                inner.state = InterruptState::Idle;
                // Assert UNTRIGGERED signal — interrupt is now idle.
                if self.flags.contains(InterruptFlags::VIRTUAL) {
                    self.base.signal_set(Signal::INTERRUPT_SIGNAL);
                }
            }
        }
        if inner.defer_unmask {
            self.trait_.unmask();
        }
        Ok(())
    }

    /// Destroy the interrupt.
    ///
    /// Asserts UNTRIGGERED signal for virtual interrupts so that waiters
    /// are not stuck waiting on a destroyed interrupt.
    pub fn destroy(&self) -> ZxResult {
        self.trait_.mask();
        self.trait_.unregister_handler()?;
        let mut inner = self.inner.lock();
        let result = if let Some(port) = &inner.port {
            let in_queue = port.remove_interrupt(inner.packet_id);
            match inner.state {
                InterruptState::NeedAck => {
                    inner.state = InterruptState::Destroy;
                    if !in_queue {
                        Err(ZxError::NOT_FOUND)
                    } else {
                        Ok(())
                    }
                }
                InterruptState::Idle => {
                    inner.state = InterruptState::Destroy;
                    Ok(())
                }
                _ => Ok(()),
            }
        } else {
            inner.state = InterruptState::Destroy;
            // Wake any thread blocked in wait() via the internal event.
            self.wait_event.signal_set(Signal::SIGNALED);
            Ok(())
        };
        // For virtual interrupts, assert UNTRIGGERED (= INTERRUPT_SIGNAL).
        if self.flags.contains(InterruptFlags::VIRTUAL) {
            self.base.signal_set(Signal::INTERRUPT_SIGNAL);
        }
        result
    }

    /// Wait until the interrupt is triggered.
    ///
    /// Uses an internal `wait_event` object for blocking, separate from the
    /// observable `base` signals. This prevents the internal wake mechanism
    /// from interfering with `INTERRUPT_SIGNAL` (bit 4) which carries the
    /// UNTRIGGERED semantic for virtual interrupts.
    pub async fn wait(self: &Arc<Self>) -> ZxResult<i64> {
        let mut defer_unmask = false;
        // Use the wait_event for blocking, NOT the base object.

        loop {
            {
                let mut inner = self.inner.lock();
                if inner.port.is_some() || self.has_vcpu {
                    return Err(ZxError::BAD_STATE);
                }
                match inner.state {
                    InterruptState::Destroy => return Err(ZxError::CANCELED),
                    InterruptState::Triggered => {
                        inner.state = InterruptState::NeedAck;
                        // Use trigger_timestamp if timestamp was cleared
                        // (e.g., after trigger-with-port then unbind).
                        let timestamp = if inner.timestamp != 0 {
                            inner.timestamp
                        } else {
                            inner.trigger_timestamp
                        };
                        inner.timestamp = 0;
                        self.wait_event.signal_clear(Signal::SIGNALED);
                        return Ok(timestamp);
                    }
                    InterruptState::NeedAck => {
                        // If there's a pending re-trigger (timestamp > 0),
                        // consume it immediately without blocking (matches
                        // Fuchsia BeginWaitForInterrupt lines 79-89).
                        if inner.timestamp > 0 {
                            let timestamp = inner.timestamp;
                            inner.timestamp = 0;
                            self.wait_event.signal_clear(Signal::SIGNALED);
                            return Ok(timestamp);
                        }
                        if self.flags.contains(InterruptFlags::UNMASK_PREWAIT) {
                            self.trait_.unmask();
                        } else if self.flags.contains(InterruptFlags::UNMASK_PREWAIT_UNLOCKED) {
                            defer_unmask = true;
                        }
                    }
                    InterruptState::Idle => (),
                    _ => return Err(ZxError::BAD_STATE),
                }
                inner.state = InterruptState::Waiting;
                // For virtual interrupts, assert UNTRIGGERED when entering wait
                // (matches Fuchsia: UpdateStateLocked(0, ZX_VIRTUAL_INTERRUPT_UNTRIGGERED))
                if self.flags.contains(InterruptFlags::VIRTUAL) {
                    self.base.signal_set(Signal::INTERRUPT_SIGNAL);
                }
                // Clear the internal wakeup signal before blocking.
                self.wait_event.signal_clear(Signal::SIGNALED);
            }
            if defer_unmask {
                self.trait_.unmask();
            }
            // Block on the internal wait_event, not the base signals.
            WaitEventFuture::new(&self.wait_event).await;
        }
    }

    fn handle_interrupt(&self) {
        let mut inner = self.inner.lock();
        if self.flags.contains(InterruptFlags::MASK_POSTWAIT) {
            self.trait_.mask();
        }
        if inner.timestamp == 0 {
            // Not sure ZX_CLOCK_MONOTONIC or ZX_CLOCK_UTC
            inner.timestamp = hal_impl::timer::timer_now().as_nanos() as i64;
        }
        // Save trigger_timestamp so it survives unbind/rebind
        // (physical interrupts never go through trigger()).
        inner.trigger_timestamp = inner.timestamp;
        match &inner.port {
            Some(port) => {
                if inner.state != InterruptState::NeedAck {
                    inner.packet_id = port.as_ref().push_interrupt(inner.timestamp, inner.key);
                    if self.flags.contains(InterruptFlags::MASK_POSTWAIT) {
                        self.trait_.mask();
                    }
                    inner.timestamp = 0;

                    inner.state = InterruptState::NeedAck;
                }
            }
            None => {
                // Wake wait() via the internal event.
                self.wait_event.signal_set(Signal::SIGNALED);
                inner.state = InterruptState::Triggered;
            }
        }
    }
}

#[derive(PartialEq, Debug, Default)]
enum InterruptState {
    Waiting = 0,
    Destroy = 1,
    Triggered = 2,
    NeedAck = 3,
    #[default]
    Idle = 4,
}

bitflags! {
    /// Bits for Interrupt.flags.
    pub struct InterruptFlags: u32 {
        #[allow(clippy::identity_op)]
        /// The interrupt is virtual.
        const VIRTUAL                  = 1 << 0;
        /// The interrupt should be unmasked before waiting on the event.
        const UNMASK_PREWAIT           = 1 << 1;
        /// The same as **INTERRUPT_UNMASK_PREWAIT** except release the dispatcher
        /// spinlock before waiting.
        const UNMASK_PREWAIT_UNLOCKED  = 1 << 2;
        /// The interrupt should be masked following waiting.
        const MASK_POSTWAIT            = 1 << 4;
    }
}

bitflags! {
    /// Interrupt bind flags.
    pub struct InterruptOptions: u32 {
        #[allow(clippy::identity_op)]
        /// Remap interrupt request(IRQ).
        const REMAP_IRQ = 0x1;
        /// Default mode.
        const MODE_DEFAULT = 0 << 1;
        /// Falling edge triggered.
        const MODE_EDGE_LOW = 1 << 1;
        /// Rising edge triggered.
        const MODE_EDGE_HIGH = 2 << 1;
        /// Low level triggered.
        const MODE_LEVEL_LOW = 3 << 1;
        /// High level triggered.
        const MODE_LEVEL_HIGH = 4 << 1;
        /// Falling/rising edge triggered.
        const MODE_EDGE_BOTH = 5 << 1;
        /// Virtual interrupt.
        const VIRTUAL = 0x10;
        /// Wake vector — interrupt can wake from suspend.
        const WAKE_VECTOR = 0x20;
    }
}

impl InterruptOptions {
    /// Extract the mode bits.
    pub fn to_mode(self) -> Self {
        InterruptOptions::from_bits_truncate(0xe) & self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;

    #[async_std::test]
    async fn bind() {
        let interrupt = Interrupt::new_virtual();
        let port = Port::new(1).unwrap();
        assert_eq!(interrupt.unbind(&port).unwrap_err(), ZxError::NOT_FOUND);
        assert!(interrupt.bind(&port, 1).is_ok());

        assert!(interrupt.destroy().is_ok());
        assert_eq!(interrupt.unbind(&port).unwrap_err(), ZxError::CANCELED);

        let interrupt = Interrupt::new_virtual();
        assert_eq!(interrupt.unbind(&port).unwrap_err(), ZxError::NOT_FOUND);
        assert!(interrupt.bind(&port, 1).is_ok());

        assert!(interrupt.trigger(1234).is_ok());
        let packet = port.wait().await;
        assert_eq!(
            PortPacketRepr::from(&packet),
            PortPacketRepr {
                key: 1,
                status: ZxError::OK,
                data: PayloadRepr::Interrupt(PacketInterrupt {
                    timestamp: 1234,
                    _reserved0: 0,
                    _reserved1: 0,
                    _reserved2: 0,
                }),
            }
        );
        assert!(interrupt.unbind(&port).is_ok());
    }

    #[async_std::test]
    async fn wait_unbound() {
        // Virtual interrupt without a port: trigger then wait
        let interrupt = Interrupt::new_virtual();

        // Spawn a task that triggers the interrupt after a delay
        let int_clone = interrupt.clone();
        async_std::task::spawn(async move {
            async_std::task::sleep(Duration::from_millis(10)).await;
            int_clone.trigger(5678).unwrap();
        });

        // Wait should return the timestamp
        let timestamp = interrupt.wait().await.unwrap();
        assert_eq!(timestamp, 5678);
    }

    #[async_std::test]
    async fn wait_returns_bad_state_when_port_bound() {
        // Wait should fail with BAD_STATE when a port is bound
        let interrupt = Interrupt::new_virtual();
        let port = Port::new(1).unwrap();
        interrupt.bind(&port, 1).unwrap();

        assert_eq!(interrupt.wait().await.unwrap_err(), ZxError::BAD_STATE);
    }

    #[async_std::test]
    async fn wait_returns_canceled_when_destroyed() {
        // Wait should fail with CANCELED when the interrupt is destroyed
        let interrupt = Interrupt::new_virtual();
        interrupt.destroy().unwrap();

        assert_eq!(interrupt.wait().await.unwrap_err(), ZxError::CANCELED);
    }

    #[async_std::test]
    async fn ack_port_bound() {
        // Ack should transition from NeedAck to Idle when port is bound
        let interrupt = Interrupt::new_virtual();
        let port = Port::new(1).unwrap();
        interrupt.bind(&port, 42).unwrap();

        // Trigger sends a packet to port, state -> NeedAck
        interrupt.trigger(9999).unwrap();

        // Consume the port packet
        let packet = port.wait().await;
        assert_eq!(
            PortPacketRepr::from(&packet),
            PortPacketRepr {
                key: 42,
                status: ZxError::OK,
                data: PayloadRepr::Interrupt(PacketInterrupt {
                    timestamp: 9999,
                    _reserved0: 0,
                    _reserved1: 0,
                    _reserved2: 0,
                }),
            }
        );

        // Ack should succeed (transitions NeedAck -> Idle)
        assert!(interrupt.ack().is_ok());
    }

    #[test]
    fn ack_without_port_returns_bad_state() {
        // Ack without a port bound should return BAD_STATE
        let interrupt = Interrupt::new_virtual();
        assert_eq!(interrupt.ack().unwrap_err(), ZxError::BAD_STATE);
    }

    #[test]
    fn ack_after_destroy_returns_canceled() {
        let interrupt = Interrupt::new_virtual();
        let port = Port::new(1).unwrap();
        interrupt.bind(&port, 1).unwrap();
        interrupt.trigger(100).unwrap();
        interrupt.destroy().unwrap();

        assert_eq!(interrupt.ack().unwrap_err(), ZxError::CANCELED);
    }
}
