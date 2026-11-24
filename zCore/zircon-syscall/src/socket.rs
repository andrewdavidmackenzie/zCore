use {super::*, zircon_object::ipc::Socket, zircon_object::ipc::SocketFlags};

impl Syscall<'_> {
    /// Create a socket.
    ///
    /// Socket is a connected pair of bidirectional stream transports, that can move only data, and that have a maximum capacity.
    pub fn sys_socket_create(
        &self,
        options: u32,
        mut out0: UserOutPtr<HandleValue>,
        mut out1: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!("socket.create: options={:#x?}", options);
        self.enforce_basic_policy(PolicyCondition::NewSocket)?;
        let proc = self.thread.proc();
        let (end0, end1) = Socket::create(options)?;
        let handle0 = proc.add_handle(Handle::new(end0, Rights::DEFAULT_SOCKET));
        let handle1 = proc.add_handle(Handle::new(end1, Rights::DEFAULT_SOCKET));
        out0.write(handle0)?;
        out1.write(handle1)?;
        Ok(())
    }

    /// Write data to a socket.
    ///
    /// Attempts to write `count: usize` bytes to the socket specified by `handle_value`.
    pub fn sys_socket_write(
        &self,
        handle_value: HandleValue,
        options: u32,
        user_bytes: UserInPtr<u8>,
        count: usize,
        mut actual_count_ptr: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "socket.write: socket={:#x?}, options={:#x?}, buffer={:#x?}, size={:#x?}",
            handle_value, options, user_bytes, count,
        );
        if (count == 0 || !user_bytes.is_null()) && options == 0 {
            if count > u32::MAX as usize {
                return Err(ZxError::INVALID_ARGS);
            }
            self.check_user_buffer_read(user_bytes.as_addr(), count)?;
            let socket = self
                .thread
                .proc()
                .get_object_with_rights::<Socket>(handle_value, Rights::WRITE)?;
            // Read user data in page-sized chunks, writing each chunk
            // to the socket directly — no count-sized kernel allocation
            // (#471). For stream sockets, partial writes are natural.
            // For datagrams, each chunk that fits is written atomically;
            // if the buffer fills, the loop stops early (SHOULD_WAIT
            // semantics are preserved by the caller checking the count).
            let actual_count = user_bytes.for_each_chunk::<ZxError>(count, |chunk| {
                match socket.write(chunk) {
                    Ok(n) => Ok(n),
                    // Buffer full: stop reading user data.
                    Err(ZxError::SHOULD_WAIT) => Ok(0),
                    Err(e) => Err(e),
                }
            })?;
            actual_count_ptr.write_if_not_null(actual_count)?;
            Ok(())
        } else {
            Err(ZxError::INVALID_ARGS)
        }
    }

    /// Read data from a socket.
    pub fn sys_socket_read(
        &self,
        handle_value: HandleValue,
        options: u32,
        mut user_bytes: UserOutPtr<u8>,
        count: usize,
        mut actual_count_ptr: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "socket.read: socket={:#x?}, options={:#x?}, buffer={:#x?}, size={:#x?}",
            handle_value, options, user_bytes, count,
        );
        if count > u32::MAX as usize {
            return Err(ZxError::INVALID_ARGS);
        }
        if count > 0 && user_bytes.is_null() {
            return Err(ZxError::INVALID_ARGS);
        }
        let options = SocketFlags::from_bits(options).ok_or(ZxError::INVALID_ARGS)?;
        if !(options - SocketFlags::SOCKET_PEEK).is_empty() {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let socket = proc.get_object_with_rights::<Socket>(handle_value, Rights::READ)?;
        let peek = options.contains(SocketFlags::SOCKET_PEEK);
        if peek {
            // Peek returns the same data on each call, so chunked reads
            // would repeat it. Use the bounded copy-all (socket buffer
            // is capped at 256 KiB).
            let mut data = {
                let mut v = alloc::vec::Vec::new();
                v.try_reserve(count).map_err(|_| ZxError::INVALID_ARGS)?;
                v.resize(count, 0u8);
                v
            };
            let actual_count = socket.read(peek, &mut data)?;
            user_bytes.write_array(&data[..actual_count])?;
            actual_count_ptr.write_if_not_null(actual_count)?;
            Ok(())
        } else {
            // Consuming read: chunked is safe — each read consumes
            // different data. Write directly to user space with no
            // count-sized kernel allocation (#471).
            let mut first_err = None;
            let result = user_bytes.for_each_chunk_mut::<ZxError>(count, |chunk| {
                match socket.read(false, chunk) {
                    Ok(n) => Ok(n),
                    // Socket empty or peer closed: stop, remember why.
                    Err(e) => {
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                        Ok(0)
                    }
                }
            });
            match result {
                Ok(actual_count) => {
                    if actual_count == 0 {
                        // Nothing was read — return the original error.
                        return Err(first_err.unwrap_or(ZxError::SHOULD_WAIT));
                    }
                    actual_count_ptr.write_if_not_null(actual_count)?;
                    Ok(())
                }
                Err(_) => {
                    // Copy to user failed after the socket data was
                    // consumed — matches Fuchsia's consuming-read
                    // semantics. Return INVALID_ARGS.
                    Err(ZxError::INVALID_ARGS)
                }
            }
        }
    }

    /// Prevent future reading or writing on a socket.
    pub fn sys_socket_shutdown(&self, socket: HandleValue, options: u32) -> ZxResult {
        let options = SocketFlags::from_bits_truncate(options);
        info!(
            "socket.shutdown: socket={:#x?}, options={:#x?}",
            socket, options
        );
        let proc = self.thread.proc();
        let socket = proc.get_object_with_rights::<Socket>(socket, Rights::WRITE)?;
        let read = options.contains(SocketFlags::SHUTDOWN_READ);
        let write = options.contains(SocketFlags::SHUTDOWN_WRITE);
        socket.shutdown(read, write)?;
        Ok(())
    }

    /// Set the write disposition of a socket and/or its peer.
    pub fn sys_socket_set_disposition(
        &self,
        handle_value: HandleValue,
        disposition: u32,
        disposition_peer: u32,
    ) -> ZxResult {
        info!(
            "socket.set_disposition: handle={:#x}, disposition={}, peer={}",
            handle_value, disposition, disposition_peer
        );
        let proc = self.thread.proc();
        let socket = proc.get_object_with_rights::<Socket>(handle_value, Rights::MANAGE_SOCKET)?;
        socket.set_disposition(disposition, disposition_peer)
    }
}
