use super::*;
use zircon_object::dev::*;
use zircon_object::task::Flavour;

impl Syscall<'_> {
    /// Write debug info to the serial port.
    pub fn sys_debug_write(&self, buf: UserInPtr<u8>, len: usize) -> ZxResult {
        trace!("debug.write: buf=({:?}; {:#x})", buf, len);
        hal_impl::console::console_write_str(&buf.read_string(len)?);
        Ok(())
    }

    /// Read debug info from the serial port.
    pub async fn sys_debug_read(
        &self,
        handle: HandleValue,
        mut buf: UserOutPtr<u8>,
        buf_size: u32,
        mut actual: UserOutPtr<u32>,
    ) -> ZxResult {
        trace!(
            "debug.read: handle={:#x}, buf=({:?}; {:#x})",
            handle,
            buf,
            buf_size
        );
        let proc = self.thread.proc();
        proc.get_object::<Resource>(handle)?
            .validate(ResourceKind::ROOT)?;
        let mut vec = vec![0u8; buf_size as usize];
        let len = hal_impl::console::console_read(&mut vec).await;
        buf.write_array(&vec[..len])?;
        actual.write(len as u32)?;
        Ok(())
    }

    /// Execute a program from the rootfs by path.
    ///
    /// Reads the binary, detects its flavour from the ELF header,
    /// spawns it, and waits for it to terminate before returning.
    pub async fn sys_debug_exec(&self, path_ptr: UserInPtr<u8>, path_len: usize) -> ZxResult {
        if path_len == 0 || path_len > 256 {
            return Err(ZxError::INVALID_ARGS);
        }
        let path = path_ptr.read_string(path_len)?;
        if !path.starts_with('/') {
            return Err(ZxError::INVALID_ARGS);
        }
        info!("debug.exec: path={:?}", path);

        // Read the binary from the rootfs.
        let data = zircon_object::task::spawn::read_rootfs_file(&path).ok_or(ZxError::NOT_FOUND)?;

        let flavour = Flavour::from_elf(&data);
        info!("debug.exec: detected flavour {:?}", flavour);

        // Spawn by flavour — unified dispatch.
        let job = self.thread.proc().job();
        let proc = zircon_object::task::spawn::spawn_by_flavour(flavour, &job, &path, &data)?;

        // Wait for the process to terminate. Use timed sleeps to
        // yield the executor — wait_signal doesn't reliably wake
        // across executor tasks on different CPUs.
        // Wait for the process to terminate. Use timed sleeps to
        // yield the executor — wait_signal doesn't reliably wake
        // across executor tasks on different CPUs.
        while proc.exit_code().is_none() {
            hal_impl::thread::sleep_until(
                hal_impl::timer::timer_now() + core::time::Duration::from_millis(1),
            )
            .await;
        }
        Ok(())
    }
}
