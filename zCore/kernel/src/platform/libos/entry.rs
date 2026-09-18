#[no_mangle]
fn main() {
    crate::primary_main(hal_impl::KernelConfig {
        cmdline: env!("ZCORE_CMDLINE"),
        ..Default::default()
    });
}
