use super::*;

impl Syscall<'_> {
    /// Draw random bytes from the kernel CPRNG.
    ///
    /// This data should be suitable for cryptographic applications.
    ///
    /// Clients that require a large volume of randomness should consider using these bytes to seed a user-space random number generator for better performance.
    pub fn sys_cprng_draw_once(&self, mut buf: UserOutPtr<u8>, len: usize) -> ZxResult {
        info!("cprng_draw_once: buf=({:?}; {:?})", buf, len);
        let mut res = vec![0u8; len];
        // Fill random bytes to the buffer
        kernel_hal::rand::fill_random(&mut res);
        buf.write_array(&res)?;
        Ok(())
    }

    /// Add entropy to the kernel CPRNG.
    ///
    /// The Zircon spec permits implementations to silently discard
    /// the provided entropy. We accept and drop it.
    pub fn sys_cprng_add_entropy(&self, buf: UserInPtr<u8>, len: usize) -> ZxResult {
        info!("cprng_add_entropy: buf=({:?}; {:?})", buf, len);
        if len == 0 {
            return Err(ZxError::INVALID_ARGS);
        }
        // Read the buffer to validate the user pointer, then discard.
        let _data = buf.read_array(len)?;
        Ok(())
    }
}
