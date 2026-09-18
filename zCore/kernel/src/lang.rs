// Rust language features implementations

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n\npanic cpu={}\n{}", hal_impl::cpu::cpu_id(), info);
    error!("\n\n{info}");

    if cfg!(feature = "baremetal-test") {
        hal_impl::cpu::reset();
    } else {
        loop {
            core::hint::spin_loop();
        }
    }
}
