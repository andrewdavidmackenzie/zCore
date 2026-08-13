use crate::PROJECT_DIR;
use os_xtask_utils::{CommandExt, Qemu};
use std::{fs, path::Path};

impl super::LinuxRootfs {
    /// 生成镜像。
    pub fn image(&self) {
        // 递归 rootfs
        self.make(false);
        // 镜像路径
        let inner = PROJECT_DIR.join("zCore");
        let image = inner.join(format!("{arch}.img", arch = self.0.name()));
        // Note: rayboot UEFI firmware download removed — no longer needed
        // for direct kernel boot. See issue #2.
        // 生成镜像
        fuse(self.path(), &image);
        // 扩充一些额外空间，供某些测试使用
        Qemu::img()
            .arg("resize")
            .args(&["-f", "raw"])
            .arg(image)
            .arg("+5M")
            .invoke();
    }
}

/// 制作镜像。
fn fuse(dir: impl AsRef<Path>, image: impl AsRef<Path>) {
    use rcore_fs::vfs::FileSystem;
    use rcore_fs_fuse::zip::zip_dir;
    use rcore_fs_sfs::SimpleFileSystem;
    use std::sync::{Arc, Mutex};

    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(image)
        .expect("failed to open image");
    const MAX_SPACE: usize = 1024 * 1024 * 1024; // 1GiB
    let fs = SimpleFileSystem::create(Arc::new(Mutex::new(file)), MAX_SPACE)
        .expect("failed to create sfs");
    zip_dir(dir.as_ref(), fs.root_inode()).expect("failed to zip fs");
}
