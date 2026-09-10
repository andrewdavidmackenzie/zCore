cfg_if! {
    if #[cfg(feature = "linux")] {
        use alloc::sync::Arc;
        use rcore_fs::vfs::FileSystem;

        #[cfg(feature = "libos")]
        pub fn rootfs() -> Arc<dyn FileSystem> {
            let path = libos_rootfs_path("linux");
            info!("LibOS rootfs: {}", path.display());
            rcore_fs_hostfs::HostFS::new(path)
        }

        #[cfg(not(feature = "libos"))]
        pub fn rootfs() -> Arc<dyn FileSystem> {
            use rcore_fs::dev::Device;

            let device: Arc<dyn Device> = {
                #[cfg(feature = "mock-disk")]{
                    let block = linux_object::fs::mock_block();
                    Arc::new(block)
                }
                #[cfg(not(feature = "mock-disk"))] {
                    use linux_object::fs::rcore_fs_wrapper::*;
                    if let Some(initrd) = init_ram_disk() {
                        Arc::new(MemBuf::new(initrd))
                    } else {
                        let block = kernel_hal::drivers::all_block().first_unwrap();
                        Arc::new(BlockCache::new(Block::new(block), 0x100))
                    }
                }
            };
            info!("Opening the rootfs...");
            rcore_fs_sfs::SimpleFileSystem::open(device).expect("failed to open device SimpleFS")
        }
    } else if #[cfg(feature = "zircon")] {
        #[cfg(not(feature = "libos"))]
        use alloc::sync::Arc;
        #[cfg(not(feature = "libos"))]
        use rcore_fs::vfs::FileSystem;

        /// Try to open a Zircon rootfs via HostFS in libos mode.
        /// Returns a filesystem rooted at `rootfs/zircon/{host_arch}/`.
        #[cfg(feature = "libos")]
        pub fn try_libos_rootfs() -> Option<alloc::sync::Arc<dyn rcore_fs::vfs::FileSystem>> {
            let path = libos_rootfs_path("zircon");
            if path.is_dir() && path.join("bin").is_dir() {
                info!("LibOS Zircon rootfs: {}", path.display());
                Some(rcore_fs_hostfs::HostFS::new(path))
            } else {
                None
            }
        }

        #[cfg(feature = "libos")]
        pub fn zbi() -> impl AsRef<[u8]> {
            let path = std::env::args().nth(1).expect(
                "Usage: zcore-libos <ZBI_FILE>\n\
                 Build a petal ZBI with: cargo petal-zbi --arch aarch64"
            );
            std::fs::read(path).expect("failed to read ZBI file")
        }

        #[cfg(not(feature = "libos"))]
        pub fn zbi() -> impl AsRef<[u8]> {
            // The petal ZBI is embedded at compile time via the PETAL_ZBI env var.
            // If not set, build.rs provides an empty stub -- the rootfs-based
            // boot path should be used instead.
            const ZBI_DATA: &[u8] = include_bytes!(env!("PETAL_ZBI"));
            ZBI_DATA
        }



        /// Try to open an SFS rootfs (from VirtIO block device or initrd).
        /// Returns None if no rootfs device is available.
        #[cfg(not(feature = "libos"))]
        pub fn try_rootfs() -> Option<Arc<dyn FileSystem>> {
            use rcore_fs_sfs::SimpleFileSystem;

            // Try initrd first (riscv64, x86_64)
            if let Some(initrd) = zircon_init_ram_disk() {
                info!("Trying Zircon rootfs from initrd...");
                let dev = Arc::new(MemBufDevice(spin::Mutex::new(initrd)));
                if let Ok(fs) = SimpleFileSystem::open(dev) {
                    let fs: Arc<dyn FileSystem> = fs;
                    return Some(fs);
                }
                warn!("Initrd is not a valid SFS image, trying block device...");
            }

            // Try VirtIO block device (aarch64, or fallback from initrd)
            if let Some(block) = kernel_hal::drivers::all_block().first() {
                info!("Trying Zircon rootfs from block device...");
                let dev: Arc<dyn rcore_fs::dev::Device> = Arc::new(BlockDevice(block));
                if let Ok(fs) = SimpleFileSystem::open(dev) {
                    let fs: Arc<dyn FileSystem> = fs;
                    return Some(fs);
                }
                warn!("Block device is not a valid SFS image");
            }

            None
        }
    }
}

/// Construct the libos rootfs path.
///
/// On aarch64 macOS, uses `rootfs/{personality}-libos/{arch}/` which
/// contains a static-PIE busybox (needed because non-PIE binaries
/// can't be loaded above macOS's ~0x400000000 address space minimum).
/// On other platforms, uses `rootfs/{personality}/{arch}/` (same as
/// bare-metal).
#[cfg(feature = "libos")]
fn libos_rootfs_path(personality: &str) -> std::path::PathBuf {
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
    // On aarch64 macOS, use the separate libos rootfs with static-PIE binaries.
    // Fall back to the shared rootfs if the libos one doesn't exist yet.
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    {
        let libos_path = project_dir
            .join("rootfs")
            .join(format!("{personality}-libos"))
            .join(arch);
        if libos_path.is_dir() {
            return libos_path;
        }
    }
    project_dir.join("rootfs").join(personality).join(arch)
}

#[cfg(all(not(feature = "libos"), feature = "linux"))]
pub(crate) fn init_ram_disk() -> Option<&'static mut [u8]> {
    if cfg!(feature = "link-user-img") {
        extern "C" {
            fn _user_img_start();
            fn _user_img_end();
        }
        Some(unsafe {
            core::slice::from_raw_parts_mut(
                _user_img_start as *const () as usize as *mut u8,
                _user_img_end as *const () as usize - _user_img_start as *const () as usize,
            )
        })
    } else {
        kernel_hal::boot::init_ram_disk()
    }
}

/// Try to get an initrd for Zircon mode (same mechanism as Linux).
#[cfg(all(not(feature = "libos"), feature = "zircon"))]
fn zircon_init_ram_disk() -> Option<&'static mut [u8]> {
    kernel_hal::boot::init_ram_disk()
}

/// Minimal rcore-fs Device wrapper for an in-memory buffer.
#[cfg(all(not(feature = "libos"), feature = "zircon"))]
struct MemBufDevice(spin::Mutex<&'static mut [u8]>);

#[cfg(all(not(feature = "libos"), feature = "zircon"))]
impl rcore_fs::dev::Device for MemBufDevice {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> rcore_fs::dev::Result<usize> {
        let data = self.0.lock();
        if offset >= data.len() {
            return Ok(0);
        }
        let len = buf.len().min(data.len() - offset);
        buf[..len].copy_from_slice(&data[offset..offset + len]);
        Ok(len)
    }
    fn write_at(&self, offset: usize, buf: &[u8]) -> rcore_fs::dev::Result<usize> {
        let mut data = self.0.lock();
        if offset >= data.len() {
            return Ok(0);
        }
        let len = buf.len().min(data.len() - offset);
        data[offset..offset + len].copy_from_slice(&buf[..len]);
        Ok(len)
    }
    fn sync(&self) -> rcore_fs::dev::Result<()> {
        Ok(())
    }
}

/// Minimal rcore-fs Device wrapper for a VirtIO block device.
#[cfg(all(not(feature = "libos"), feature = "zircon"))]
struct BlockDevice(alloc::sync::Arc<dyn kernel_hal::drivers::scheme::BlockScheme>);

#[cfg(all(not(feature = "libos"), feature = "zircon"))]
impl rcore_fs::dev::Device for BlockDevice {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> rcore_fs::dev::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        const BLK_SIZE: usize = 512;
        let start_blk = offset / BLK_SIZE;
        let end_blk = offset
            .checked_add(buf.len())
            .and_then(|end| end.checked_add(BLK_SIZE - 1))
            .map(|v| v / BLK_SIZE)
            .ok_or(rcore_fs::dev::DevError)?;
        let mut tmp = alloc::vec![0u8; (end_blk - start_blk) * BLK_SIZE];
        for (i, blk) in (start_blk..end_blk).enumerate() {
            self.0
                .read_block(blk, &mut tmp[i * BLK_SIZE..(i + 1) * BLK_SIZE])
                .map_err(|_| rcore_fs::dev::DevError)?;
        }
        let skip = offset % BLK_SIZE;
        buf.copy_from_slice(&tmp[skip..skip + buf.len()]);
        Ok(buf.len())
    }
    fn write_at(&self, offset: usize, buf: &[u8]) -> rcore_fs::dev::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        const BLK_SIZE: usize = 512;
        let start_blk = offset / BLK_SIZE;
        let end_blk = offset
            .checked_add(buf.len())
            .and_then(|end| end.checked_add(BLK_SIZE - 1))
            .map(|v| v / BLK_SIZE)
            .ok_or(rcore_fs::dev::DevError)?;
        let skip = offset % BLK_SIZE;
        let mut tmp = alloc::vec![0u8; (end_blk - start_blk) * BLK_SIZE];
        // Read existing data (propagate errors)
        for (i, blk) in (start_blk..end_blk).enumerate() {
            self.0
                .read_block(blk, &mut tmp[i * BLK_SIZE..(i + 1) * BLK_SIZE])
                .map_err(|_| rcore_fs::dev::DevError)?;
        }
        // Overlay new data
        tmp[skip..skip + buf.len()].copy_from_slice(buf);
        // Write back
        for (i, blk) in (start_blk..end_blk).enumerate() {
            self.0
                .write_block(blk, &tmp[i * BLK_SIZE..(i + 1) * BLK_SIZE])
                .map_err(|_| rcore_fs::dev::DevError)?;
        }
        Ok(buf.len())
    }
    fn sync(&self) -> rcore_fs::dev::Result<()> {
        Ok(())
    }
}

// Hard link rootfs img
#[cfg(all(not(feature = "libos"), feature = "linux"))]
#[cfg(feature = "link-user-img")]
core::arch::global_asm!(concat!(
    r#"
    .section .data.img
    .global _user_img_start
    .global _user_img_end
_user_img_start:
    .incbin ""#,
    env!("USER_IMG"),
    r#""
_user_img_end:
"#
));
