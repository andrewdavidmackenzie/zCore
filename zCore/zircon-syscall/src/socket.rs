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
            // Zircon: bound the write at what the socket can accept —
            // stream writes cap at the buffer size (short write),
            // datagram writes larger than the buffer are rejected with
            // OUT_OF_RANGE before any copy (#471). The bytes go
            // straight from user pages into the mbuf nodes — no
            // intermediate kernel buffer (#901 step 2).
            let read_len = socket.bound_write_len(count)?;
            let actual_count = socket.write_from_user(user_bytes, read_len)?;
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
        user_bytes: UserOutPtr<u8>,
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
        // Zircon: the socket can never hold more than SOCKET_SIZE bytes,
        // so a larger read is pointless (#471). The bytes go straight
        // from the mbuf nodes to user pages — no intermediate kernel
        // buffer (#901 step 2). Fault semantics (datagram dropped,
        // stream prefix consumed) are handled inside read_to_user,
        // matching Fuchsia.
        let bounded_count = socket.bound_read_len(count);
        let peek = options.contains(SocketFlags::SOCKET_PEEK);
        let actual_count = socket.read_to_user(peek, user_bytes, bounded_count)?;
        actual_count_ptr.write_if_not_null(actual_count)?;
        Ok(())
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
