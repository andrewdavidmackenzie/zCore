//! SBI (Supervisor Binary Interface) support for RISC-V.
//!
//! All SBI calls go through the `sbi_rt` crate. This module only
//! provides the `console_write_early` hal_fn implementation, which
//! uses the legacy SBI console putchar for boot-time output before
//! the UART driver is available.

hal_fn_impl! {
    impl mod crate::hal_fn::console {
        fn console_write_early(s: &str) {
            for c in s.bytes() {
                #[allow(deprecated)]
                sbi_rt::legacy::console_putchar(c as usize);
            }
        }
    }
}
