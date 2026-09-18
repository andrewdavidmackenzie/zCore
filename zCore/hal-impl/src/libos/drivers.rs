#[cfg(feature = "mock-uart")]
pub(super) fn init_early() {
    use ::drivers::uart::MockUart;
    use ::drivers::{scheme::Scheme, Device};
    use alloc::sync::Arc;

    let uart = Arc::new(MockUart::new());
    crate::device_registry::add_device(Device::Uart(uart.clone()));
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
