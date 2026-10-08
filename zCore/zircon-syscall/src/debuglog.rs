use {super::*, zircon_object::debuglog::*};

impl Syscall<'_> {
    /// Create a kernel managed debuglog reader or writer.
    pub fn sys_debuglog_create(
        &self,
        rsrc: HandleValue,
        options: u32,
        mut target: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "debuglog.create: resource_handle={:#x?}, options={:#x?}",
            rsrc, options,
        );
        const FLAG_READABLE: u32 = 0x4000_0000u32;
        // Validate options — only FLAG_READABLE is allowed.
        if options & !FLAG_READABLE != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        if rsrc != 0 {
            // Validate: ROOT or SYSTEM/DEBUGLOG_BASE resource.
            let res = proc.get_resource(rsrc)?;
            if res
                .validate(zircon_object::dev::ResourceKind::ROOT)
                .is_err()
            {
                res.validate_ranged_resource(
                    zircon_object::dev::ResourceKind::SYSTEM,
                    zircon_object::dev::ZX_RSRC_SYSTEM_DEBUGLOG_BASE,
                    1,
                )
                .map_err(|_| ZxError::WRONG_TYPE)?;
            }
        } else if options & FLAG_READABLE != 0 {
            // ZX_HANDLE_INVALID is only allowed for write-only debuglogs.
            return Err(ZxError::BAD_HANDLE);
        }
        let dlog = DebugLog::create(options);
        let dlog_right = if options & FLAG_READABLE == 0 {
            Rights::DEFAULT_DEBUGLOG
        } else {
            Rights::DEFAULT_DEBUGLOG | Rights::READ
        };
        let dlog_handle = proc.add_handle(Handle::new(dlog, dlog_right));
        target.write(dlog_handle)?;
        Ok(())
    }

    /// Write log entry to debuglog.
    pub fn sys_debuglog_write(
        &self,
        handle_value: HandleValue,
        options: u32,
        buf: UserInPtr<u8>,
        len: usize,
    ) -> ZxResult {
        info!(
            "debuglog.write: handle={:#x?}, options={:#x?}, buf=({:#x?}; {:#x?})",
            handle_value, options, buf, len,
        );
        const LOG_FLAGS_MASK: u32 = 0x10;
        if options & !LOG_FLAGS_MASK != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let datalen = len.min(DLOG_MAX_DATA);
        let data = buf.read_string(datalen)?;
        let proc = self.thread.proc();
        let dlog = proc.get_object_with_rights::<DebugLog>(handle_value, Rights::WRITE)?;
        dlog.write(Severity::Info, options, self.thread.id(), proc.id(), &data);
        // print to kernel console
        hal_impl::console::console_write_str(&data);
        if data.as_bytes().last() != Some(&b'\n') {
            hal_impl::console::console_write_str("\n");
        }
        Ok(())
    }

    /// Read log entries from debuglog.
    ///
    /// Returns the number of bytes read on success (as a positive isize),
    /// matching Zircon's `zx_debuglog_read` ABI where the byte count is
    /// returned through the status value.
    pub fn sys_debuglog_read(
        &self,
        handle_value: HandleValue,
        options: u32,
        mut buf: UserOutPtr<u8>,
        len: usize,
    ) -> Result<isize, ZxError> {
        info!(
            "debuglog.read: handle={:#x?}, options={:#x?}, buf=({:#x?}; {:#x?})",
            handle_value, options, buf, len,
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        let proc = self.thread.proc();
        let mut buffer = [0; DLOG_MAX_LEN];
        let dlog = proc.get_object_with_rights::<DebugLog>(handle_value, Rights::READ)?;
        let actual_len = dlog.read(&mut buffer).min(len);
        if actual_len == 0 {
            return Err(ZxError::SHOULD_WAIT);
        }
        buf.write_array(&buffer[..actual_len])?;
        Ok(actual_len as isize)
    }
}
