use alloc::sync::Arc;

use crate::drivers;
use kernel_drivers::uart::MockUart;
use kernel_drivers::{scheme::Scheme, Device};

pub(super) fn init_early() {
    let uart = Arc::new(MockUart::new());
    drivers::add_device(Device::Uart(uart.clone()));
    MockUart::start_irq_service(move || uart.handle_irq(0));
}

pub(super) fn init() {
    // graphic and loopback features removed (see #237).
    // Display, input, and network drivers are in petal/drivers/.
}
