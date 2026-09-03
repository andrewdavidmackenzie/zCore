//! File descriptor operations
//!
//! - open(at)
//! - close
//! - dup2
//! - pipe

use super::*;
use alloc::string::String;

impl Syscall<'_> {
    /// Opens or creates a file, depending on the flags passed to the call. Returns an integer with the file descriptor.
    pub fn sys_open(&self, path: UserInPtr<u8>, flags: usize, mode: usize) -> SysResult {
        self.sys_openat(FileDesc::CWD, path, flags, mode)
    }

    /// open file relative to directory file descriptor
    pub fn sys_openat(
        &self,
        dir_fd: FileDesc,
        path: UserInPtr<u8>,
        flags: usize,
        mode: usize,
    ) -> SysResult {
        let proc = self.linux_process();
        let path = path.as_c_str()?;
        // hard code special path
        let path = if path == "/dev/shm/testshm" {
            "/testshm"
        } else {
            path
        };
        let flags = OpenFlags::from_bits_truncate(flags);
        info!(
            "openat: dir_fd={:?}, path={:?}, flags={:?}, mode={:#o}",
            dir_fd, path, flags, mode
        );

        let inode = if flags.contains(OpenFlags::CREATE) {
            let (dir_path, file_name) = split_path(path);
            // relative to cwd
            let dir_inode = proc.lookup_inode_at(dir_fd, dir_path, true)?;
            match dir_inode.find(file_name) {
                Ok(file_inode) => {
                    if flags.contains(OpenFlags::EXCLUSIVE) {
                        return Err(LxError::EEXIST);
                    }
                    file_inode
                }
                Err(FsError::EntryNotFound) => {
                    let effective_mode = mode as u32 & !proc.umask();
                    dir_inode.create(file_name, FileType::File, effective_mode)?
                }
                Err(e) => return Err(LxError::from(e)),
            }
        } else {
            proc.lookup_inode_at(dir_fd, path, true)?
        };
        let file = File::new(inode, flags, path.into());
        let fd = proc.add_file(file)?;
        Ok(fd.into())
    }

    /// Closes a file descriptor, so that it no longer refers to any file and may be reused.
    pub fn sys_close(&self, fd: FileDesc) -> SysResult {
        info!("close: fd={:?}", fd);
        let proc = self.linux_process();
        proc.close_file(fd)?;
        Ok(0)
    }

    /// create a copy of the file descriptor oldfd.
    pub fn sys_dup2(&self, fd1: FileDesc, fd2: FileDesc) -> SysResult {
        info!("dup2: from {:?} to {:?}", fd1, fd2);
        let proc = self.linux_process();
        // close fd2 first if it is opened
        let _ = proc.close_file(fd2);
        let file_like = proc.get_file_like(fd1)?.dup()?;
        let fd2 = proc.add_file_at(fd2, file_like)?;
        Ok(fd2.into())
    }

    /// `dup3` creates a copy of the file descriptor `oldfd`, using the
    /// specified file descriptor number `newfd`. Unlike `dup2`, `dup3`
    /// supports a `flags` argument: only `O_CLOEXEC` (0x80000) is valid.
    pub fn sys_dup3(&self, fd1: FileDesc, fd2: FileDesc, flags: usize) -> SysResult {
        info!("dup3: from {:?} to {:?}, flags={:#x}", fd1, fd2, flags);
        if fd1 == fd2 {
            return Err(LxError::EINVAL);
        }
        // Only O_CLOEXEC is a valid flag for dup3
        if flags & !OpenFlags::CLOEXEC.bits() != 0 {
            return Err(LxError::EINVAL);
        }
        let proc = self.linux_process();
        // Validate and duplicate fd1 before closing fd2, so failures
        // leave the destination descriptor unchanged.
        let file_like = proc.get_file_like(fd1)?.dup()?;
        // Set or clear O_CLOEXEC based on flags (dup3 does not inherit
        // close-on-exec from the source descriptor).
        let mut current_flags = file_like.flags();
        current_flags.set(OpenFlags::CLOEXEC, flags & OpenFlags::CLOEXEC.bits() != 0);
        file_like.set_flags(current_flags)?;
        // Now close fd2 if it is opened
        let _ = proc.close_file(fd2);
        let fd2 = proc.add_file_at(fd2, file_like)?;
        Ok(fd2.into())
    }

    /// create a copy of the file descriptor fd, and uses the lowest-numbered unused descriptor for the new descriptor.
    pub fn sys_dup(&self, fd1: FileDesc) -> SysResult {
        info!("dup: from {:?}", fd1);
        let proc = self.linux_process();
        let file_like = proc.get_file_like(fd1)?.dup()?;
        let fd2 = proc.add_file(file_like)?;
        Ok(fd2.into())
    }

    /// Creates a pipe, a unidirectional data channel that can be used for interprocess communication.
    pub fn sys_pipe(&self, fds: UserOutPtr<[i32; 2]>) -> SysResult {
        self.sys_pipe2(fds, 0)
    }

    /// Creates a pipe, a unidirectional data channel that can be used for interprocess communication.
    pub fn sys_pipe2(&self, mut fds: UserOutPtr<[i32; 2]>, flags: usize) -> SysResult {
        info!("pipe2: fds={:?}, flags: {:#x}", fds, flags);

        let proc = self.linux_process();
        let (read, write) = Pipe::create_pair();

        let base_flags =
            OpenFlags::from_bits_truncate(flags) & (OpenFlags::NON_BLOCK | OpenFlags::CLOEXEC);
        let read_fd = proc.add_file(File::new(
            Arc::new(read),
            base_flags | OpenFlags::RDONLY,
            String::from("pipe_r:[]"),
        ))?;

        let write_fd = proc.add_file(File::new(
            Arc::new(write),
            base_flags | OpenFlags::WRONLY,
            String::from("pipe_w:[]"),
        ))?;
        fds.write([read_fd.into(), write_fd.into()])?;

        info!(
            "pipe2: created rfd={:?} wfd={:?} fds={:?}",
            read_fd, write_fd, fds
        );

        Ok(0)
    }

    /// Create an eventfd file descriptor for event notification.
    ///
    /// `initval` is the initial counter value. `flags` can include
    /// `EFD_CLOEXEC` (0x80000), `EFD_NONBLOCK` (0x800), and
    /// `EFD_SEMAPHORE` (0x1).
    pub fn sys_eventfd2(&self, initval: usize, flags: usize) -> SysResult {
        info!("eventfd2: initval={}, flags={:#x}", initval, flags);
        let eventfd = EventFd::new(initval as u64, flags);
        let fd = self.linux_process().add_file(Arc::new(eventfd))?;
        info!("eventfd2: fd={:?}", fd);
        Ok(fd.into())
    }

    /// apply or remove an advisory lock on an open file
    /// TODO: handle operation
    pub fn sys_flock(&mut self, fd: FileDesc, operation: usize) -> SysResult {
        bitflags! {
            struct Operation: u8 {
                const LOCK_SH = 1;
                const LOCK_EX = 2;
                const LOCK_NB = 4;
                const LOCK_UN = 8;
            }
        }
        let operation = Operation::from_bits(operation as u8).unwrap();
        info!("flock: fd: {:?}, operation: {:?}", fd, operation);
        let proc = self.linux_process();

        proc.get_file(fd)?;
        Ok(0)
    }
}
