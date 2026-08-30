use core::fmt::{Result, Write};

use lock::Mutex;
use virtio_drivers::device::console::VirtIOConsole as InnerDriver;
use virtio_drivers::transport::mmio::MmioTransport;

use super::HalImpl;
use crate::prelude::DeviceResult;
use crate::scheme::{impl_event_scheme, Scheme, UartScheme};
use crate::utils::EventListener;

pub struct VirtIoConsole {
    inner: Mutex<InnerDriver<HalImpl, MmioTransport>>,
    listener: EventListener,
}

impl_event_scheme!(VirtIoConsole);

impl VirtIoConsole {
    pub fn new(transport: MmioTransport) -> DeviceResult<Self> {
        Ok(Self {
            inner: Mutex::new(InnerDriver::new(transport)?),
            listener: EventListener::new(),
        })
    }
}

impl Scheme for VirtIoConsole {
    fn name(&self) -> &str {
        "virtio-console"
    }

    fn handle_irq(&self, _irq_num: usize) {
        // ack_interrupt now returns Result<bool>; discard the value.
        let _ = self.inner.lock().ack_interrupt();
        self.listener.trigger(());
    }
}

impl UartScheme for VirtIoConsole {
    fn try_recv(&self) -> DeviceResult<Option<u8>> {
        Ok(self.inner.lock().recv(true)?)
    }

    fn send(&self, ch: u8) -> DeviceResult {
        self.inner.lock().send(ch)?;
        Ok(())
    }
}

impl Write for VirtIoConsole {
    fn write_str(&mut self, s: &str) -> Result {
        for b in s.bytes() {
            self.send(b).unwrap()
        }
        Ok(())
    }
}
