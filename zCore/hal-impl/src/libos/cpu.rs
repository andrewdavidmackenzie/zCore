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
            // LibOS runs on the host thread pool. Report host
            // parallelism capped at the build-time maximum.
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
                .min(crate::config::MAX_CORE_NUM)
        }

        fn reset() -> ! {
            info!("shutdown...");
            std::process::exit(0);
        }
    }
}
