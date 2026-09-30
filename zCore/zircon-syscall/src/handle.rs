use {super::*, core::convert::TryFrom};

impl Syscall<'_> {
    /// Creates a duplicate of handle.
    ///
    /// Referring to the same underlying object, with new access rights rights.
    pub fn sys_handle_duplicate(
        &self,
        handle_value: HandleValue,
        rights: u32,
        mut new_handle_value: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        let rights = Rights::try_from(rights)?;
        info!(
            "handle.dup: handle={:#x?}, rights={:?}",
            handle_value, rights
        );
        let proc = self.thread.proc();
        // Handle pseudo-handles: create a real handle from the pseudo-handle object.
        if let Some(obj) = self.resolve_pseudo_handle(handle_value) {
            let dup_rights = if rights.contains(Rights::SAME_RIGHTS) {
                Rights::all()
            } else {
                rights
            };
            let new_value = proc.add_handle(Handle::new(obj, dup_rights));
            new_handle_value.write(new_value)?;
            return Ok(());
        }
        let new_value = proc.dup_handle_operating_rights(handle_value, |handle_rights| {
            if !handle_rights.contains(Rights::DUPLICATE) {
                return Err(ZxError::ACCESS_DENIED);
            }
            if !rights.contains(Rights::SAME_RIGHTS) {
                if (handle_rights & rights).bits() != rights.bits() {
                    return Err(ZxError::INVALID_ARGS);
                }
                Ok(rights)
            } else {
                Ok(handle_rights)
            }
        })?;
        new_handle_value.write(new_value)?;
        Ok(())
    }

    /// Check that a handle is valid (exists in the process's handle table).
    ///
    /// In Fuchsia this is a vDSO-only function that checks the handle table
    /// directly. In zCore we implement it as a lightweight syscall since
    /// userspace cannot access the handle table.
    /// Check that a handle is valid (exists in the process's handle table).
    ///
    /// In Fuchsia this is a vDSO-only function that checks the handle table
    /// directly. In zCore we implement it as a lightweight syscall since
    /// userspace cannot access the handle table.
    ///
    /// Returns `NOT_FOUND` for handles that don't exist in the table
    /// (matching the error code used by the Fuchsia test suite).
    /// Check that a handle is valid.
    ///
    /// Fuchsia's error codes:
    /// - `INVALID_ARGS` for `ZX_HANDLE_INVALID` (0)
    /// - `OUT_OF_RANGE` for odd handle values (invalid format)
    /// - `NOT_FOUND` for even non-zero handles not in the table
    pub fn sys_handle_check_valid(&self, handle: HandleValue) -> ZxResult {
        info!("handle.check_valid: handle={:#x}", handle);
        if handle == INVALID_HANDLE {
            return Err(ZxError::INVALID_ARGS);
        }
        // Handle values always have bits 0-1 = 0b11 (from add_handle).
        // Any other pattern is an invalid format.
        if handle & 0x3 != 0x3 {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // Pseudo-handles are always valid.
        if self.resolve_pseudo_handle(handle).is_some() {
            return Ok(());
        }
        let proc = self.thread.proc();
        proc.get_handle_info(handle)
            .map(|_| ())
            .map_err(|_| ZxError::NOT_FOUND)
    }

    /// Close a handle and reclaim the underlying object if no other handles to it exist.
    pub fn sys_handle_close(&self, handle: HandleValue) -> ZxResult {
        info!("handle.close: handle={:?}", handle);
        if handle == INVALID_HANDLE {
            return Ok(());
        }
        // Pseudo-handles cannot be closed (Fuchsia returns BAD_HANDLE).
        if self.resolve_pseudo_handle(handle).is_some() {
            return Err(ZxError::BAD_HANDLE);
        }
        let proc = self.thread.proc();
        proc.remove_handle(handle)?;
        Ok(())
    }

    /// Close a number of handles.
    pub fn sys_handle_close_many(
        &self,
        handles: UserInPtr<HandleValue>,
        num_handles: usize,
    ) -> ZxResult {
        info!(
            "handle.close_many: handles=({:#x?}; {:#x?})",
            handles, num_handles,
        );
        let proc = self.thread.proc();
        let mut first_err = None;
        for handle in handles.read_array(num_handles)? {
            if handle != INVALID_HANDLE {
                if let Err(e) = proc.remove_handle(handle) {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }
        // Close all handles even if some are invalid.
        // Return the first error encountered (Fuchsia behavior).
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Creates a replacement for handle.
    ///
    /// Referring to the same underlying object, with new access rights rights.
    pub fn sys_handle_replace(
        &self,
        handle_value: HandleValue,
        rights: u32,
        mut out: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        let rights = Rights::try_from(rights)?;
        info!(
            "handle.replace: handle={:#x?}, rights={:?}",
            handle_value, rights
        );
        let proc = self.thread.proc();
        let new_value = proc.dup_handle_operating_rights(handle_value, |handle_rights| {
            if !rights.contains(Rights::SAME_RIGHTS)
                && (handle_rights & rights).bits() != rights.bits()
            {
                return Err(ZxError::INVALID_ARGS);
            }
            Ok(rights)
        })?;
        proc.remove_handle(handle_value)?;
        out.write(new_value)?;
        Ok(())
    }
}
