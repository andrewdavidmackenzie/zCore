#[no_mangle]
fn main() {
    crate::primary_main(kernel_hal::KernelConfig {
        cmdline: env!("ZCORE_CMDLINE"),
    });
}
