//! CPU information.

hal_fn_impl! {
    impl mod crate::hal_fn::cpu {
        fn cpu_id() -> u8 {
            std::thread::current().id().as_u64().get() as u8
        }

        fn cpu_index() -> usize {
            0 // LibOS is single-threaded from the kernel's perspective.
        }

        fn num_cpus() -> usize {
            // LibOS has a single logical kernel CPU (cpu_index always
            // returns 0). Report 1 regardless of host parallelism.
            1
        }

        fn reset() -> ! {
            info!("shutdown...");
            std::process::exit(0);
        }
    }
}
