//! File status
//!
//! - stat
//! - lstat
//! - fstat(at)

use super::*;
use linux_object::fs::vfs::{FileType, Metadata};

impl Syscall<'_> {
    /// Works exactly like the stat syscall, but if the file in question is a symbolic link,
    /// information on the link is returned rather than its target.
    /// - `path` – full path to file
    /// - `stat_ptr` – pointer to stat buffer
    pub fn sys_lstat(&self, path: UserInPtr<u8>, stat_ptr: UserOutPtr<Stat>) -> SysResult {
        self.sys_fstatat(
            FileDesc::CWD,
            path,
            stat_ptr,
            AtFlags::SYMLINK_NOFOLLOW.bits(),
        )
    }

    /// Works exactly like the stat syscall except a file descriptor (fd) is provided instead of a path.
    /// - `fd` – file descriptor
    /// - `stat_ptr` – pointer to stat buffer
    pub fn sys_fstat(&self, fd: FileDesc, mut stat_ptr: UserOutPtr<Stat>) -> SysResult {
        debug!("fstat: fd={:?}, stat_ptr={:?}", fd, stat_ptr);

        let meta = self.linux_process().get_file(fd)?.metadata()?;
        stat_ptr.write(meta.into())?;
        Ok(0)
    }

    /// get file status relative to a directory file descriptor
    pub fn sys_fstatat(
        &self,
        dirfd: FileDesc,
        path: UserInPtr<u8>,
        mut stat_ptr: UserOutPtr<Stat>,
        flags: usize,
    ) -> SysResult {
        let path = path.read_c_string()?;
        let flags = AtFlags::from_bits_truncate(flags);
        debug!(
            "fstatat: dirfd={:?}, path={:?}, stat_ptr={:?}, flags={:?}",
            dirfd, path, stat_ptr, flags
        );

        let follow = !flags.contains(AtFlags::SYMLINK_NOFOLLOW);
        let inode = self.linux_process().lookup_inode_at(dirfd, &path, follow)?;
        let stat = inode.metadata()?;
        stat_ptr.write(stat.into())?;
        Ok(0)
    }

    /// Returns information about a file in a structure named stat.
    /// - `path` – pointer to the name of the file
    /// - `stat_ptr` –  pointer to the structure to receive file information
    pub fn sys_stat(&self, path: UserInPtr<u8>, stat_ptr: UserOutPtr<Stat>) -> SysResult {
        self.sys_fstatat(FileDesc::CWD, path, stat_ptr, 0)
    }

    /// Extended file stat with mask-selectable fields.
    ///
    /// Returns a `struct statx` (256 bytes) with basic file information.
    /// The `mask` parameter selects which fields to fill; we fill all
    /// available fields regardless.
    pub fn sys_statx(
        &self,
        dirfd: FileDesc,
        path: UserInPtr<u8>,
        flags: i32,
        _mask: u32,
        mut statxbuf: UserOutPtr<u8>,
    ) -> SysResult {
        let path = path.read_c_string()?;
        debug!(
            "statx: dirfd={:?}, path={:?}, flags={:#x}",
            dirfd, path, flags
        );

        let follow = flags & 0x100 == 0; // AT_SYMLINK_NOFOLLOW = 0x100
        let inode = self.linux_process().lookup_inode_at(dirfd, &path, follow)?;
        let meta = inode.metadata()?;

        // Build statx struct (256 bytes, all zeros initially)
        let mut buf = [0u8; 256];
        // stx_mask: u32 at offset 0 — which fields are filled
        // STATX_BASIC_STATS = 0x07ff
        buf[0..4].copy_from_slice(&0x07ffu32.to_ne_bytes());
        // stx_blksize: u32 at offset 4
        buf[4..8].copy_from_slice(&(meta.blk_size as u32).to_ne_bytes());
        // stx_attributes: u64 at offset 8 — file attributes (none)
        // stx_nlink: u32 at offset 16
        buf[16..20].copy_from_slice(&(meta.nlinks as u32).to_ne_bytes());
        // stx_uid: u32 at offset 20
        buf[20..24].copy_from_slice(&(meta.uid as u32).to_ne_bytes());
        // stx_gid: u32 at offset 24
        buf[24..28].copy_from_slice(&(meta.gid as u32).to_ne_bytes());
        // stx_mode: u16 at offset 28 (file type + permissions)
        let file_type: u16 = match meta.type_ {
            rcore_fs::vfs::FileType::File => 0o100000,
            rcore_fs::vfs::FileType::Dir => 0o040000,
            rcore_fs::vfs::FileType::SymLink => 0o120000,
            rcore_fs::vfs::FileType::CharDevice => 0o020000,
            rcore_fs::vfs::FileType::BlockDevice => 0o060000,
            rcore_fs::vfs::FileType::NamedPipe => 0o010000,
            rcore_fs::vfs::FileType::Socket => 0o140000,
        };
        #[allow(clippy::unnecessary_cast)]
        let mode = file_type | (meta.mode as u16 & 0o7777);
        buf[28..30].copy_from_slice(&mode.to_ne_bytes());
        // stx_ino: u64 at offset 32
        buf[32..40].copy_from_slice(&(meta.inode as u64).to_ne_bytes());
        // stx_size: u64 at offset 40
        buf[40..48].copy_from_slice(&(meta.size as u64).to_ne_bytes());
        // stx_blocks: u64 at offset 48
        buf[48..56].copy_from_slice(&(meta.blocks as u64).to_ne_bytes());
        // stx_attributes_mask: u64 at offset 56
        // Timestamps (statx_timestamp: tv_sec i64 + tv_nsec u32 + pad u32 = 16 bytes each)
        // stx_atime at offset 64, stx_btime at offset 80, stx_ctime at offset 96, stx_mtime at offset 112
        // Timestamps — use allow for casts that may be identity on some platforms
        #[allow(clippy::unnecessary_cast)]
        {
            let atime_sec = (meta.atime.sec as i64).to_ne_bytes();
            let atime_nsec = (meta.atime.nsec as u32).to_ne_bytes();
            buf[64..72].copy_from_slice(&atime_sec);
            buf[72..76].copy_from_slice(&atime_nsec);
            let ctime_sec = (meta.ctime.sec as i64).to_ne_bytes();
            let ctime_nsec = (meta.ctime.nsec as u32).to_ne_bytes();
            buf[96..104].copy_from_slice(&ctime_sec);
            buf[104..108].copy_from_slice(&ctime_nsec);
            let mtime_sec = (meta.mtime.sec as i64).to_ne_bytes();
            let mtime_nsec = (meta.mtime.nsec as u32).to_ne_bytes();
            buf[112..120].copy_from_slice(&mtime_sec);
            buf[120..124].copy_from_slice(&mtime_nsec);
        }

        statxbuf.write_array(&buf)?;
        Ok(0)
    }
}

#[cfg(not(target_arch = "mips"))]
use linux_object::time::TimeSpec;

#[cfg(target_arch = "mips")]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct TimeSpec {
    pub sec: i32,
    pub nsec: i32,
}

#[cfg(target_arch = "mips")]
impl From<linux_object::fs::vfs::TimeSpec> for TimeSpec {
    fn from(t: TimeSpec) -> Self {
        TimeSpec {
            sec: t.sec as _,
            nsec: t.nsec as _,
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    dev: u64,
    /// inode number
    ino: u64,
    /// number of hard links
    nlink: u64,

    /// file type and mode
    mode: StatMode,
    /// user ID of owner
    uid: u32,
    /// group ID of owner
    gid: u32,
    /// padding
    _pad0: u32,
    /// device ID (if special file)
    rdev: u64,
    /// total size, in bytes
    size: u64,
    /// blocksize for filesystem I/O
    blksize: u64,
    /// number of 512B blocks allocated
    blocks: u64,

    /// last access time
    atime: TimeSpec,
    /// last modification time
    mtime: TimeSpec,
    /// last status change time
    ctime: TimeSpec,
}

#[cfg(target_arch = "mips")]
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    dev: u64,
    /// padding
    _pad0: u64,
    /// inode number
    ino: u64,
    /// file type and mode
    mode: StatMode,
    /// number of hard links
    nlink: u32,

    /// user ID of owner
    uid: u32,
    /// group ID of owner
    gid: u32,
    /// device ID (if special file)
    rdev: u64,
    /// padding
    _pad1: u64,
    /// total size, in bytes
    size: u64,

    /// last access time
    atime: TimeSpec,
    /// last modification time
    mtime: TimeSpec,
    /// last status change time
    ctime: TimeSpec,

    /// blocksize for filesystem I/O
    blksize: u32,
    /// padding
    _pad2: u32,
    /// number of 512B blocks allocated
    blocks: u64,
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "mips")))]
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    dev: u64,
    /// inode number
    ino: u64,
    /// file type and mode
    mode: StatMode,
    /// number of hard links
    nlink: u32,

    /// user ID of owner
    uid: u32,
    /// group ID of owner
    gid: u32,
    /// device ID (if special file)
    rdev: u64,
    /// padding
    _pad0: u64,
    /// total size, in bytes
    size: u64,
    /// blocksize for filesystem I/O
    blksize: u32,
    /// padding
    _pad1: u32,
    /// number of 512B blocks allocated
    blocks: u64,

    /// last access time
    atime: TimeSpec,
    /// last modification time
    mtime: TimeSpec,
    /// last status change time
    ctime: TimeSpec,
}

impl From<Metadata> for Stat {
    fn from(info: Metadata) -> Self {
        Stat {
            dev: info.dev as _,
            ino: info.inode as _,
            mode: StatMode::from_type_mode(info.type_, info.mode as _),
            nlink: info.nlinks as _,
            uid: info.uid as _,
            gid: info.gid as _,
            rdev: info.rdev as _,
            size: info.size as _,
            blksize: info.blk_size as _,
            blocks: info.blocks as _,
            atime: info.atime.into(),
            mtime: info.mtime.into(),
            ctime: info.ctime.into(),
            _pad0: 0,
            #[cfg(not(target_arch = "x86_64"))]
            _pad1: 0,
            #[cfg(target_arch = "mips")]
            _pad2: 0,
        }
    }
}

bitflags! {
    pub struct StatMode: u32 {
        /// Type
        const TYPE_MASK = 0o170_000;
        /// FIFO
        const FIFO  = 0o010_000;
        /// character device
        const CHAR  = 0o020_000;
        /// directory
        const DIR   = 0o040_000;
        /// block device
        const BLOCK = 0o060_000;
        /// ordinary regular file
        const FILE  = 0o100_000;
        /// symbolic link
        const LINK  = 0o120_000;
        /// socket
        const SOCKET = 0o140_000;

        /// Set-user-ID on execution.
        const SET_UID = 0o4_000;
        /// Set-group-ID on execution.
        const SET_GID = 0o2_000;

        /// Read, write, execute/search by owner.
        const OWNER_MASK = 0o700;
        /// Read permission, owner.
        const OWNER_READ = 0o400;
        /// Write permission, owner.
        const OWNER_WRITE = 0o200;
        /// Execute/search permission, owner.
        const OWNER_EXEC = 0o100;

        /// Read, write, execute/search by group.
        const GROUP_MASK = 0o70;
        /// Read permission, group.
        const GROUP_READ = 0o40;
        /// Write permission, group.
        const GROUP_WRITE = 0o20;
        /// Execute/search permission, group.
        const GROUP_EXEC = 0o10;

        /// Read, write, execute/search by others.
        const OTHER_MASK = 0o7;
        /// Read permission, others.
        const OTHER_READ = 0o4;
        /// Write permission, others.
        const OTHER_WRITE = 0o2;
        /// Execute/search permission, others.
        const OTHER_EXEC = 0o1;
    }
}

impl StatMode {
    fn from_type_mode(type_: FileType, mode: u16) -> Self {
        let type_ = match type_ {
            FileType::File => StatMode::FILE,
            FileType::Dir => StatMode::DIR,
            FileType::SymLink => StatMode::LINK,
            FileType::CharDevice => StatMode::CHAR,
            FileType::BlockDevice => StatMode::BLOCK,
            FileType::Socket => StatMode::SOCKET,
            FileType::NamedPipe => StatMode::FIFO,
        };
        let mode = StatMode::from_bits_truncate(mode as u32);
        type_ | mode
    }
}
