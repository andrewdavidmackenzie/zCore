pub use self::port_packet::*;
use crate::object::*;
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::sync::atomic::AtomicBool;
use lock::Mutex;

#[path = "port_packet.rs"]
mod port_packet;

const MAX_ALLOCATED_PACKET_COUNT: usize = 16 * 1024;
/// Per-port packet limit. Fuchsia uses 4096 (kMaxAllocatedPacketCountPerPort).
const MAX_ALLOCATED_PACKET_COUNT_PER_PORT: usize = MAX_ALLOCATED_PACKET_COUNT / 4;

/// Signaling and mailbox primitive
///
/// ## SYNOPSIS
///
/// Ports allow threads to wait for packets to be delivered from various
/// events. These events include explicit queueing on the port,
/// asynchronous waits on other handles bound to the port, and
/// asynchronous message delivery from IPC transports.
pub struct Port {
    base: KObjectBase,
    options: PortOptions,
    inner: Mutex<PortInner>,
}

impl_kobject!(Port);

/// A queued packet with optional source-tracking for signal packets.
/// The `source_koid` allows `port_cancel` to remove already-queued
/// packets that match a `(source, key)` pair.
struct QueuedPacket {
    packet: PortPacket,
    /// Source object koid for signal packets; 0 for user packets.
    source_koid: KoID,
}

#[derive(Default)]
struct PortInner {
    queue: VecDeque<QueuedPacket>,
    interrupt_queue: VecDeque<PortInterruptPacket>,
    interrupt_grave: BTreeSet<u64>,
    interrupt_pid: u64,
    /// Cancellation flags for signal callbacks registered by wait_async.
    /// Keyed by (source_koid, key). When port_cancel is called, the flag
    /// is set to true and the callback will remove itself on next invocation.
    async_subscriptions: BTreeMap<(KoID, u64), Vec<Arc<AtomicBool>>>,
}

#[derive(Debug)]
struct PortInterruptPacket {
    timestamp: i64,
    key: u64,
    pid: u64,
}

impl From<PortInterruptPacket> for PacketInterrupt {
    fn from(packet: PortInterruptPacket) -> Self {
        PacketInterrupt {
            timestamp: packet.timestamp,
            _reserved0: 0,
            _reserved1: 0,
            _reserved2: 0,
        }
    }
}

impl Port {
    /// Create a new `Port`.
    pub fn new(options: u32) -> ZxResult<Arc<Self>> {
        Ok(Arc::new(Port {
            base: KObjectBase::default(),
            options: PortOptions::from_bits(options).ok_or(ZxError::INVALID_ARGS)?,
            inner: Mutex::default(),
        }))
    }

    /// Register an async signal subscription for cancellation tracking.
    ///
    /// Returns a shared cancellation flag. When the flag is set to `true`,
    /// the signal callback will remove itself on its next invocation.
    pub fn register_async(&self, source_koid: KoID, key: u64) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut inner = self.inner.lock();
        inner
            .async_subscriptions
            .entry((source_koid, key))
            .or_default()
            .push(flag.clone());
        flag
    }

    /// Cancel async signal subscriptions only (no queue drain).
    ///
    /// Used internally after one-shot signal delivery to clean up the
    /// subscription entry without removing the just-queued packet.
    pub fn cancel_async(&self, source_koid: KoID, key: u64) -> ZxResult {
        let mut inner = self.inner.lock();
        if let Some(flags) = inner.async_subscriptions.remove(&(source_koid, key)) {
            for flag in flags {
                flag.store(true, core::sync::atomic::Ordering::Relaxed);
            }
            Ok(())
        } else {
            Err(ZxError::NOT_FOUND)
        }
    }

    /// Cancel async signal subscriptions AND drain queued signal packets.
    ///
    /// This is the full cancel used by `zx_port_cancel` — it removes
    /// pending callbacks AND already-queued packets matching `(source, key)`.
    ///
    /// Returns `Ok(())` if anything matched, `Err(NOT_FOUND)` otherwise.
    pub fn cancel_async_and_drain(&self, source_koid: KoID, key: u64) -> ZxResult {
        let mut inner = self.inner.lock();
        let mut found = false;
        // Cancel pending subscription callbacks.
        if let Some(flags) = inner.async_subscriptions.remove(&(source_koid, key)) {
            for flag in flags {
                flag.store(true, core::sync::atomic::Ordering::Relaxed);
            }
            found = true;
        }
        // Remove already-queued signal packets matching (source, key).
        let before = inner.queue.len();
        inner
            .queue
            .retain(|qp| !(qp.source_koid == source_koid && qp.packet.key == key));
        if inner.queue.len() < before {
            found = true;
        }
        // Clear READABLE while still holding the inner lock to prevent
        // a race with concurrent push_with_source.
        let has_interrupt = self.can_bind_to_interrupt() && !inner.interrupt_queue.is_empty();
        if inner.queue.is_empty() && !has_interrupt {
            self.base.signal_clear(Signal::READABLE);
        }
        drop(inner);
        if found {
            Ok(())
        } else {
            Err(ZxError::NOT_FOUND)
        }
    }

    /// Cancel all async waits matching the given key, regardless of source.
    pub fn cancel_by_key(&self, key: u64) -> ZxResult {
        let mut inner = self.inner.lock();
        let mut found = false;
        let matching: alloc::vec::Vec<_> = inner
            .async_subscriptions
            .keys()
            .filter(|(_, k)| *k == key)
            .cloned()
            .collect();
        if !matching.is_empty() {
            found = true;
        }
        for compound_key in matching {
            if let Some(flags) = inner.async_subscriptions.remove(&compound_key) {
                for flag in flags {
                    flag.store(true, core::sync::atomic::Ordering::Relaxed);
                }
            }
        }
        // Remove ALL queued packets matching the key, including user packets.
        // Fuchsia's port_cancel_key removes both async-wait and user packets.
        let before = inner.queue.len();
        inner.queue.retain(|qp| qp.packet.key != key);
        if inner.queue.len() < before {
            found = true;
        }
        // Clear READABLE while still holding the inner lock.
        let has_interrupt = self.can_bind_to_interrupt() && !inner.interrupt_queue.is_empty();
        if inner.queue.is_empty() && !has_interrupt {
            self.base.signal_clear(Signal::READABLE);
        }
        drop(inner);
        if found {
            Ok(())
        } else {
            Err(ZxError::NOT_FOUND)
        }
    }

    /// Push a `packet` into the port.
    pub fn push(&self, packet: impl Into<PortPacket>) {
        self.push_with_source(packet, 0);
    }

    /// Push a signal packet with source tracking for cancellation.
    pub fn push_with_source(&self, packet: impl Into<PortPacket>, source_koid: KoID) {
        let mut inner = self.inner.lock();
        inner.queue.push_back(QueuedPacket {
            packet: packet.into(),
            source_koid,
        });
        // Signal READABLE while still holding the port inner lock to
        // prevent a TOCTOU race: without this, a concurrent `wait()`
        // could dequeue the packet and `signal_clear(READABLE)` between
        // our `drop(inner)` and `signal_set`, leaving READABLE set on
        // an empty queue. The next `push()` would then be a no-op
        // (READABLE already set → callbacks don't fire → waiters hang).
        self.base.signal_set(Signal::READABLE);
        drop(inner);
    }

    /// Atomically check a cancellation flag and push a signal packet.
    ///
    /// Returns `true` if the packet was enqueued (flag was not set),
    /// `false` if the subscription was already cancelled. This prevents
    /// the TOCTOU race where `cancel_by_key` / `cancel_async_and_drain`
    /// removes the subscription and drains the queue, but the signal
    /// callback pushes a packet after the drain because it checked the
    /// `cancelled` flag outside the port lock.
    pub fn push_if_not_cancelled(
        &self,
        packet: impl Into<PortPacket>,
        source_koid: KoID,
        cancelled: &AtomicBool,
    ) -> bool {
        let mut inner = self.inner.lock();
        if cancelled.load(core::sync::atomic::Ordering::Relaxed) {
            return false;
        }
        inner.queue.push_back(QueuedPacket {
            packet: packet.into(),
            source_koid,
        });
        self.base.signal_set(Signal::READABLE);
        drop(inner);
        true
    }

    /// Push a `User` type `packet` into the port.
    ///
    /// Returns `SHOULD_WAIT` when the per-port packet limit is exceeded.
    /// In Fuchsia, exceeding this limit also generates a policy exception
    /// (`ZX_EXCP_POLICY_CODE_PORT_TOO_MANY_PACKETS`).
    pub fn push_user(&self, packet: impl Into<PortPacket>) -> ZxResult<()> {
        let mut packet = packet.into();
        packet.type_ = PacketType::User;
        // Hold the lock across the limit check and enqueue to prevent
        // concurrent calls from exceeding the per-port limit.
        let mut inner = self.inner.lock();
        if inner.queue.len() >= MAX_ALLOCATED_PACKET_COUNT_PER_PORT {
            return Err(ZxError::SHOULD_WAIT);
        }
        inner.queue.push_back(QueuedPacket {
            packet,
            source_koid: 0,
        });
        // Signal READABLE while still holding the port inner lock —
        // see push_with_source for rationale on the TOCTOU fix.
        self.base.signal_set(Signal::READABLE);
        drop(inner);
        Ok(())
    }

    /// Push an `InterruptPacket` into the port.
    pub(crate) fn push_interrupt(&self, timestamp: i64, key: u64) -> u64 {
        let mut inner = self.inner.lock();
        inner.interrupt_pid += 1;
        let pid = inner.interrupt_pid;
        inner.interrupt_queue.push_back(PortInterruptPacket {
            timestamp,
            key,
            pid,
        });
        inner.interrupt_grave.insert(pid);
        // Signal READABLE while still holding the port inner lock —
        // see push_with_source for rationale on the TOCTOU fix.
        self.base.signal_set(Signal::READABLE);
        drop(inner);
        pid
    }

    /// Remove an `InterruptPacket` from the port.
    /// Return whether the packet is in the port
    pub(crate) fn remove_interrupt(&self, pid: u64) -> bool {
        let mut inner = self.inner.lock();
        inner.interrupt_grave.remove(&pid)
    }

    /// Check whether an interrupt packet is still queued (not yet consumed).
    pub(crate) fn is_interrupt_packet_queued(&self, pid: u64) -> bool {
        let inner = self.inner.lock();
        inner.interrupt_grave.contains(&pid)
    }

    /// Asynchronous wait until at least one packet is available, then take out the earliest
    /// (in FIFO order) available packet.
    pub async fn wait(self: &Arc<Self>) -> PortPacket {
        let object = self.clone() as Arc<dyn KernelObject>;
        loop {
            object.wait_signal(Signal::READABLE).await;
            let mut inner = self.inner.lock();
            if self.can_bind_to_interrupt() {
                while let Some(packet) = inner.interrupt_queue.pop_front() {
                    let in_queue = inner.interrupt_grave.remove(&packet.pid);
                    if inner.queue.is_empty() && inner.interrupt_queue.is_empty() {
                        self.base.signal_clear(Signal::READABLE);
                    }
                    if !in_queue {
                        continue;
                    }
                    return PortPacketRepr {
                        key: packet.key,
                        status: ZxError::OK,
                        data: PayloadRepr::Interrupt(packet.into()),
                    }
                    .into();
                }
            }
            if let Some(qp) = inner.queue.pop_front() {
                let remaining = inner.queue.len();
                if inner.queue.is_empty()
                    && (inner.interrupt_queue.is_empty() || !self.can_bind_to_interrupt())
                {
                    self.base.signal_clear(Signal::READABLE);
                }
                trace!(
                    "port.wait: dequeued type={:#x} key={:#x} rem={} data={:?}",
                    qp.packet.type_ as u32,
                    qp.packet.key,
                    remaining,
                    PortPacketRepr::from(&qp.packet).data
                );
                return qp.packet;
            }
            trace!("port.wait: READABLE signaled but queue empty, retrying");
        }
    }

    /// Remove queued page request (READ) packets matching the given
    /// key and page range. Called by `complete_pager_requests` to
    /// drain resolved requests from the port so they don't confuse
    /// the pager's `WaitForPageRead`.
    #[allow(unsafe_code)]
    pub fn remove_page_read_packets(&self, key: u64, start_page: usize, end_page: usize) {
        use crate::vm::PAGE_SIZE;
        let mut inner = self.inner.lock();
        inner.queue.retain(|qp| {
            if qp.packet.type_ as u32 != PacketType::PageRequest as u32 || qp.packet.key != key {
                return true;
            }
            // Safe: we checked type_ == PageRequest above.
            let req = unsafe { &qp.packet.data.page_request };
            if req.command != ZX_PAGER_VMO_READ {
                return true;
            }
            let pkt_start = req.offset as usize / PAGE_SIZE;
            let pkt_end = pkt_start + (req.length as usize).max(PAGE_SIZE) / PAGE_SIZE;
            // Remove if the packet's range overlaps the resolved range.
            !(pkt_start >= start_page && pkt_end <= end_page)
        });
        if inner.queue.is_empty() && inner.interrupt_queue.is_empty() {
            self.base.signal_clear(Signal::READABLE);
        }
    }

    /// Get the number of packets in queue.
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.inner.lock().queue.len()
    }

    /// Check whether the port can be bound to an interrupt.
    pub fn can_bind_to_interrupt(&self) -> bool {
        self.options.contains(PortOptions::BIND_TO_INTERUPT)
    }
}

bitflags! {
    /// If you need this port to be bound to an interrupt, pass **BIND_TO_INTERRUPT** to *options*,
    /// otherwise it should be **0**.
    pub struct PortOptions: u32 {
        #[allow(clippy::identity_op)]
        /// Allow this port to be bound to an interrupt.
        const BIND_TO_INTERUPT         = 1 << 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;

    #[test]
    fn new() {
        assert!(Port::new(0).is_ok());
        assert!(Port::new(1).is_ok());
        assert_eq!(Port::new(2).unwrap_err(), ZxError::INVALID_ARGS);
    }

    #[async_std::test]
    async fn wait() {
        let port = Port::new(0).unwrap();
        let object = DummyObject::new() as Arc<dyn KernelObject>;
        object
            .send_signal_to_port_async(Signal::READABLE, &port, 1, false)
            .unwrap();

        let packet_repr2 = PortPacketRepr {
            key: 2,
            status: ZxError::OK,
            data: PayloadRepr::Signal(PacketSignal {
                trigger: Signal::WRITABLE,
                observed: Signal::WRITABLE,
                count: 1,
                timestamp: 0,
                _reserved1: 0,
            }),
        };
        async_std::task::spawn({
            let port = port.clone();
            let object = object.clone();
            let packet2 = packet_repr2.clone();
            async move {
                // Assert an irrelevant signal to test the `false` branch of the callback for `READABLE`.
                object.signal_set(Signal::USER_SIGNAL_0);
                object.signal_clear(Signal::USER_SIGNAL_0);
                object.signal_set(Signal::READABLE);
                async_std::task::sleep(Duration::from_millis(1)).await;
                port.push(packet2);
            }
        });

        let packet = port.wait().await;
        let packet_repr = PortPacketRepr {
            key: 1,
            status: ZxError::OK,
            data: PayloadRepr::Signal(PacketSignal {
                trigger: Signal::READABLE,
                observed: Signal::READABLE,
                count: 1,
                timestamp: 0,
                _reserved1: 0,
            }),
        };
        assert_eq!(PortPacketRepr::from(&packet), packet_repr);

        let packet = port.wait().await;
        assert_eq!(PortPacketRepr::from(&packet), packet_repr2);

        // Test asserting signal before `send_signal_to_port_async`.
        let port = Port::new(0).unwrap();
        let object = DummyObject::new() as Arc<dyn KernelObject>;
        object.signal_set(Signal::READABLE);
        object
            .send_signal_to_port_async(Signal::READABLE, &port, 1, false)
            .unwrap();
        let packet = port.wait().await;
        assert_eq!(PortPacketRepr::from(&packet), packet_repr);
    }

    #[test]
    fn cancel_async() {
        let port = Port::new(0).unwrap();
        let source = DummyObject::new();
        let source_koid = source.id();
        let key = 42u64;

        // Register an async subscription and get the cancel flag.
        let cancel_flag = port.register_async(source_koid, key);
        assert!(!cancel_flag.load(core::sync::atomic::Ordering::Relaxed));

        // Cancelling a matching subscription should succeed and set the flag.
        assert!(port.cancel_async(source_koid, key).is_ok());
        assert!(cancel_flag.load(core::sync::atomic::Ordering::Relaxed));

        // Cancelling again (no remaining subscription) should return NOT_FOUND.
        assert_eq!(
            port.cancel_async(source_koid, key).err(),
            Some(ZxError::NOT_FOUND)
        );

        // Cancelling with a nonexistent koid should return NOT_FOUND.
        assert_eq!(
            port.cancel_async(source_koid + 9999, key).err(),
            Some(ZxError::NOT_FOUND)
        );
    }

    #[test]
    fn cancel_async_multiple_flags() {
        let port = Port::new(0).unwrap();
        let source = DummyObject::new();
        let source_koid = source.id();
        let key = 7u64;

        // Register multiple subscriptions on the same (koid, key).
        let flag1 = port.register_async(source_koid, key);
        let flag2 = port.register_async(source_koid, key);

        // Both should start unset.
        assert!(!flag1.load(core::sync::atomic::Ordering::Relaxed));
        assert!(!flag2.load(core::sync::atomic::Ordering::Relaxed));

        // A single cancel should set all flags for that (koid, key).
        assert!(port.cancel_async(source_koid, key).is_ok());
        assert!(flag1.load(core::sync::atomic::Ordering::Relaxed));
        assert!(flag2.load(core::sync::atomic::Ordering::Relaxed));
    }
}
