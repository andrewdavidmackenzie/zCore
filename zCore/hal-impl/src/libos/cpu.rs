//! CPU information.

hal_fn_impl! {
    impl mod crate::hal_fn::cpu {
        fn cpu_id() -> u8 {
            std::thread::current().id().as_u64().get() as u8
        }

        fn cpu_index() -> usize {
            0 // LibOS is single-threaded from the kernel's perspective.
        }

        fn reset() -> ! {
            info!("shutdown...");
            std::process::exit(0);
        }
    }
}
