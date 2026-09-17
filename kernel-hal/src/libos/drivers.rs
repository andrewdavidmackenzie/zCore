#[cfg(feature = "mock-uart")]
pub(super) fn init_early() {
    use crate::drivers;
    use alloc::sync::Arc;
    use kernel_drivers::uart::MockUart;
    use kernel_drivers::{scheme::Scheme, Device};

    let uart = Arc::new(MockUart::new());
    drivers::add_device(Device::Uart(uart.clone()));
    MockUart::start_irq_service(move || uart.handle_irq(0));
}

#[cfg(not(feature = "mock-uart"))]
pub(super) fn init_early() {
    // No UART driver configured for this libos build.
}

pub(super) fn init() {
    // graphic and loopback features removed (see #237).
    // Display, input, and network drivers are in petal/drivers/.
}
