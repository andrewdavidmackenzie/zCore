// Bare-metal panic handler.

use core::fmt::Write;
use core::panic::PanicInfo;

/// Minimal writer that goes directly to the early console,
/// bypassing the log framework (which may not be initialized).
struct PanicWriter;

impl Write for PanicWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        crate::hal_fn::console::console_write_early(s);
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let cpu_id = crate::cpu::cpu_id();
    let _ = write!(PanicWriter, "\npanic cpu={}\n{}\n", cpu_id, info);

    loop {
        core::hint::spin_loop();
    }
}
