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
    /// The Zircon ABI accepts 0..=256 bytes. Zero-length is a no-op.
    /// The kernel CPRNG currently uses hardware random (rdrand/…)
    /// directly, so there is no software entropy pool to mix into.
    /// The supplied bytes are validated but discarded.
    // TODO: add a software entropy pool and mix supplied bytes into it
    pub fn sys_cprng_add_entropy(&self, buf: UserInPtr<u8>, len: usize) -> ZxResult {
        const ZX_CPRNG_ADD_ENTROPY_MAX_LEN: usize = 256;
        info!("cprng_add_entropy: buf=({:?}; {:?})", buf, len);
        if len > ZX_CPRNG_ADD_ENTROPY_MAX_LEN {
            return Err(ZxError::INVALID_ARGS);
        }
        if len == 0 {
            return Ok(());
        }
        // Read the buffer to validate the user pointer, then discard.
        let _data = buf.read_array(len)?;
        Ok(())
    }
}
