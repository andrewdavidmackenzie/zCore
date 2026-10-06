use {
    super::*,
    alloc::vec::Vec,
    core::convert::TryFrom,
    hal::UserContextField,
    numeric_enum_macro::numeric_enum,
    zircon_object::{dev::*, ipc::*, signal::Clock, signal::Port, task::*, vm::*},
};

/// Check if an x86_64 address is canonical (bits 48..63 are copies of bit 47).
#[cfg(target_arch = "x86_64")]
fn is_canonical(addr: usize) -> bool {
    // Sign-extend bit 47 to bits 48..63
    let canonical = ((addr as i64) << 16 >> 16) as usize;
    addr == canonical
}

impl Syscall<'_> {
    /// Ask for various properties of various kernel objects.
    ///
    /// `handle_value: HandleValue`, indicates the target kernel object.
    /// `property: u32`, indicates which property to get/set.
    /// `buffer: usize`, holds the property value, and must be a pointer to a buffer of value_size bytes.
    pub fn sys_object_get_property(
        &self,
        handle_value: HandleValue,
        property: u32,
        buffer: usize,
        buffer_size: usize,
    ) -> ZxResult {
        // Fuchsia rejects null value pointers before handle/rights checks.
        if buffer == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let property = Property::try_from(property).map_err(|_| ZxError::INVALID_ARGS)?;
        info!(
            "object.get_property: handle={:#x?}, property={:?}, buffer=({:#x}; {:#x?})",
            handle_value, property, buffer, buffer_size
        );
        let proc = self.thread.proc();
        // ProcessVdsoBaseAddress: try the handle as a Process first (Fuchsia
        // requires a Process handle). If the handle is not a Process, fall
        // back to the calling process's own VMAR. This allows petal programs
        // (which don't have a Process self-handle) to query their own vDSO base.
        if matches!(property, Property::ProcessVdsoBaseAddress) {
            let mut info_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
            let vdso_base = if let Ok(target) =
                proc.get_object_with_rights::<Process>(handle_value, Rights::INSPECT)
            {
                target.vmar().vdso_base_addr().unwrap_or(0)
            } else {
                proc.vmar().vdso_base_addr().unwrap_or(0)
            };
            info_ptr.write(vdso_base)?;
            return Ok(());
        }
        let object = self.get_object_with_pseudo(handle_value, Rights::GET_PROPERTY)?;
        match property {
            Property::Name => {
                if !object.supports_name() {
                    return Err(ZxError::WRONG_TYPE);
                }
                if buffer_size < MAX_NAME_LEN {
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let s = object.name();
                // Write the full buffer zero-padded. Fuchsia guarantees
                // all bytes after the name are zero.
                let mut buf = [0u8; MAX_NAME_LEN];
                let name_bytes = s.as_bytes();
                let copy_len = name_bytes.len().min(MAX_NAME_LEN - 1);
                buf[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
                UserOutPtr::<u8>::from(buffer).write_array(&buf)?;
                Ok(())
            }
            Property::ProcessDebugAddr => {
                let mut info_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
                let debug_addr = proc
                    .get_object_with_rights::<Process>(handle_value, Rights::GET_PROPERTY)?
                    .get_debug_addr();
                info_ptr.write(debug_addr)?;
                Ok(())
            }
            // ProcessVdsoBaseAddress is handled above (before rights check)
            Property::ProcessVdsoBaseAddress => unreachable!(),
            Property::ProcessBreakOnLoad => {
                let mut info_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
                let break_on_load = proc
                    .get_object_with_rights::<Process>(handle_value, Rights::GET_PROPERTY)?
                    .get_dyn_break_on_load();
                info_ptr.write(break_on_load)?;
                Ok(())
            }
            Property::JobKillOnOom => {
                let mut info_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
                let value = proc
                    .get_object_with_rights::<Job>(handle_value, Rights::GET_PROPERTY)?
                    .get_kill_on_oom();
                info_ptr.write(value as usize)?;
                Ok(())
            }
            Property::SocketRxThreshold => {
                let mut info_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
                let rx = proc
                    .get_object_with_rights::<Socket>(handle_value, Rights::GET_PROPERTY)?
                    .get_rx_tx_threshold()
                    .0;
                info_ptr.write(rx)?;
                Ok(())
            }
            Property::SocketTxThreshold => {
                let mut info_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
                let tx = proc
                    .get_object_with_rights::<Socket>(handle_value, Rights::GET_PROPERTY)?
                    .get_rx_tx_threshold()
                    .1;
                info_ptr.write(tx)?;
                Ok(())
            }
            Property::VmoContentSize => {
                let mut info_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
                let content_size = proc
                    .get_object_with_rights::<VmObject>(handle_value, Rights::GET_PROPERTY)?
                    .content_size();
                info_ptr.write(content_size)?;
                Ok(())
            }
            Property::ExceptionState => {
                let mut info_ptr = UserOutPtr::<u32>::from_addr_size(buffer, buffer_size)?;
                let state = proc
                    .get_object_with_rights::<ExceptionObject>(handle_value, Rights::GET_PROPERTY)?
                    .state();
                info_ptr.write(state)?;
                Ok(())
            }
            Property::ExceptionStrategy => {
                let mut info_ptr = UserOutPtr::<u32>::from_addr_size(buffer, buffer_size)?;
                let strategy = proc
                    .get_object_with_rights::<ExceptionObject>(handle_value, Rights::GET_PROPERTY)?
                    .strategy();
                info_ptr.write(strategy)?;
                Ok(())
            }
            Property::RegisterFs => {
                let mut info_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
                let thread =
                    proc.get_object_with_rights::<Thread>(handle_value, Rights::GET_PROPERTY)?;
                // FS/GS register properties only work on the calling thread.
                if thread.id() != self.thread.id() {
                    return Err(ZxError::ACCESS_DENIED);
                }
                let value = thread
                    .with_context(|ctx| ctx.get_field(UserContextField::ThreadPointer))
                    .map_err(|_| ZxError::BAD_STATE)?;
                info_ptr.write(value)?;
                Ok(())
            }
            Property::RegisterGs => {
                #[cfg(target_arch = "x86_64")]
                {
                    let mut info_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
                    let thread =
                        proc.get_object_with_rights::<Thread>(handle_value, Rights::GET_PROPERTY)?;
                    if thread.id() != self.thread.id() {
                        return Err(ZxError::ACCESS_DENIED);
                    }
                    let value = thread
                        .with_context(|ctx| ctx.general().gsbase)
                        .map_err(|_| ZxError::BAD_STATE)?;
                    info_ptr.write(value)?;
                    Ok(())
                }
                #[cfg(not(target_arch = "x86_64"))]
                Err(ZxError::NOT_SUPPORTED)
            }
            Property::StreamModeAppend => {
                let mut info_ptr = UserOutPtr::<u8>::from_addr_size(buffer, buffer_size)?;
                let stream = proc.get_object_with_rights::<zircon_object::vm::Stream>(
                    handle_value,
                    Rights::GET_PROPERTY,
                )?;
                info_ptr.write(stream.get_mode_append() as u8)?;
                Ok(())
            }
            Property::ProcessHwTraceContextId => {
                // HW tracing is not supported by zCore.
                Err(ZxError::NOT_SUPPORTED)
            }
        }
    }

    /// Set various properties of various kernel objects.
    pub fn sys_object_set_property(
        &mut self,
        handle_value: HandleValue,
        property: u32,
        buffer: usize,
        buffer_size: usize,
    ) -> ZxResult {
        // Fuchsia rejects null value pointers before handle/rights checks.
        if buffer == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let property = Property::try_from(property).map_err(|_| ZxError::INVALID_ARGS)?;
        info!(
            "object.set_property: handle={:#x?}, property={:?}, buffer=({:#x}; {:#x?})",
            handle_value, property, buffer, buffer_size
        );
        let proc = self.thread.proc();
        let object = self.get_object_with_pseudo(handle_value, Rights::SET_PROPERTY)?;
        match property {
            Property::Name => {
                if !object.supports_name() {
                    return Err(ZxError::WRONG_TYPE);
                }
                let length = buffer_size.min(MAX_NAME_LEN);
                let raw = UserInPtr::<u8>::from(buffer).read_array(length)?;
                // Truncate at first null byte — Fuchsia names are
                // null-terminated C strings, any bytes after null are ignored.
                let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                let name = core::str::from_utf8(&raw[..end]).map_err(|_| ZxError::INVALID_ARGS)?;
                object.set_name(name);
                Ok(())
            }
            Property::ProcessDebugAddr => {
                let addr = UserInPtr::<usize>::from_addr_size(buffer, buffer_size)?.read()?;
                proc.get_object_with_rights::<Process>(handle_value, Rights::SET_PROPERTY)?
                    .set_debug_addr(addr);
                Ok(())
            }
            #[cfg(target_arch = "x86_64")]
            Property::RegisterFs => {
                let thread = proc.get_object::<Thread>(handle_value)?;
                if thread.id() != self.thread.id() {
                    return Err(ZxError::ACCESS_DENIED);
                }
                let fsbase = UserInPtr::<usize>::from_addr_size(buffer, buffer_size)?.read()?;
                // Reject non-canonical addresses (x86_64: bits 48..63
                // must be copies of bit 47).
                if !is_canonical(fsbase) {
                    return Err(ZxError::INVALID_ARGS);
                }
                thread.with_context(|ctx| ctx.general_mut().fsbase = fsbase)?;
                Ok(())
            }
            #[cfg(target_arch = "x86_64")]
            Property::RegisterGs => {
                let thread = proc.get_object::<Thread>(handle_value)?;
                if thread.id() != self.thread.id() {
                    return Err(ZxError::ACCESS_DENIED);
                }
                let gsbase = UserInPtr::<usize>::from_addr_size(buffer, buffer_size)?.read()?;
                if !is_canonical(gsbase) {
                    return Err(ZxError::INVALID_ARGS);
                }
                thread.with_context(|ctx| ctx.general_mut().gsbase = gsbase)?;
                Ok(())
            }
            Property::ProcessBreakOnLoad => {
                let addr = UserInPtr::<usize>::from_addr_size(buffer, buffer_size)?.read()?;
                proc.get_object_with_rights::<Process>(handle_value, Rights::SET_PROPERTY)?
                    .set_dyn_break_on_load(addr);
                Ok(())
            }
            Property::JobKillOnOom => {
                let value = UserInPtr::<usize>::from_addr_size(buffer, buffer_size)?.read()?;
                // Only 0 (disable) and 1 (enable) are valid values.
                if value > 1 {
                    return Err(ZxError::INVALID_ARGS);
                }
                proc.get_object_with_rights::<Job>(handle_value, Rights::SET_PROPERTY)?
                    .set_kill_on_oom(value != 0);
                Ok(())
            }
            Property::SocketRxThreshold => {
                let threshold = UserInPtr::<usize>::from_addr_size(buffer, buffer_size)?.read()?;
                proc.get_object::<Socket>(handle_value)?
                    .set_read_threshold(threshold)
            }
            Property::SocketTxThreshold => {
                let threshold = UserInPtr::<usize>::from_addr_size(buffer, buffer_size)?.read()?;
                proc.get_object::<Socket>(handle_value)?
                    .set_write_threshold(threshold)
            }
            Property::VmoContentSize => {
                let content_size =
                    UserInPtr::<usize>::from_addr_size(buffer, buffer_size)?.read()?;
                // Fuchsia rejects content_size values > INT64_MAX because seek
                // offsets are signed.
                if content_size > isize::MAX as usize {
                    return Err(ZxError::OUT_OF_RANGE);
                }
                // Setting content_size requires WRITE right.
                proc.get_object_with_rights::<VmObject>(handle_value, Rights::WRITE)?
                    .set_content_size_with_zero(content_size)
            }
            Property::ExceptionState => {
                let state = UserInPtr::<u32>::from_addr_size(buffer, buffer_size)?.read()?;
                proc.get_object_with_rights::<ExceptionObject>(handle_value, Rights::SET_PROPERTY)?
                    .set_state(state)?;
                Ok(())
            }
            Property::ExceptionStrategy => {
                let strategy = UserInPtr::<u32>::from_addr_size(buffer, buffer_size)?.read()?;
                proc.get_object_with_rights::<ExceptionObject>(handle_value, Rights::SET_PROPERTY)?
                    .set_strategy(strategy)?;
                Ok(())
            }
            Property::StreamModeAppend => {
                let value = UserInPtr::<u8>::from_addr_size(buffer, buffer_size)?.read()?;
                let stream = proc.get_object_with_rights::<zircon_object::vm::Stream>(
                    handle_value,
                    Rights::SET_PROPERTY,
                )?;
                stream.set_mode_append(value != 0);
                Ok(())
            }
            Property::ProcessHwTraceContextId => {
                // HW tracing is not supported by zCore.
                Err(ZxError::NOT_SUPPORTED)
            }
            _ => {
                warn!("unknown property {:?}", property);
                Err(ZxError::INVALID_ARGS)
            }
        }
    }

    /// A blocking syscall waits for signals on an object.
    pub async fn sys_object_wait_one(
        &self,
        handle: HandleValue,
        signals: u32,
        deadline: Deadline,
        mut observed: UserOutPtr<Signal>,
    ) -> ZxResult {
        let signals = Signal::from_bits_truncate(signals);
        info!(
            "object.wait_one: handle={:#x?}, signals={:#x?}, deadline={:#x?}, observed={:#x?}",
            handle, signals, deadline, observed
        );
        let proc = self.thread.proc();
        let object = self.get_object_with_pseudo(handle, Rights::WAIT)?;
        let cancel_token = proc.get_cancel_token(handle)?;
        let future = object.wait_signal(signals);
        self.thread.set_blocking_state(ThreadState::BlockedWaitOne);
        let signal = self
            .thread
            .blocking_run(
                future,
                ThreadState::BlockedWaitOne,
                deadline.into(),
                Some(cancel_token),
            )
            .await
            .or_else(|e| {
                if e == ZxError::TIMED_OUT {
                    observed.write_if_not_null(object.signal())?;
                }
                Err(e)
            })?;
        observed.write_if_not_null(signal)?;
        Ok(())
    }

    /// Query information about an object.
    ///
    /// `topic: u32`, indicates what specific information is desired.
    /// `buffer: usize`, a pointer to a buffer of size buffer_size to return the information.
    pub fn sys_object_get_info(
        &self,
        handle: HandleValue,
        topic: u32,
        buffer: usize,
        buffer_size: usize,
        mut actual: UserOutPtr<usize>,
        mut avail: UserOutPtr<usize>,
    ) -> ZxResult {
        // Fuchsia info topics use low bits for the topic ID and upper bits
        // for version (bits 28+). Preserve the version before stripping.
        let topic_version = topic >> 28;
        let masked_topic = topic & 0xFFFF;
        let topic = Topic::try_from(masked_topic).map_err(|_| {
            warn!(
                "object.get_info: unknown topic {:#x} (masked {:#x})",
                topic, masked_topic
            );
            ZxError::INVALID_ARGS
        })?;
        info!(
            "object.get_info: handle={:#x?}, topic={:?}, buffer=({:#x}; {:#x})",
            handle, topic, buffer, buffer_size,
        );
        let proc = self.thread.proc();
        match topic {
            Topic::HandleValid => {
                let _ = self.get_object_with_pseudo(handle, Rights::empty())?;
            }
            Topic::Process => {
                // ZX_INFO_PROCESS = __ZX_INFO_TOPIC(3u, 1u) — only V1 (24-byte)
                // layout exists in Fuchsia. No V0 16-byte variant.
                let target = proc.get_object_with_rights::<Process>(handle, Rights::INSPECT)?;
                if buffer_size < core::mem::size_of::<ProcessInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr = UserOutPtr::<ProcessInfo>::from_addr_size(buffer, buffer_size)?;
                info_ptr.write(target.get_info())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::Vmar => {
                let vmar =
                    proc.get_object_with_rights::<VmAddressRegion>(handle, Rights::INSPECT)?;
                if buffer_size < core::mem::size_of::<VmarInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr = UserOutPtr::<VmarInfo>::from_addr_size(buffer, buffer_size)?;
                info_ptr.write(vmar.get_info())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::HandleBasic => {
                let info = proc.get_handle_info(handle)?;
                if buffer_size < core::mem::size_of::<HandleBasicInfo>() {
                    // Buffer too small — still write actual=0, avail=1 per Fuchsia ABI.
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr =
                    UserOutPtr::<HandleBasicInfo>::from_addr_size(buffer, buffer_size)?;
                info_ptr.write(info)?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::Thread => {
                let thread = proc.get_object_with_rights::<Thread>(handle, Rights::INSPECT)?;
                if buffer_size < core::mem::size_of::<ThreadInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr = UserOutPtr::<ThreadInfo>::from_addr_size(buffer, buffer_size)?;
                info_ptr.write(thread.get_thread_info())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::ThreadExceptionReport => {
                let mut info_ptr =
                    UserOutPtr::<ExceptionReport>::from_addr_size(buffer, buffer_size)?;
                let thread = proc.get_object_with_rights::<Thread>(handle, Rights::INSPECT)?;
                info_ptr.write(thread.get_thread_exception_info()?)?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::HandleCount => {
                let object = self.get_object_with_pseudo(handle, Rights::INSPECT)?;
                if buffer_size < core::mem::size_of::<u32>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr = UserOutPtr::<u32>::from_addr_size(buffer, buffer_size)?;
                info_ptr.write(object.handle_count())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::Job => {
                let job = proc.get_object_with_rights::<Job>(handle, Rights::INSPECT)?;
                if buffer_size < core::mem::size_of::<JobInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr = UserOutPtr::<JobInfo>::from_addr_size(buffer, buffer_size)?;
                info_ptr.write(job.get_info())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::HandleTable => {
                // Fuchsia requires MANAGE_PROCESS to enumerate handles.
                let target =
                    proc.get_object_with_rights::<Process>(handle, Rights::MANAGE_PROCESS)?;
                let raw_entries = target.get_handle_table_entries();
                let mut entries: Vec<HandleExtendedInfo> = raw_entries
                    .iter()
                    .map(
                        |&(obj_type, hv, rights, koid, related)| HandleExtendedInfo {
                            obj_type,
                            handle_value: hv,
                            rights,
                            reserved: 0,
                            koid,
                            related_koid: related,
                            peer_owner_koid: 0,
                        },
                    )
                    .collect();
                // Sort by handle value to match Fuchsia's handle table ordering.
                entries.sort_by_key(|e| e.handle_value);
                let entry_size = core::mem::size_of::<HandleExtendedInfo>();
                let count = (buffer_size / entry_size).min(entries.len());
                if count > 0 {
                    UserOutPtr::<HandleExtendedInfo>::from(buffer)
                        .write_array(&entries[..count])?;
                }
                actual.write_if_not_null(count)?;
                avail.write_if_not_null(entries.len())?;
            }
            Topic::ProcessVmos => {
                let target = proc.get_object_with_rights::<Process>(handle, Rights::INSPECT)?;
                // Collect VMO info from all handles that reference VMOs.
                let raw_entries = target.get_handle_table_entries();
                let mut vmo_infos: Vec<VmoInfo> = Vec::new();
                // Iterate process handles looking for VMO-typed objects.
                let inner_handles = target.get_handle_table_entries();
                for &(obj_type, hv, rights_bits, _koid, _related) in &inner_handles {
                    // obj_type 3 = VmObject
                    if obj_type == 3 {
                        if let Ok(vmo) = target.get_object::<VmObject>(hv) {
                            let mut info = vmo.get_info();
                            info.flags |= VmoInfoFlags::VIA_HANDLE;
                            info.rights |= Rights::from_bits_truncate(rights_bits);
                            vmo_infos.push(info);
                        }
                    }
                }
                drop(raw_entries);
                let entry_size = core::mem::size_of::<VmoInfo>();
                let count = (buffer_size / entry_size).min(vmo_infos.len());
                if count > 0 {
                    UserOutPtr::<VmoInfo>::from(buffer).write_array(&vmo_infos[..count])?;
                }
                actual.write_if_not_null(count)?;
                avail.write_if_not_null(vmo_infos.len())?;
            }
            Topic::Vmo => {
                let mut info_ptr = UserOutPtr::<VmoInfo>::from_addr_size(buffer, buffer_size)?;
                let (vmo, rights) = proc.get_object_and_rights::<VmObject>(handle)?;
                let mut info = vmo.get_info();
                info.flags |= VmoInfoFlags::VIA_HANDLE;
                info.rights |= rights;
                info_ptr.write(info)?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::KmemStats => {
                // Fuchsia requires a root resource or system-info resource.
                proc.get_resource(handle)?.validate(ResourceKind::ROOT)?;
                if buffer_size < core::mem::size_of::<KmemInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr = UserOutPtr::<KmemInfo>::from_addr_size(buffer, buffer_size)?;
                let kmem = KmemInfo {
                    vmo_bytes: vmo_page_bytes() as u64,
                    ..Default::default()
                };
                info_ptr.write(kmem)?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::KmemStatsExtended => {
                proc.get_resource(handle)?.validate(ResourceKind::ROOT)?;
                if buffer_size < core::mem::size_of::<KmemStatsExtendedInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr =
                    UserOutPtr::<KmemStatsExtendedInfo>::from_addr_size(buffer, buffer_size)?;
                info_ptr.write(KmemStatsExtendedInfo::default())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::ThreadStats => {
                let thread = proc.get_object_with_rights::<Thread>(handle, Rights::INSPECT)?;
                if buffer_size < core::mem::size_of::<ThreadStatsInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr =
                    UserOutPtr::<ThreadStatsInfo>::from_addr_size(buffer, buffer_size)?;
                // Report ZX_INFO_INVALID_CPU (0xFFFFFFFF) for threads that
                // have never been scheduled.
                let last_cpu = if thread.state() == ThreadState::New {
                    0xFFFF_FFFFu32
                } else {
                    0 // stub: report CPU 0 until real tracking is added
                };
                info_ptr.write(ThreadStatsInfo {
                    total_runtime: thread.get_time() as i64,
                    last_scheduled_cpu: last_cpu,
                    padding1: [0; 4],
                })?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::TaskStats => {
                let vmar = proc
                    .get_object_with_rights::<Process>(handle, Rights::INSPECT)?
                    .vmar();
                if buffer_size < core::mem::size_of::<TaskStatsInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr =
                    UserOutPtr::<TaskStatsInfo>::from_addr_size(buffer, buffer_size)?;
                let task_stats = vmar.get_task_stats();
                info_ptr.write(task_stats)?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::JobChildren | Topic::JobProcess | Topic::ProcessThreads => {
                let ids = match topic {
                    Topic::JobChildren => proc
                        .get_object_with_rights::<Job>(handle, Rights::ENUMERATE)?
                        .children_ids(),
                    Topic::JobProcess => proc
                        .get_object_with_rights::<Job>(handle, Rights::ENUMERATE)?
                        .process_ids(),
                    Topic::ProcessThreads => proc
                        .get_object_with_rights::<Process>(handle, Rights::ENUMERATE)?
                        .thread_ids(),
                    _ => unreachable!(),
                };
                let count = (buffer_size / core::mem::size_of::<KoID>()).min(ids.len());
                UserOutPtr::<KoID>::from(buffer).write_array(&ids[..count])?;
                actual.write_if_not_null(count)?;
                avail.write_if_not_null(ids.len())?;
            }
            Topic::Bti => {
                let mut info_ptr = UserOutPtr::<BtiInfo>::from_addr_size(buffer, buffer_size)?;
                let bti = proc
                    .get_object_with_rights::<BusTransactionInitiator>(handle, Rights::INSPECT)?;
                info_ptr.write(bti.get_info())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::Resource => {
                let mut info_ptr = UserOutPtr::<ResourceInfo>::from_addr_size(buffer, buffer_size)?;
                let resource = proc.get_resource_with_rights(handle, Rights::INSPECT)?;
                info_ptr.write(resource.get_info())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::Socket => {
                let mut info_ptr = UserOutPtr::<SocketInfo>::from_addr_size(buffer, buffer_size)?;
                let socket = proc.get_object_with_rights::<Socket>(handle, Rights::INSPECT)?;
                info_ptr.write(socket.get_info())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::Stream => {
                let mut info_ptr = UserOutPtr::<StreamInfo>::from_addr_size(buffer, buffer_size)?;
                let stream = proc.get_object_with_rights::<Stream>(handle, Rights::INSPECT)?;
                info_ptr.write(stream.get_info())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::TaskRuntime => {
                // ZX_INFO_TASK_RUNTIME — applies to Job, Process, and Thread.
                // Return a zeroed struct as a stub (no real CPU accounting yet).
                // First check the handle is a valid task type, then check rights.
                // This ensures we return ACCESS_DENIED (not WRONG_TYPE) when
                // the handle is a valid task but lacks INSPECT rights.
                let (_obj, rights) = proc.get_dyn_object_and_rights(handle)?;
                let type_name = _obj.type_name();
                if type_name != "Job" && type_name != "Process" && type_name != "Thread" {
                    return Err(ZxError::WRONG_TYPE);
                }
                if !rights.contains(Rights::INSPECT) {
                    return Err(ZxError::ACCESS_DENIED);
                }
                // Determine required record size from topic version:
                // V1 (version 0) = 16 bytes (cpu_time + queue_time)
                // V2 (version 1) = 32 bytes (+ page_fault_time + lock_contention_time)
                let info_size = match topic_version {
                    0 => 2 * core::mem::size_of::<i64>(),         // V1: 16 bytes
                    1 => core::mem::size_of::<TaskRuntimeInfo>(), // V2: 32 bytes
                    _ => return Err(ZxError::INVALID_ARGS),
                };
                if buffer_size < info_size {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                // Compute real CPU time. Use get_object_with_rights for the
                // correct typed handle lookup (downcast_arc on _obj was
                // returning stale values due to LTO optimization).
                drop(_obj); // release the dyn object
                let cpu_time: i64 = if let Ok(t) =
                    proc.get_object_with_rights::<Thread>(handle, Rights::INSPECT)
                {
                    t.get_time() as i64
                } else if let Ok(p) =
                    proc.get_object_with_rights::<Process>(handle, Rights::INSPECT)
                {
                    p.total_cpu_time() as i64
                } else if let Ok(j) = proc.get_object_with_rights::<Job>(handle, Rights::INSPECT) {
                    j.total_cpu_time() as i64
                } else {
                    0
                };
                // Compute queue time (time spent ready-but-not-running).
                let queue_time: i64 =
                    if let Ok(t) = proc.get_object_with_rights::<Thread>(handle, Rights::INSPECT) {
                        t.queue_time() as i64
                    } else {
                        0
                    };
                // Write cpu_time and queue_time as raw i64 values.
                let mut out = UserOutPtr::<i64>::from(buffer);
                out.write(cpu_time)?;
                // queue_time is at offset 8
                let mut out2 = UserOutPtr::<i64>::from(buffer + core::mem::size_of::<i64>());
                out2.write(queue_time)?;
                if info_size > 16 {
                    // V2: write page_fault_time and lock_contention_time (zeros)
                    let mut out3 =
                        UserOutPtr::<i64>::from(buffer + 2 * core::mem::size_of::<i64>());
                    out3.write(0)?;
                    let mut out4 =
                        UserOutPtr::<i64>::from(buffer + 3 * core::mem::size_of::<i64>());
                    out4.write(0)?;
                }
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::ClockMappedSize => {
                // Returns the size needed to map a clock's state VMO.
                // The clock state is a single page containing the
                // zx_clock_details_v1_t structure.
                let mut size_ptr = UserOutPtr::<usize>::from_addr_size(buffer, buffer_size)?;
                let _clock = proc.get_object_with_rights::<Clock>(handle, Rights::INSPECT)?;
                size_ptr.write(0x1000)?; // PAGE_SIZE
            }
            Topic::CpuStats => {
                // Requires a root or system-info resource handle.
                proc.get_resource(handle)?.validate(ResourceKind::ROOT)?;
                // Return one CPU stats record (single-CPU system).
                let entry = CpuStatsInfo {
                    flags: 1, // ZX_INFO_CPU_STATS_FLAG_ONLINE
                    ..Default::default()
                };
                let entry_size = core::mem::size_of::<CpuStatsInfo>();
                let count = (buffer_size / entry_size).min(1);
                if count > 0 {
                    UserOutPtr::<CpuStatsInfo>::from(buffer).write(entry)?;
                }
                actual.write_if_not_null(count)?;
                avail.write_if_not_null(1)?;
            }
            Topic::MemoryStall => {
                // Requires a root or system-stall resource handle.
                proc.get_resource(handle)?.validate(ResourceKind::ROOT)?;
                if buffer_size < core::mem::size_of::<MemoryStallInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let mut info_ptr =
                    UserOutPtr::<MemoryStallInfo>::from_addr_size(buffer, buffer_size)?;
                info_ptr.write(MemoryStallInfo::default())?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::GuestStats => {
                // Requires a root or system-info resource handle.
                proc.get_resource(handle)?.validate(ResourceKind::ROOT)?;
                // Return one record per CPU with zeroed guest counters.
                // GuestStats is per-CPU like CpuStats.
                let entry_size = core::mem::size_of::<GuestStatsInfo>();
                let count = (buffer_size / entry_size).min(1);
                if count > 0 {
                    UserOutPtr::<GuestStatsInfo>::from(buffer).write(GuestStatsInfo::default())?;
                }
                actual.write_if_not_null(count)?;
                avail.write_if_not_null(1)?;
            }
            Topic::ProcessHandleStats => {
                let target = proc.get_object_with_rights::<Process>(handle, Rights::INSPECT)?;
                if buffer_size < core::mem::size_of::<ProcessHandleStatsInfo>() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(1)?;
                    return Err(ZxError::BUFFER_TOO_SMALL);
                }
                let raw_entries = target.get_handle_table_entries();
                let mut stats = ProcessHandleStatsInfo::default();
                for &(obj_type, _, _, _, _) in &raw_entries {
                    if (obj_type as usize) < stats.handle_count.len() {
                        stats.handle_count[obj_type as usize] += 1;
                    }
                }
                let mut info_ptr = UserOutPtr::<ProcessHandleStatsInfo>::from(buffer);
                info_ptr.write(stats)?;
                actual.write_if_not_null(1)?;
                avail.write_if_not_null(1)?;
            }
            Topic::ProcessMaps => {
                let target = proc.get_object_with_rights::<Process>(handle, Rights::INSPECT)?;
                // Cannot inspect own maps (would deadlock on VMAR lock).
                if target.id() == proc.id() {
                    actual.write_if_not_null(0)?;
                    avail.write_if_not_null(0)?;
                    return Ok(());
                }
                let vmar = target.vmar();
                // Build the entries: aspace (depth 0) + VMAR tree (depth 1+).
                let mut entries = Vec::new();
                // Aspace entry (depth 0) — represents the full address space.
                let mut aspace_name = [0u8; 32];
                let pname = target.name();
                let pb = pname.as_bytes();
                let pcopy = pb.len().min(31);
                aspace_name[..pcopy].copy_from_slice(&pb[..pcopy]);
                entries.push(InfoMapsEntry {
                    name: aspace_name,
                    base: vmar.addr() as u64,
                    size: vmar.get_info().len as u64,
                    depth: 0,
                    r#type: 1, // ZX_INFO_MAPS_TYPE_ASPACE
                    padding: 0,
                    mapping: InfoMapsMapping::default(),
                });
                // VMAR tree walk (depth 1+).
                let vmar_entries = vmar.get_info_maps(1)?;
                entries.extend(vmar_entries);
                let entry_size = core::mem::size_of::<InfoMapsEntry>();
                let count = (buffer_size / entry_size).min(entries.len());
                if count > 0 {
                    UserOutPtr::<InfoMapsEntry>::from(buffer).write_array(&entries[..count])?;
                }
                actual.write_if_not_null(count)?;
                avail.write_if_not_null(entries.len())?;
            }
            Topic::VmarMaps => {
                let vmar =
                    proc.get_object_with_rights::<VmAddressRegion>(handle, Rights::INSPECT)?;
                let entries = vmar.get_info_maps(0)?;
                let entry_size = core::mem::size_of::<InfoMapsEntry>();
                let count = (buffer_size / entry_size).min(entries.len());
                if count > 0 {
                    UserOutPtr::<InfoMapsEntry>::from(buffer).write_array(&entries[..count])?;
                }
                actual.write_if_not_null(count)?;
                avail.write_if_not_null(entries.len())?;
            }
            _ => {
                error!("not supported info topic: {:?}", topic);
                return Err(ZxError::NOT_SUPPORTED);
            }
        }
        Ok(())
    }

    /// Asserts and deasserts the userspace-accessible signal bits on the object's peer.
    pub fn sys_object_signal_peer(
        &self,
        handle_value: HandleValue,
        clear_mask: u32,
        set_mask: u32,
    ) -> ZxResult {
        info!(
            "object.signal_peer: handle_value = {:#x}, clear_mask = {:#x}, set_mask = {:#x}",
            handle_value, clear_mask, set_mask
        );
        let proc = self.thread.proc();
        let object = proc.get_dyn_object_with_rights(handle_value, Rights::SIGNAL_PEER)?;
        let allowed_signals = object.allowed_signals();
        let clear_signal = Signal::verify_user_signal(allowed_signals, clear_mask)?;
        let set_signal = Signal::verify_user_signal(allowed_signals, set_mask)?;
        object.peer()?.signal_change(clear_signal, set_signal);
        Ok(())
    }

    /// A non-blocking syscall subscribes for signals on an object.
    pub fn sys_object_wait_async(
        &self,
        handle_value: HandleValue,
        port_handle_value: HandleValue,
        key: u64,
        signals: u32,
        options: u32,
    ) -> ZxResult {
        let signals = Signal::from_bits_truncate(signals);
        info!(
            "object.wait_async: handle={:#x}, port={:#x}, key={:#x}, signal={:?}, options={:#X}",
            handle_value, port_handle_value, key, signals, options
        );
        const ZX_WAIT_ASYNC_TIMESTAMP: u32 = 1 << 0;
        const ZX_WAIT_ASYNC_EDGE: u32 = 1 << 1;
        const ZX_WAIT_ASYNC_BOOT_TIMESTAMP: u32 = 1 << 2;
        const VALID_OPTIONS: u32 =
            ZX_WAIT_ASYNC_EDGE | ZX_WAIT_ASYNC_BOOT_TIMESTAMP | ZX_WAIT_ASYNC_TIMESTAMP;
        if options & !VALID_OPTIONS != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        // TIMESTAMP and BOOT_TIMESTAMP are mutually exclusive.
        if options & ZX_WAIT_ASYNC_TIMESTAMP != 0 && options & ZX_WAIT_ASYNC_BOOT_TIMESTAMP != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let use_timestamp = options & (ZX_WAIT_ASYNC_TIMESTAMP | ZX_WAIT_ASYNC_BOOT_TIMESTAMP) != 0;
        let proc = self.thread.proc();
        let object = self.get_object_with_pseudo(handle_value, Rights::WAIT)?;
        let port = proc.get_object_with_rights::<Port>(port_handle_value, Rights::WRITE)?;
        if options & ZX_WAIT_ASYNC_EDGE != 0 {
            object.send_signal_to_port_async_edge(signals, &port, key, use_timestamp)?;
        } else {
            object.send_signal_to_port_async(signals, &port, key, use_timestamp)?;
        }
        Ok(())
    }

    /// Signal an object.
    ///
    /// Asserts and deasserts the userspace-accessible signal bits on an object.
    pub fn sys_object_signal(
        &self,
        handle_value: HandleValue,
        clear_mask: u32,
        set_mask: u32,
    ) -> ZxResult {
        info!(
            "object.signal: handle_value={:#x}, clear_mask={:#x}, set_mask={:#x}",
            handle_value, clear_mask, set_mask
        );
        let object = self.get_object_with_pseudo(handle_value, Rights::SIGNAL)?;
        let allowed_signals = object.allowed_signals();
        info!("{:?} allowed: {:?}", object, allowed_signals);
        let clear_signal = Signal::verify_user_signal(allowed_signals, clear_mask)?;
        let set_signal = Signal::verify_user_signal(allowed_signals, set_mask)?;
        object.signal_change(clear_signal, set_signal);
        Ok(())
    }

    /// Wait for signals on multiple objects.
    pub async fn sys_object_wait_many(
        &self,
        mut user_items: UserInOutPtr<UserWaitItem>,
        count: u32,
        deadline: Deadline,
    ) -> ZxResult {
        if count > MAX_WAIT_MANY_ITEMS {
            return Err(ZxError::OUT_OF_RANGE);
        }
        let mut items = user_items.read_array(count as usize)?;
        info!("user_items: {:#x?}, deadline: {:?}", user_items, deadline);
        let mut waiters = Vec::with_capacity(count as usize);
        for item in items.iter() {
            let object = self.get_object_with_pseudo(item.handle, Rights::WAIT)?;
            waiters.push((object, item.wait_for));
        }
        let future = wait_signal_many(&waiters);
        self.thread.set_blocking_state(ThreadState::BlockedWaitMany);
        let res = self
            .thread
            .blocking_run(future, ThreadState::BlockedWaitMany, deadline.into(), None)
            .await?;
        for (i, item) in items.iter_mut().enumerate() {
            item.observed = res[i];
        }
        user_items.write_array(&items)?;
        Ok(())
    }

    /// Find the child of an object by its kid.
    ///
    /// Given a kernel object with children objects, obtain a handle to the child specified by the provided kernel object id.
    pub fn sys_object_get_child(
        &self,
        handle: HandleValue,
        koid: KoID,
        rights: u32,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "object.get_child: handle={:#x}, koid={:#x}, rights={:#x}",
            handle, koid, rights
        );
        let mut rights = Rights::from_bits(rights).ok_or(ZxError::INVALID_ARGS)?;
        let proc = self.thread.proc();
        let (task, parent_rights) = proc.get_dyn_object_and_rights(handle)?;
        if !parent_rights.contains(Rights::ENUMERATE) {
            return Err(ZxError::ACCESS_DENIED);
        }
        if rights == Rights::SAME_RIGHTS {
            rights = parent_rights;
        } else if (rights & parent_rights) != rights {
            return Err(ZxError::ACCESS_DENIED);
        }
        let child = task.get_child(koid)?;
        let child_handle = proc.add_handle(Handle::new(child, rights));
        out.write(child_handle)?;
        Ok(())
    }
}

numeric_enum! {
    #[repr(u32)]
    #[derive(Debug)]
    enum Topic {
        None = 0,
        HandleValid = 1,
        HandleBasic = 2,
        Process = 3,
        ProcessThreads = 4,
        Vmar = 7,
        JobChildren = 8,
        JobProcess = 9,
        Thread = 10,
        ThreadExceptionReport = 11,
        TaskStats = 12,
        ProcessMaps = 13,
        ProcessVmos = 14,
        ThreadStats = 15,
        CpuStats = 16,
        KmemStats = 17,
        Resource = 18,
        HandleCount = 19,
        Bti = 20,
        ProcessHandleStats = 21,
        Socket = 22,
        Vmo = 23,
        Job = 24,
        Timer = 25,
        Stream = 26,
        HandleTable = 27,
        GuestStats = 29,
        TaskRuntime = 30,
        KmemStatsExtended = 31,
        VmarMaps = 36,
        MemoryStall = 38,
        ClockMappedSize = 40,
    }
}

numeric_enum! {
    #[repr(u32)]
    #[derive(Debug)]
    enum Property {
        RegisterGs = 2,
        Name = 3,
        RegisterFs = 4,
        ProcessDebugAddr = 5,
        ProcessVdsoBaseAddress = 6,
        ProcessBreakOnLoad = 7,
        ProcessHwTraceContextId = 8,
        SocketRxThreshold = 12,
        SocketTxThreshold = 13,
        JobKillOnOom = 15,
        ExceptionState = 16,
        VmoContentSize = 17,
        ExceptionStrategy = 18,
        StreamModeAppend = 19,
    }
}

const MAX_NAME_LEN: usize = 32;
const MAX_WAIT_MANY_ITEMS: u32 = 32;

#[derive(Debug)]
#[repr(C)]
pub struct UserWaitItem {
    handle: HandleValue,
    wait_for: Signal,
    observed: Signal,
}

/// `zx_info_handle_extended_t` — per-handle information for ZX_INFO_HANDLE_TABLE.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct HandleExtendedInfo {
    pub obj_type: u32,
    pub handle_value: u32,
    pub rights: u32,
    pub reserved: u32,
    pub koid: u64,
    pub related_koid: u64,
    pub peer_owner_koid: u64,
}

/// `zx_info_thread_stats_t` — per-thread runtime statistics.
#[repr(C)]
#[derive(Default)]
struct ThreadStatsInfo {
    total_runtime: i64,
    last_scheduled_cpu: u32,
    padding1: [u8; 4],
}

/// `zx_info_task_runtime_t` — CPU and scheduling time for a task.
#[repr(C)]
#[derive(Default)]
struct TaskRuntimeInfo {
    cpu_time: i64,
    queue_time: i64,
    page_fault_time: i64,
    lock_contention_time: i64,
}

/// `zx_info_kmem_stats_t` — 152 bytes (19 x u64).
#[repr(C)]
#[derive(Default)]
struct KmemInfo {
    total_bytes: u64,
    free_bytes: u64,
    free_loaned_bytes: u64,
    wired_bytes: u64,
    total_heap_bytes: u64,
    free_heap_bytes: u64,
    vmo_bytes: u64,
    mmu_overhead_bytes: u64,
    ipc_bytes: u64,
    cache_bytes: u64,
    slab_bytes: u64,
    zram_bytes: u64,
    other_bytes: u64,
    vmo_reclaim_total_bytes: u64,
    vmo_reclaim_newest_bytes: u64,
    vmo_reclaim_oldest_bytes: u64,
    vmo_reclaim_disabled_bytes: u64,
    vmo_discardable_locked_bytes: u64,
    vmo_discardable_unlocked_bytes: u64,
}

/// `zx_info_kmem_stats_extended_t` — includes all base kmem fields
/// plus pager-specific extended fields.
#[repr(C)]
#[derive(Default)]
struct KmemStatsExtendedInfo {
    // Base kmem stats (same as KmemInfo, 19 fields, 152 bytes)
    total_bytes: u64,
    free_bytes: u64,
    free_loaned_bytes: u64,
    wired_bytes: u64,
    total_heap_bytes: u64,
    free_heap_bytes: u64,
    vmo_bytes: u64,
    mmu_overhead_bytes: u64,
    ipc_bytes: u64,
    cache_bytes: u64,
    slab_bytes: u64,
    zram_bytes: u64,
    other_bytes: u64,
    vmo_reclaim_total_bytes: u64,
    vmo_reclaim_newest_bytes: u64,
    vmo_reclaim_oldest_bytes: u64,
    vmo_reclaim_disabled_bytes: u64,
    vmo_discardable_locked_bytes: u64,
    vmo_discardable_unlocked_bytes: u64,
    // Extended pager fields
    vmo_pager_total_bytes: u64,
    vmo_pager_newest_bytes: u64,
    vmo_pager_oldest_bytes: u64,
    vmo_pager_writeback_bytes: u64,
}

/// `zx_info_cpu_stats_t` — 120 bytes per CPU.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct CpuStatsInfo {
    cpu_number: u32,
    flags: u32,
    idle_time: i64,
    reschedules: u64,
    context_switches: u64,
    irq_preempts: u64,
    preempts: u64,
    yields: u64,
    ints: u64,
    timer_ints: u64,
    timers: u64,
    page_faults: u64,
    exceptions: u64,
    syscalls: u64,
    reschedule_ipis: u64,
    generic_ipis: u64,
}

/// `zx_info_guest_stats_t` — per-CPU guest stats (x86_64).
/// On x86_64 Fuchsia uses 120 bytes per entry (same layout as CpuStatsInfo
/// but with guest-specific fields). For zCore we reuse the CpuStatsInfo
/// layout with zeroed counters.
type GuestStatsInfo = CpuStatsInfo;

/// `zx_info_memory_stall_t` — 16 bytes.
#[repr(C)]
#[derive(Default)]
struct MemoryStallInfo {
    stall_time_some: i64,
    stall_time_full: i64,
}

/// `zx_info_process_handle_stats_t` — 256 bytes (64 x u32).
#[repr(C)]
#[derive(Clone, Copy)]
struct ProcessHandleStatsInfo {
    handle_count: [u32; 64],
}

impl Default for ProcessHandleStatsInfo {
    fn default() -> Self {
        Self {
            handle_count: [0u32; 64],
        }
    }
}
