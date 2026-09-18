//! Platform capabilities for libos (hosted) mode.

hal_fn_impl! {
    impl mod crate::hal_fn::platform {
        fn is_hosted() -> bool {
            true
        }

        fn needs_user_write_flush() -> bool {
            cfg!(all(target_arch = "aarch64", target_os = "macos"))
        }

        fn needs_kernel_textrel() -> bool {
            true
        }

        fn syscall_args_from_stack() -> bool {
            cfg!(target_arch = "x86_64")
        }
    }
}
