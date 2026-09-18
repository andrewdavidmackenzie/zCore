//! UART device drivers.
//!
//! Each UART variant is gated by its own feature flag.

mod buffered;
pub use buffered::BufferedUart;

#[cfg(feature = "uart-16550")]
mod uart_16550;
#[cfg(feature = "uart-16550")]
pub use uart_16550::Uart16550Mmio;
#[cfg(all(feature = "uart-16550", target_arch = "x86_64"))]
pub use uart_16550::Uart16550Pmio;

#[cfg(feature = "pl011-uart")]
mod uart_pl011;
#[cfg(feature = "pl011-uart")]
pub use uart_pl011::Pl011Uart;

#[cfg(feature = "allwinner")]
mod uart_allwinner;

#[cfg(feature = "allwinner")]
pub use uart_allwinner::UartAllwinner;

#[cfg(feature = "fu740")]
mod uart_u740;

#[cfg(feature = "fu740")]
pub use uart_u740::UartU740Mmio;

/// Mock UART for LibOS mode (reads from host stdin, writes to host stderr).
#[cfg(feature = "mock-uart")]
mod mock_uart;

#[cfg(feature = "mock-uart")]
pub use mock_uart::MockUart;
