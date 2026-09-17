//! PL011 UART.
use crate::scheme::{impl_event_scheme, Scheme, UartScheme};
use crate::utils::EventListener;
use crate::DeviceResult;
use bitflags::*;
use core::ptr;

bitflags! {
    /// UARTFR
    struct UartFrFlags: u16 {
        const TXFE = 1 << 7;
        const RXFF = 1 << 6;
        const TXFF = 1 << 5;
        const RXFE = 1 << 4;
        const BUSY = 1 << 3;
    }
}

bitflags! {
    /// UARTCR
    struct UartCrFlags: u16 {
        const RXE = 1 << 9;
        const TXE = 1 << 8;
        const UARTEN = 1 << 0;
    }
}

bitflags! {
    // UARTIMSC
    struct UartImscFlags: u16 {
        const RTIM = 1 << 6;
        const TXIM = 1 << 5;
        const RXIM = 1 << 4;
    }
}

bitflags! {
    // UARTICR
    struct UartIcrFlags: u16 {
        const RTIC = 1 << 6;
        const TXIC = 1 << 5;
        const RXIC = 1 << 4;
    }
}

bitflags! {
    //UARTMIS
    struct UartMisFlags: u16 {
        const TXMIS = 1 << 5;
        const RXMIS = 1 << 4;
    }
}

bitflags! {
    //UARTLCR_H
    struct UartLcrhFlags: u16 {
        const FEN = 1 << 4;
    }
}

#[allow(dead_code)]
pub struct Pl011Uart {
    inner: Pl011Inner,
    crlf: bool,
    listener: EventListener,
}

impl Pl011Uart {
    pub fn new(base: usize) -> Self {
        Self {
            inner: {
                let inner = Pl011Inner::new(base);
                inner.init();
                inner
            },
            crlf: false,
            listener: EventListener::new(),
        }
    }

    /// Create a Pl011Uart without touching hardware configuration.
    ///
    /// The firmware is expected to have already configured the UART
    /// (baud rate, line control, FIFOs). Only enables the RX interrupt.
    pub fn new_crlf(base: usize) -> Self {
        Self {
            inner: {
                let inner = Pl011Inner::new(base);
                inner.init_irq_only();
                inner
            },
            crlf: true,
            listener: EventListener::new(),
        }
    }

    /// Create a Pl011Uart and initialize with a specific baud rate.
    ///
    /// `uart_clock` is the input clock frequency in Hz (e.g. 48_000_000 for Pi 400).
    /// `baud_rate` is the desired baud rate (e.g. 9600).
    /// `crlf` controls whether `\r` is sent before `\n`.
    pub fn new_with_baud(base: usize, uart_clock: u32, baud_rate: u32, crlf: bool) -> Self {
        Self {
            inner: {
                let inner = Pl011Inner::new(base);
                inner.init_with_baud(uart_clock, baud_rate);
                inner
            },
            crlf,
            listener: EventListener::new(),
        }
    }

    fn getchar(&self) -> Option<u8> {
        self.inner.getchar()
    }

    fn putchar(&self, data: u8) {
        self.inner.putchar(data, self.crlf);
    }
}

struct Pl011Inner {
    base: usize,
    // PL011 register offsets
    data_reg: u8,             // 0x00 UARTDR
    flag_reg: u8,             // 0x18 UARTFR
    ibrd_reg: u8,             // 0x24 UARTIBRD (integer baud rate divisor)
    fbrd_reg: u8,             // 0x28 UARTFBRD (fractional baud rate divisor)
    line_ctrl_reg: u8,        // 0x2C UARTLCR_H
    ctrl_reg: u8,             // 0x30 UARTCR
    intr_mask_setclr_reg: u8, // 0x38 UARTIMSC
    intr_clr_reg: u8,         // 0x44 UARTICR
}

impl Pl011Inner {
    pub fn new(base: usize) -> Pl011Inner {
        Pl011Inner {
            base,
            data_reg: 0x00,
            flag_reg: 0x18,
            ibrd_reg: 0x24,
            fbrd_reg: 0x28,
            line_ctrl_reg: 0x2c,
            ctrl_reg: 0x30,
            intr_mask_setclr_reg: 0x38,
            intr_clr_reg: 0x44,
        }
    }

    fn read_reg(&self, register: u8) -> u16 {
        unsafe { ptr::read_volatile((self.base + register as usize) as *mut u16) }
    }

    fn write_reg(&self, register: u8, data: u16) {
        unsafe {
            ptr::write_volatile((self.base + register as usize) as *mut u16, data);
        }
    }

    /// Initialize the UART with a specific baud rate.
    ///
    /// `uart_clock` is the input clock frequency in Hz.
    /// `baud_rate` is the desired baud rate (e.g. 9600, 115200).
    ///
    /// PL011 init procedure (per ARM PrimeCell UART PL011 TRM):
    /// 1. Disable UART
    /// 2. Wait for current TX to complete
    /// 3. Flush FIFOs
    /// 4. Set baud rate divisors (IBRD, FBRD)
    /// 5. Set line control (8N1)
    /// 6. Clear pending interrupts
    /// 7. Enable RX interrupt
    /// 8. Re-enable UART
    fn init_with_baud(&self, uart_clock: u32, baud_rate: u32) {
        // 1. Disable UART
        self.write_reg(self.ctrl_reg, 0);

        // 2. Wait for any current TX to complete
        while self.line_sts().contains(UartFrFlags::BUSY) {}

        // 3. Flush FIFOs by disabling them
        let mut lcrh = UartLcrhFlags::from_bits_truncate(self.read_reg(self.line_ctrl_reg));
        lcrh.remove(UartLcrhFlags::FEN);
        self.write_reg(self.line_ctrl_reg, lcrh.bits());

        // 4. Set baud rate: divisor = uart_clock / (16 * baud_rate)
        //    IBRD = integer part, FBRD = round(fractional * 64)
        let divisor_x64 = ((uart_clock as u64) * 4) / (baud_rate as u64);
        let ibrd = (divisor_x64 / 64) as u16;
        let fbrd = (divisor_x64 % 64) as u16;
        self.write_reg(self.ibrd_reg, ibrd);
        self.write_reg(self.fbrd_reg, fbrd);

        // 5. Set line control: 8 data bits, no parity, 1 stop bit (8N1)
        //    WLEN bits [6:5] = 0b11 for 8 bits, FEN disabled
        self.write_reg(self.line_ctrl_reg, 0b11 << 5);

        // 6. Clear all pending interrupts
        self.write_reg(self.intr_clr_reg, 0x7ff);

        // 7. Enable RX interrupt
        let imsc = UartImscFlags::RXIM;
        self.write_reg(self.intr_mask_setclr_reg, imsc.bits);

        // 8. Enable UART with TX and RX
        let cr = UartCrFlags::UARTEN | UartCrFlags::TXE | UartCrFlags::RXE;
        self.write_reg(self.ctrl_reg, cr.bits());
    }

    /// Legacy init (no baud rate change -- for QEMU where firmware sets it up).
    fn init(&self) {
        // Enable RX, TX, UART
        let flags = UartCrFlags::RXE | UartCrFlags::TXE | UartCrFlags::UARTEN;
        self.write_reg(self.ctrl_reg, flags.bits());

        // Disable FIFOs (use character mode instead)
        let mut flags = UartLcrhFlags::from_bits_truncate(self.read_reg(self.line_ctrl_reg));
        flags.remove(UartLcrhFlags::FEN);
        self.write_reg(self.line_ctrl_reg, flags.bits());

        // Enable IRQs
        let flags = UartImscFlags::RXIM;
        self.write_reg(self.intr_mask_setclr_reg, flags.bits);

        // Clear pending interrupts
        self.write_reg(self.intr_clr_reg, 0x7ff);
    }

    /// Minimal init: only enable RX interrupt, don't touch UART config.
    /// For use when firmware has already configured the UART correctly.
    fn init_irq_only(&self) {
        let flags = UartImscFlags::RXIM;
        self.write_reg(self.intr_mask_setclr_reg, flags.bits);
        self.write_reg(self.intr_clr_reg, 0x7ff);
    }

    fn line_sts(&self) -> UartFrFlags {
        UartFrFlags::from_bits_truncate(self.read_reg(self.flag_reg))
    }

    fn getchar(&self) -> Option<u8> {
        if self.line_sts().contains(UartFrFlags::RXFF) {
            Some(self.read_reg(self.data_reg) as u8)
        } else {
            None
        }
    }

    fn putchar(&self, data: u8, crlf: bool) {
        if crlf && data == b'\n' {
            // Send \r\n for serial terminals
            while !self.line_sts().contains(UartFrFlags::TXFE) {}
            self.write_reg(self.data_reg, b'\r' as u16);
        }
        while !self.line_sts().contains(UartFrFlags::TXFE) {}
        self.write_reg(self.data_reg, data as u16);
    }
}

impl Scheme for Pl011Uart {
    fn name(&self) -> &str {
        "Pl011 ARM series uart"
    }

    fn handle_irq(&self, _irq_num: usize) {
        self.listener.trigger(())
    }
}

impl_event_scheme!(Pl011Uart);

impl UartScheme for Pl011Uart {
    fn try_recv(&self) -> DeviceResult<Option<u8>> {
        Ok(self.getchar())
    }

    fn send(&self, ch: u8) -> DeviceResult {
        self.putchar(ch);
        Ok(())
    }

    fn write_str(&self, s: &str) -> DeviceResult {
        for c in s.bytes() {
            self.send(c)?;
        }
        Ok(())
    }
}
