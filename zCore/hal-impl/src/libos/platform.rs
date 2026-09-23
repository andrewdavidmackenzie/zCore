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

        fn libos_rootfs_path(flavour: &str) -> Option<String> {
            let project_dir = if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
                std::path::Path::new(&dir).parent().unwrap().to_path_buf()
            } else {
                std::env::current_dir().unwrap()
            };
            let arch = if cfg!(target_arch = "x86_64") {
                "x86_64"
            } else if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else if cfg!(target_arch = "riscv64") {
                "riscv64"
            } else {
                "unknown"
            };
            let rootfs_base = project_dir.join("target").join("rootfs");
            // On aarch64 macOS, prefer the libos-specific rootfs with
            // static-PIE binaries.
            #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
            {
                let libos_path = rootfs_base
                    .join(format!("{flavour}-libos"))
                    .join(arch);
                if libos_path.is_dir() {
                    return Some(libos_path.to_string_lossy().into_owned());
                }
            }
            let path = rootfs_base.join(flavour).join(arch);
            Some(path.to_string_lossy().into_owned())
        }
    }
}
