use crate::PROJECT_DIR;
use os_xtask_utils::{CommandExt, Qemu};
use std::{fs, path::Path};

impl super::LinuxRootfs {
    /// Generates the rootfs image.
    pub fn image(&self) {
        // Recursively build rootfs
        self.make(false);
        // Image path
        let inner = PROJECT_DIR.join("zCore");
        let image = inner.join(format!("{arch}.img", arch = self.0.name()));
        // Skip image creation if it already exists and is newer than the
        // rootfs directory. Recreating the image on every run triggers a
        // full kernel recompile because the image lives inside the zCore
        // package directory and cargo watches it for changes.
        if image.is_file() && !is_stale(&image, &self.path()) {
            return;
        }
        // Generate image
        fuse(self.path(), &image);
        // Add extra space for certain tests
        Qemu::img()
            .arg("resize")
            .args(["-f", "raw"])
            .arg(&image)
            .arg("+5M")
            .invoke();
    }
}

/// Check if `output` is older than any file in `input_dir`.
fn is_stale(output: &std::path::Path, input_dir: &std::path::Path) -> bool {
    let output_mtime = match output.metadata().and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return true,
    };
    fn any_newer(dir: &std::path::Path, than: std::time::SystemTime) -> bool {
        let entries = match dir.read_dir() {
            Ok(e) => e,
            Err(_) => return true,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if any_newer(&path, than) {
                    return true;
                }
            } else if let Ok(m) = path.metadata().and_then(|m| m.modified()) {
                if m > than {
                    return true;
                }
            }
        }
        false
    }
    any_newer(input_dir, output_mtime)
}

/// Creates a filesystem image.
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
