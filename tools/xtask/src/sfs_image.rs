//! SFS image building utilities.
//!
//! Inlined from `vfs-fuse/src/zip.rs` to eliminate the git dependency.
//! Only the `zip_dir` function is used by zCore (for building rootfs images).

use std::error::Error;
use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;
use zcore_fs::vfs::{FileType, INode};

const DEFAULT_MODE: u32 = 0o664;
const BUF_SIZE: usize = 0x1000;

/// Recursively copy a host directory tree into an vfs INode.
///
/// Each file is created in the VFS, resized to the source file's length,
/// and its contents are copied in 4 KiB chunks. Directories are created
/// recursively. Symlinks are created with their target path as content.
pub fn zip_dir(path: &Path, inode: Arc<dyn INode>) -> Result<(), Box<dyn Error>> {
    let dir = fs::read_dir(path)?;
    for entry in dir {
        let entry = entry?;
        let name_ = entry.file_name();
        let name = name_.to_str().unwrap();
        let type_ = entry.file_type()?;
        if type_.is_file() {
            let inode = inode.create(name, FileType::File, DEFAULT_MODE)?;
            let mut file = fs::File::open(entry.path())?;
            inode.resize(file.metadata()?.len() as usize)?;
            let mut buf = [0u8; BUF_SIZE];
            let mut offset = 0usize;
            let mut len = BUF_SIZE;
            while len == BUF_SIZE {
                len = file.read(&mut buf)?;
                inode.write_at(offset, &buf[..len])?;
                offset += len;
            }
        } else if type_.is_dir() {
            let inode = inode.create(name, FileType::Dir, DEFAULT_MODE)?;
            zip_dir(entry.path().as_path(), inode)?;
        } else if type_.is_symlink() {
            let target = fs::read_link(entry.path())?;
            let inode = inode.create(name, FileType::SymLink, DEFAULT_MODE)?;
            #[cfg(unix)]
            let data = target.as_os_str().as_bytes();
            #[cfg(windows)]
            let data = target.to_str().unwrap().as_bytes();
            inode.resize(data.len())?;
            inode.write_at(0, data)?;
        }
    }
    Ok(())
}
