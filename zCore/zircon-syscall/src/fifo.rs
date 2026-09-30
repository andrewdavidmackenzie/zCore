use {super::*, zircon_object::ipc::Fifo};

impl Syscall<'_> {
    /// Creates a fifo, which is actually a pair of fifos of `elem_count` entries of `elem_size` bytes.
    pub fn sys_fifo_create(
        &self,
        elem_count: usize,
        elem_size: usize,
        options: u32,
        mut out0: UserOutPtr<HandleValue>,
        mut out1: UserOutPtr<HandleValue>,
    ) -> ZxResult {
        info!(
            "fifo.create: count={:#x}, item_size={:#x}, options={:#x}",
            elem_count, elem_size, options,
        );
        if options != 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        if elem_count == 0 || elem_size == 0 || elem_count * elem_size > 4096 {
            return Err(ZxError::OUT_OF_RANGE);
        }
        let (end0, end1) = Fifo::create(elem_count, elem_size);
        let proc = self.thread.proc();
        let handle0 = proc.add_handle(Handle::new(end0, Rights::DEFAULT_FIFO));
        let handle1 = proc.add_handle(Handle::new(end1, Rights::DEFAULT_FIFO));
        out0.write(handle0)?;
        out1.write(handle1)?;
        Ok(())
    }

    /// Write data to a fifo.
    pub fn sys_fifo_write(
        &self,
        handle_value: HandleValue,
        elem_size: usize,
        user_bytes: UserInPtr<u8>,
        count: usize,
        mut actual_count_ptr: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "fifo.write: handle={:?}, item_size={}, count={:#x}",
            handle_value, elem_size, count
        );
        if count != 0 {
            let data = user_bytes.read_array(count * elem_size)?;
            let actual_count = self
                .thread
                .proc()
                .get_object_with_rights::<Fifo>(handle_value, Rights::WRITE)?
                .write(elem_size, &data, count)?;
            actual_count_ptr.write_if_not_null(actual_count)?;
            Ok(())
        } else {
            Err(ZxError::OUT_OF_RANGE)
        }
    }

    /// Read data from a fifo.
    pub fn sys_fifo_read(
        &self,
        handle_value: HandleValue,
        elem_size: usize,
        mut user_bytes: UserOutPtr<u8>,
        count: usize,
        mut actual_count_ptr: UserOutPtr<usize>,
    ) -> ZxResult {
        info!(
            "fifo.read: handle={:?}, item_size={}, count={:#x}",
            handle_value, elem_size, count
        );
        if count == 0 {
            return Err(ZxError::OUT_OF_RANGE);
        }
        let proc = self.thread.proc();
        let fifo = proc.get_object_with_rights::<Fifo>(handle_value, Rights::READ)?;
        let mut data = vec![0; elem_size * count];
        let actual_count = fifo.read(elem_size, &mut data, count)?;
        let actual_bytes = actual_count * elem_size;
        // Try writing to user buffer. If this fails (e.g. bad pointer),
        // roll back the FIFO read so data is not lost.
        if user_bytes.write_array(&data[..actual_bytes]).is_err() {
            fifo.rollback_read(&data[..actual_bytes]);
            return Err(ZxError::INVALID_ARGS);
        }
        actual_count_ptr.write_if_not_null(actual_count)?;
        Ok(())
    }
}
