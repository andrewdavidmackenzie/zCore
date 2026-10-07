//! Root filesystem access.
//!
//! Provides a unified rootfs interface for the kernel. On bare-metal,
//! the rootfs is an SFS image loaded from initrd or a block device.
//! In libOS mode, the rootfs is a host directory via HostFS.

/// Try to open a rootfs.
///
/// Returns `None` if no rootfs is available (no initrd, no block device,
/// and no host directory). Tries initrd first, then block device.
///
/// In libOS mode, uses HostFS backed by a host directory.
pub fn try_rootfs() -> Option<alloc::sync::Arc<dyn rcore_fs::vfs::FileSystem>> {
    // LibOS mode: use HostFS from the rootfs directory on the host.
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
        info!("Trying rootfs from initrd...");
        let dev = Arc::new(MemBufDevice(spin::Mutex::new(initrd)));
        if let Ok(fs) = SimpleFileSystem::open(dev) {
            let fs: Arc<dyn FileSystem> = fs;
            return Some(fs);
        }
        warn!("Initrd is not a valid SFS image, trying block device...");
    }

    if let Some(block) = hal_impl::device_registry::all_block().first() {
        info!("Trying rootfs from block device...");
        let dev: Arc<dyn rcore_fs::dev::Device> = Arc::new(BlockDevice(block));
        if let Ok(fs) = SimpleFileSystem::open(dev) {
            let fs: Arc<dyn FileSystem> = fs;
            return Some(fs);
        }
        warn!("Block device is not a valid SFS image");
    }

    None
}

/// Read a file from the rootfs by path.
///
/// Registered at boot via [`zircon_object::task::spawn::set_rootfs_reader`]
/// so that zircon-syscall can read ELF binaries and shared libraries
/// for process creation and dynamic linking.
pub fn read_rootfs_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    let rootfs = try_rootfs()?;
    let inode = rootfs.root_inode().lookup(path).ok()?;
    let meta = inode.metadata().ok()?;
    let mut data = alloc::vec![0u8; meta.size];
    inode.read_at(0, &mut data).ok()?;
    Some(data)
}

// ── Device wrappers ──────────────────────────────────────────────────

/// In-memory device backed by a static byte slice (used for initrd).
struct MemBufDevice(spin::Mutex<&'static mut [u8]>);

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

/// Block device adapter from [`hal_impl::device_registry::scheme::BlockScheme`]
/// to [`rcore_fs::dev::Device`].
struct BlockDevice(alloc::sync::Arc<dyn hal_impl::device_registry::scheme::BlockScheme>);

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
