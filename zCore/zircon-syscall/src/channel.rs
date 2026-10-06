use {
    super::*,
    alloc::vec::Vec,
    zircon_object::{
        ipc::{Channel, MessagePacket},
        object::{obj_type, HandleInfo},
        task::ThreadState,
    },
};

impl Syscall<'_> {
    #[allow(clippy::too_many_arguments)]
    /// Read/Receive a message from a channel.
    pub fn sys_channel_read(
        &self,
        handle_value: HandleValue,
        options: u32,
        mut bytes: UserOutPtr<u8>,
        handles: usize,
        num_bytes: u32,
        num_handles: u32,
        mut actual_bytes: UserOutPtr<u32>,
        mut actual_handles: UserOutPtr<u32>,
        is_etc: bool,
    ) -> ZxResult {
        info!(
            "channel.read: handle={:#x?}, options={:?}, bytes=({:#x?}; {:#x?}), handles=({:#x?}; {:#x?})",
            handle_value, options, bytes, num_bytes, handles, num_handles,
        );
        const MAY_DISCARD: u32 = 1;
        if options & !MAY_DISCARD != 0 {
            return Err(ZxError::NOT_SUPPORTED);
        }
        let proc = self.thread.proc();
        let channel = proc.get_object_with_rights::<Channel>(handle_value, Rights::READ)?;
        let never_discard = options & MAY_DISCARD == 0;

        let msg = if never_discard {
            channel.check_and_read(|front_msg| {
                if num_bytes < front_msg.data.len() as u32
                    || num_handles < front_msg.handles.len() as u32
                {
                    actual_bytes.write_if_not_null(front_msg.data.len() as u32)?;
                    actual_handles.write_if_not_null(front_msg.handles.len() as u32)?;
                    Err(ZxError::BUFFER_TOO_SMALL)
                } else {
                    Ok(())
                }
            })?
        } else {
            channel.read()?
        };

        actual_bytes.write_if_not_null(msg.data.len() as u32)?;
        actual_handles.write_if_not_null(msg.handles.len() as u32)?;
        if num_bytes < msg.data.len() as u32 || num_handles < msg.handles.len() as u32 {
            return Err(ZxError::BUFFER_TOO_SMALL);
        }
        bytes.write_array(msg.data.as_slice())?;
        if is_etc {
            let mut handle_infos: Vec<HandleInfo> = msg
                .handles
                .iter()
                .map(|handle| handle.get_handle_info())
                .collect();
            let values = proc.add_handles(msg.handles);
            for (i, value) in values.iter().enumerate() {
                handle_infos[i].handle = *value;
            }
            UserOutPtr::<HandleInfo>::from(handles).write_array(&handle_infos)?;
        } else {
            let values = proc.add_handles(msg.handles);
            UserOutPtr::<HandleValue>::from(handles).write_array(&values)?;
        }
        Ok(())
    }
    /// Write a message to a channel.
    pub fn sys_channel_write(
        &self,
        handle_value: HandleValue,
        options: u32,
        user_bytes: UserInPtr<u8>,
        num_bytes: u32,
        user_handles: UserInPtr<HandleValue>,
        num_handles: u32,
    ) -> ZxResult {
        info!(
            "channel.write: handle_value={:#x}, options={:#x}, num_bytes={:#x}, num_handles={:#x}",
            handle_value, options, num_bytes, num_handles,
        );
        if options != 0 && options != ZX_CHANNEL_WRITE_USE_IOVEC {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let data = if options == ZX_CHANNEL_WRITE_USE_IOVEC {
            read_iovec_data(user_bytes, num_bytes)?
        } else {
            if num_bytes as usize > ZX_CHANNEL_MAX_MSG_BYTES {
                return Err(ZxError::OUT_OF_RANGE);
            }
            user_bytes.read_array(num_bytes as usize)?
        };
        let handle_values = user_handles.read_array(num_handles as usize)?;
        let transfer_self = handle_values.contains(&handle_value);
        // Look up the channel BEFORE consuming handles — if the handle
        // list includes the channel's own handle, remove_handles would
        // close it and the lookup would fail with BAD_HANDLE.
        let channel = proc.get_object_with_rights::<Channel>(handle_value, Rights::WRITE);
        // Consume handles immediately — Fuchsia guarantees handles are
        // always consumed by channel_write regardless of subsequent errors.
        let handles = if !handle_values.is_empty() {
            proc.remove_handles(&handle_values)?
        } else {
            alloc::vec::Vec::new()
        };
        if handles.len() > 64 {
            return Err(ZxError::OUT_OF_RANGE);
        }
        if transfer_self {
            return Err(ZxError::NOT_SUPPORTED);
        }
        let channel = channel?;
        for handle in handles.iter() {
            if !handle.rights.contains(Rights::TRANSFER) {
                return Err(ZxError::ACCESS_DENIED);
            }
        }
        channel.write(MessagePacket { data, handles })?;
        Ok(())
    }
    /// Create a new channel.
    pub fn sys_channel_create(
        &self,
        options: u32,
        mut out0: UserOutPtr<HandleValue>,
        mut out1: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!("channel.create: options={:#x}", options);
        if options != 0u32 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        proc.check_policy(PolicyCondition::NewChannel)?;
        let (end0, end1) = Channel::create();
        let handle0 = proc.add_handle(Handle::new(end0, Rights::DEFAULT_CHANNEL));
        let handle1 = proc.add_handle(Handle::new(end1, Rights::DEFAULT_CHANNEL));
        out0.write(handle0)?;
        out1.write(handle1)?;
        Ok(())
    }

    /// Send a message to a channel and await a reply.
    pub async fn sys_channel_call_noretry(
        &self,
        handle_value: HandleValue,
        options: u32,
        deadline: Deadline,
        user_args: UserInPtr<ChannelCallArgs>,
        mut actual_bytes: UserOutPtr<u32>,
        mut actual_handles: UserOutPtr<u32>,
    ) -> ZxResult {
        let mut args = user_args.read()?;
        info!(
            "channel.call_noretry: handle={:#x}, deadline={:?}, args={:#x?}",
            handle_value, deadline, args
        );
        let use_iovec = options == ZX_CHANNEL_WRITE_USE_IOVEC;
        if options != 0 && !use_iovec {
            return Err(ZxError::INVALID_ARGS);
        }
        if args.rd_num_bytes < 4 {
            return Err(ZxError::INVALID_ARGS);
        }
        if !use_iovec && args.wr_num_bytes < 4 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let channel =
            proc.get_object_with_rights::<Channel>(handle_value, Rights::READ | Rights::WRITE)?;
        let wr_data = if use_iovec {
            read_iovec_data(args.wr_bytes, args.wr_num_bytes)?
        } else {
            args.wr_bytes.read_array(args.wr_num_bytes as usize)?
        };
        // Channel call requires at least 4 bytes for the txid header
        if wr_data.len() < 4 {
            return Err(ZxError::INVALID_ARGS);
        }
        let wr_msg = MessagePacket {
            data: wr_data,
            handles: {
                let handles = args.wr_handles.read_array(args.wr_num_handles as usize)?;
                let handles = proc.remove_handles(&handles)?;
                for handle in handles.iter() {
                    if !handle.rights.contains(Rights::TRANSFER) {
                        return Err(ZxError::ACCESS_DENIED);
                    }
                }
                handles
            },
        };

        let cancel_token = proc.get_cancel_token(handle_value)?;
        let future = channel.call(wr_msg);
        pin_mut!(future);
        self.thread.set_blocking_state(ThreadState::BlockedChannel);
        let rd_msg: MessagePacket = self
            .thread
            .blocking_run(
                future,
                ThreadState::BlockedChannel,
                deadline.into(),
                Some(cancel_token),
            )
            .await?;

        actual_bytes.write(rd_msg.data.len() as u32)?;
        actual_handles.write(rd_msg.handles.len() as u32)?;
        if args.rd_num_bytes < rd_msg.data.len() as u32
            || args.rd_num_handles < rd_msg.handles.len() as u32
        {
            return Err(ZxError::BUFFER_TOO_SMALL);
        }
        args.rd_bytes.write_array(rd_msg.data.as_slice())?;
        args.rd_handles
            .write_array(&proc.add_handles(rd_msg.handles))?;
        Ok(())
    }

    pub fn sys_channel_call_finish(
        &self,
        deadline: Deadline,
        user_args: UserInPtr<ChannelCallArgs>,
        _actual_bytes: UserOutPtr<u32>,
        _actual_handles: UserOutPtr<u32>,
    ) -> ZxResult {
        let args = user_args.read()?;
        info!(
            "channel.call_finish: deadline={:?}, args={:#x?}",
            deadline, args
        );
        let thread_state = self.thread.state();
        if thread_state == ThreadState::BlockedChannel {
            // The thread is still waiting for a channel call reply.
            // zCore's blocking_run doesn't generate INTR_RETRY errors,
            // so this path shouldn't normally be reached. Return TIMED_OUT
            // to let the caller retry the full channel_call if needed.
            warn!("channel.call_finish: thread still in BlockedChannel, returning TIMED_OUT");
            Err(ZxError::TIMED_OUT)
        } else {
            // Thread is not in BlockedChannel state -- the original call
            // already completed (timed out or succeeded). Return BAD_STATE
            // per the Zircon spec.
            Err(ZxError::BAD_STATE)
        }
    }
    /// Write a message to a channel.
    pub fn sys_channel_write_etc(
        &self,
        handle: HandleValue,
        options: u32,
        user_bytes: UserInPtr<u8>,
        num_bytes: u32,
        mut user_handles: UserInOutPtr<HandleDisposition>,
        num_handles: u32,
    ) -> ZxResult {
        info!(
            "channel.write_etc: handle={:#x}, options={:#x}, user_bytes={:#x?}, num_bytes={:#x}, user_handles={:#x?}, num_handles={:#x}",
            handle, options, user_bytes, num_bytes, user_handles, num_handles
        );
        let use_iovec = options == ZX_CHANNEL_WRITE_USE_IOVEC;
        let proc = self.thread.proc();
        // Process dispositions FIRST (consuming MOVE handles) before
        // checking options or data, matching Fuchsia's behavior where
        // MOVE handles are always consumed regardless of other errors.
        let mut dispositions = user_handles.read_array(num_handles as usize)?;
        let mut handles: Vec<Handle> = Vec::new();
        let mut ret: ZxResult = Ok(());
        for disposition in dispositions.iter_mut() {
            if let Ok((object, src_rights)) = proc.get_dyn_object_and_rights(disposition.handle) {
                if let Err(e) = handle_check(disposition, &object, src_rights, handle) {
                    disposition.result = e as _;
                    if ret.is_ok() {
                        ret = Err(e);
                    }
                }
                let new_rights = if disposition.rights != Rights::SAME_RIGHTS.bits() {
                    Rights::from_bits(disposition.rights).unwrap()
                } else {
                    src_rights
                };
                let new_handle = Handle::new(object, new_rights);
                if disposition.op != ZX_HANDLE_OP_DUP {
                    proc.remove_handle(disposition.handle).unwrap();
                }
                handles.push(new_handle);
            } else {
                disposition.result = ZxError::BAD_HANDLE as _;
                if ret.is_ok() {
                    ret = Err(ZxError::BAD_HANDLE);
                }
            }
        }
        user_handles.write_array(&dispositions)?;
        // Check options after processing dispositions.
        if options != 0 && !use_iovec {
            return Err(ZxError::INVALID_ARGS);
        }
        let data = if use_iovec {
            read_iovec_data(user_bytes, num_bytes)?
        } else {
            if num_bytes as usize > ZX_CHANNEL_MAX_MSG_BYTES {
                return Err(ZxError::OUT_OF_RANGE);
            }
            user_bytes.read_array(num_bytes as usize)?
        };
        if num_handles > 64 || data.len() > ZX_CHANNEL_MAX_MSG_BYTES {
            return Err(ZxError::OUT_OF_RANGE);
        }
        ret?;
        let channel = proc.get_object_with_rights::<Channel>(handle, Rights::WRITE)?;
        channel.write(MessagePacket { data, handles })?;
        Ok(())
    }

    /// Send a message to a channel and await a reply (extended version).
    ///
    /// Like `channel_call_noretry` but uses handle dispositions for writes
    /// and returns `HandleInfo` (handle + type + rights) for reads.
    pub async fn sys_channel_call_etc_noretry(
        &self,
        handle_value: HandleValue,
        options: u32,
        deadline: Deadline,
        user_args: UserInPtr<ChannelCallEtcArgs>,
        mut actual_bytes: UserOutPtr<u32>,
        mut actual_handles: UserOutPtr<u32>,
    ) -> ZxResult {
        let mut args = user_args.read()?;
        info!(
            "channel.call_etc_noretry: handle={:#x}, options={:#x}, deadline={:?}",
            handle_value, options, deadline
        );
        let use_iovec = options == ZX_CHANNEL_WRITE_USE_IOVEC;
        if options != 0 && !use_iovec {
            return Err(ZxError::INVALID_ARGS);
        }
        // Validate write-side args first (including iovec limits) before
        // checking read-side args, so that OUT_OF_RANGE from iovec
        // validation is returned before INVALID_ARGS from rd_num_bytes.
        if !use_iovec && args.wr_num_bytes < 4 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let channel =
            proc.get_object_with_rights::<Channel>(handle_value, Rights::READ | Rights::WRITE)?;
        let data = if use_iovec {
            read_iovec_data(args.wr_bytes, args.wr_num_bytes)?
        } else {
            args.wr_bytes.read_array(args.wr_num_bytes as usize)?
        };
        if data.len() < 4 {
            return Err(ZxError::INVALID_ARGS);
        }
        if args.rd_num_bytes < 4 {
            return Err(ZxError::INVALID_ARGS);
        }
        let handles = if args.wr_num_handles > 0 {
            // Process dispositions with per-handle result tracking,
            // matching the Fuchsia ABI: each disposition's result field
            // is set to OK or the specific error, and the entire array
            // is written back to userspace.
            let mut dispositions = args.wr_handles.read_array(args.wr_num_handles as usize)?;
            let mut handles: Vec<Handle> = Vec::new();
            let mut first_err: ZxResult = Ok(());
            for disposition in dispositions.iter_mut() {
                if let Ok((object, src_rights)) = proc.get_dyn_object_and_rights(disposition.handle)
                {
                    if let Err(e) = handle_check(disposition, &object, src_rights, handle_value) {
                        disposition.result = e as _;
                        if first_err.is_ok() {
                            first_err = Err(e);
                        }
                    }
                    let new_rights = if disposition.rights != Rights::SAME_RIGHTS.bits() {
                        match Rights::from_bits(disposition.rights) {
                            Some(r) => r,
                            None => {
                                disposition.result = ZxError::INVALID_ARGS as _;
                                if first_err.is_ok() {
                                    first_err = Err(ZxError::INVALID_ARGS);
                                }
                                src_rights
                            }
                        }
                    } else {
                        src_rights
                    };
                    let new_handle = Handle::new(object, new_rights);
                    if disposition.op != ZX_HANDLE_OP_DUP {
                        proc.remove_handle(disposition.handle).ok();
                    }
                    handles.push(new_handle);
                } else {
                    disposition.result = ZxError::BAD_HANDLE as _;
                    if first_err.is_ok() {
                        first_err = Err(ZxError::BAD_HANDLE);
                    }
                }
            }
            args.wr_handles.write_array(&dispositions)?;
            first_err?;
            handles
        } else {
            Vec::new()
        };
        let wr_msg = MessagePacket { data, handles };
        let cancel_token = proc.get_cancel_token(handle_value)?;
        let future = channel.call(wr_msg);
        pin_mut!(future);
        self.thread.set_blocking_state(ThreadState::BlockedChannel);
        let rd_msg: MessagePacket = self
            .thread
            .blocking_run(
                future,
                ThreadState::BlockedChannel,
                deadline.into(),
                Some(cancel_token),
            )
            .await?;
        actual_bytes.write(rd_msg.data.len() as u32)?;
        actual_handles.write(rd_msg.handles.len() as u32)?;
        if args.rd_num_bytes < rd_msg.data.len() as u32
            || args.rd_num_handles < rd_msg.handles.len() as u32
        {
            return Err(ZxError::BUFFER_TOO_SMALL);
        }
        args.rd_bytes.write_array(rd_msg.data.as_slice())?;
        let handle_infos: Vec<HandleInfo> = rd_msg
            .handles
            .into_iter()
            .map(|h| {
                let mut info = h.get_handle_info();
                info.handle = proc.add_handle(h);
                info
            })
            .collect();
        args.rd_handles.write_array(&handle_infos)?;
        Ok(())
    }

    /// Finish a channel call (extended version).
    pub fn sys_channel_call_etc_finish(
        &self,
        deadline: Deadline,
        _user_args: UserInPtr<ChannelCallEtcArgs>,
        _actual_bytes: UserOutPtr<u32>,
        _actual_handles: UserOutPtr<u32>,
    ) -> ZxResult {
        info!("channel.call_etc_finish: deadline={:?}", deadline);
        let thread_state = self.thread.state();
        if thread_state == ThreadState::BlockedChannel {
            warn!("channel.call_etc_finish: thread still in BlockedChannel, returning TIMED_OUT");
            Err(ZxError::TIMED_OUT)
        } else {
            Err(ZxError::BAD_STATE)
        }
    }
}

fn handle_check(
    disposition: &HandleDisposition,
    object: &Arc<dyn KernelObject>,
    src_rights: Rights,
    handle_value: HandleValue,
) -> ZxResult {
    if !src_rights.contains(Rights::TRANSFER) {
        Err(ZxError::ACCESS_DENIED)
    } else if disposition.handle == handle_value {
        Err(ZxError::NOT_SUPPORTED)
    } else if disposition.type_ != 0 && disposition.type_ != obj_type(object) {
        Err(ZxError::WRONG_TYPE)
    } else if disposition.op != ZX_HANDLE_OP_MOVE && disposition.op != ZX_HANDLE_OP_DUP
        || disposition.rights != Rights::SAME_RIGHTS.bits()
            && (!src_rights.bits() & disposition.rights) != 0
    {
        Err(ZxError::INVALID_ARGS)
    } else if disposition.op == ZX_HANDLE_OP_DUP && !src_rights.contains(Rights::DUPLICATE) {
        Err(ZxError::ACCESS_DENIED)
    } else {
        Ok(())
    }
}

/// Read data from an iovec array. `ptr` points to the iovec array,
/// `count` is the number of iovecs.
/// Read data from an iovec array. `ptr` points to the iovec array,
/// `count` is the number of iovecs.
fn read_iovec_data(ptr: UserInPtr<u8>, count: u32) -> ZxResult<Vec<u8>> {
    if count as usize > ZX_CHANNEL_MAX_MSG_IOVECS {
        return Err(ZxError::OUT_OF_RANGE);
    }
    let iovecs_ptr: UserInPtr<ChannelIovec> = ptr.as_addr().into();
    let iovecs = iovecs_ptr.read_array(count as usize)?;
    let total: usize = iovecs.iter().map(|v| v.capacity as usize).sum();
    if total > ZX_CHANNEL_MAX_MSG_BYTES {
        return Err(ZxError::OUT_OF_RANGE);
    }
    let mut data = Vec::with_capacity(total);
    for iov in iovecs.iter() {
        if iov.reserved != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        if iov.capacity > 0 {
            let p: UserInPtr<u8> = iov.buffer.into();
            data.extend_from_slice(&p.read_array(iov.capacity as usize)?);
        }
    }
    Ok(data)
}

const ZX_HANDLE_OP_MOVE: u32 = 0;
const ZX_HANDLE_OP_DUP: u32 = 1;
const ZX_CHANNEL_WRITE_USE_IOVEC: u32 = 2;
const ZX_CHANNEL_MAX_MSG_BYTES: usize = 65536;
const ZX_CHANNEL_MAX_MSG_IOVECS: usize = 8192;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct ChannelIovec {
    buffer: usize,
    capacity: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct ChannelCallArgs {
    wr_bytes: UserInPtr<u8>,
    wr_handles: UserInPtr<HandleValue>,
    rd_bytes: UserOutPtr<u8>,
    rd_handles: UserOutPtr<HandleValue>,
    wr_num_bytes: u32,
    wr_num_handles: u32,
    rd_num_bytes: u32,
    rd_num_handles: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct ChannelCallEtcArgs {
    wr_bytes: UserInPtr<u8>,
    wr_handles: UserInOutPtr<HandleDisposition>,
    rd_bytes: UserOutPtr<u8>,
    rd_handles: UserOutPtr<HandleInfo>,
    wr_num_bytes: u32,
    wr_num_handles: u32,
    rd_num_bytes: u32,
    rd_num_handles: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct HandleDisposition {
    op: u32,
    handle: HandleValue,
    type_: u32,
    rights: u32,
    result: i32,
}
