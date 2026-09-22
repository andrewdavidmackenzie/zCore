//! Filesystem initialization.
//!
//! Provides `rootfs()` (Linux) or `zbi()` + `try_zircon_rootfs()` (Zircon)
//! depending on the kernel personality. Platform differences (libos vs
//! bare-metal) are handled by hal-impl, not by cfg guards here.

// ── Linux personality ──────────────────────────────────────────────────

#[cfg(feature = "linux")]
pub fn rootfs() -> alloc::sync::Arc<dyn rcore_fs::vfs::FileSystem> {
    use alloc::sync::Arc;

    // LibOS mode: use HostFS from the rootfs directory on the host.
    #[cfg(feature = "libos")]
    if let Some(path) = hal_impl::platform::libos_rootfs_path("linux") {
        info!("LibOS rootfs: {}", path);
        return rcore_fs_hostfs::HostFS::new(path);
    }

    // Bare-metal: open an SFS image from initrd or block device.
    use rcore_fs::dev::Device;
    let device: Arc<dyn Device> = {
        use linux_object::fs::rcore_fs_wrapper::*;
        if let Some(initrd) = init_ram_disk() {
            Arc::new(MemBuf::new(initrd))
        } else if let Some(block) = hal_impl::device_registry::all_block().first() {
            Arc::new(BlockCache::new(Block::new(block), 0x100))
        } else {
            panic!(
                "No rootfs available: no initrd and no block device. \
                 On RPi 400, pass rootfs via -initrd or use Zircon mode."
            );
        }
    };
    info!("Opening the rootfs...");
    rcore_fs_sfs::SimpleFileSystem::open(device).expect("failed to open device SimpleFS")
}

// ── Zircon personality ─────────────────────────────────────────────────

#[cfg(feature = "zircon")]
pub fn zbi() -> impl AsRef<[u8]> {
    #[cfg(feature = "libos")]
    {
        let path = std::env::args().nth(1).expect(
            "Usage: zcore-libos <ZBI_FILE>\n\
             Build a petal ZBI with: cargo petal-zbi --arch aarch64",
        );
        std::fs::read(path).expect("failed to read ZBI file")
    }

    #[cfg(not(feature = "libos"))]
    {
        const ZBI_DATA: &[u8] = include_bytes!(env!("PETAL_ZBI"));
        ZBI_DATA.to_vec()
    }
}

/// Try to open a Zircon rootfs (works for both libos and bare-metal).
#[cfg(feature = "zircon")]
pub fn try_zircon_rootfs() -> Option<alloc::sync::Arc<dyn rcore_fs::vfs::FileSystem>> {
    // LibOS mode: use HostFS.
    #[cfg(feature = "libos")]
    if let Some(path) = hal_impl::platform::libos_rootfs_path("zircon") {
        let path = std::path::PathBuf::from(path);
        if path.is_dir() && path.join("bin").is_dir() {
            info!("LibOS Zircon rootfs: {}", path.display());
            return Some(rcore_fs_hostfs::HostFS::new(path));
        }
        return None;
    }

    // Bare-metal: try initrd or block device.
    use alloc::sync::Arc;
    use rcore_fs::vfs::FileSystem;
    use rcore_fs_sfs::SimpleFileSystem;

    if let Some(initrd) = hal_impl::boot::init_ram_disk() {
        info!("Trying Zircon rootfs from initrd...");
        let dev = Arc::new(MemBufDevice(spin::Mutex::new(initrd)));
        if let Ok(fs) = SimpleFileSystem::open(dev) {
            let fs: Arc<dyn FileSystem> = fs;
            return Some(fs);
        }
        warn!("Initrd is not a valid SFS image, trying block device...");
    }

    if let Some(block) = hal_impl::device_registry::all_block().first() {
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

// ── Initrd support (bare-metal only) ──────────────────────────────────

#[cfg(feature = "linux")]
pub(crate) fn init_ram_disk() -> Option<&'static mut [u8]> {
    if hal_impl::platform::is_hosted() {
        return None;
    }
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
        hal_impl::boot::init_ram_disk()
    }
}

// ── Device wrappers (bare-metal Zircon) ───────────────────────────────

/// Minimal rcore-fs Device wrapper for an in-memory buffer.
#[cfg(feature = "zircon")]
struct MemBufDevice(spin::Mutex<&'static mut [u8]>);

#[cfg(feature = "zircon")]
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
#[cfg(feature = "zircon")]
struct BlockDevice(alloc::sync::Arc<dyn hal_impl::device_registry::scheme::BlockScheme>);

#[cfg(feature = "zircon")]
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
        for (i, blk) in (start_blk..end_blk).enumerate() {
            self.0
                .read_block(blk, &mut tmp[i * BLK_SIZE..(i + 1) * BLK_SIZE])
                .map_err(|_| rcore_fs::dev::DevError)?;
        }
        tmp[skip..skip + buf.len()].copy_from_slice(buf);
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

// ── Embedded rootfs image (link-user-img feature) ─────────────────────

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
