use {super::*, crate::object::*, alloc::sync::Arc, lock::Mutex, numeric_enum_macro::numeric_enum};

/// A readable, writable, seekable interface to some underlying storage
///
/// ## SYNOPSIS
///
/// A stream is an interface for reading and writing data to some underlying
/// storage, typically a VMO.
/// Bit flag for append mode (matches `ZX_STREAM_MODE_APPEND`).
const MODE_APPEND: u32 = 1 << 2;

pub struct Stream {
    base: KObjectBase,
    vmo: Arc<VmObject>,
    /// Mutable state: (options, seek_offset).
    inner: Mutex<StreamInner>,
}

struct StreamInner {
    options: u32,
    seek: usize,
}

impl_kobject!(Stream
    fn supports_name(&self) -> bool {
        false
    }
);

numeric_enum! {
    #[repr(usize)]
    #[derive(Debug)]
    /// Enumeration of possible methods to modify the seek within an Stream.
    pub enum SeekOrigin {
        /// Set the seek offset relative to the start of the stream.
        Start = 0,
        /// Set the seek offset relative to the current seek offset of the stream.
        Current = 1,
        /// Set the seek offset relative to the end of the stream, as defined by the content size of the stream.
        End = 2,
    }
}

impl Stream {
    /// Create a stream from a VMO
    pub fn create(vmo: Arc<VmObject>, seek: usize, options: u32) -> Arc<Self> {
        Arc::new(Stream {
            base: KObjectBase::default(),
            vmo,
            inner: Mutex::new(StreamInner { options, seek }),
        })
    }

    /// Get the VMO's current size in bytes.
    pub fn vmo_len(&self) -> usize {
        self.vmo.len()
    }

    /// Check if this stream is effectively in append mode.
    ///
    /// True if either the per-call `append` flag is set or the stream
    /// was created with `MODE_APPEND`.
    pub fn is_append(&self, per_call_append: bool) -> bool {
        per_call_append || (self.inner.lock().options & MODE_APPEND) != 0
    }

    /// Get the offset where the next write would occur.
    ///
    /// If `append` is true (or stream has MODE_APPEND), returns content_size.
    /// Otherwise returns the current seek offset.
    pub fn write_offset(&self, append: bool) -> usize {
        let inner = self.inner.lock();
        if append || (inner.options & MODE_APPEND) != 0 {
            self.vmo.content_size()
        } else {
            inner.seek
        }
    }

    /// Read data from the stream at the current seek offset
    pub fn read(&self, data: &mut [u8]) -> ZxResult<usize> {
        let mut inner = self.inner.lock();
        let length = self.read_at(data, inner.seek)?;
        inner.seek += length;
        Ok(length)
    }

    /// Read data from the stream at a given offset
    pub fn read_at(&self, data: &mut [u8], offset: usize) -> ZxResult<usize> {
        let count = data.len();
        let content_size = self.vmo.content_size();
        if offset >= content_size {
            return Ok(0);
        }
        let length = count.min(content_size - offset);
        self.vmo.read(offset, &mut data[..length])?;
        Ok(length)
    }

    /// Write data to the stream at the current seek offset or append data at the end of content.
    ///
    /// `append` is true when `ZX_STREAM_APPEND` is passed to `stream_writev`.
    /// The stream also appends when `MODE_APPEND` is set in the options
    /// (via `ZX_PROP_STREAM_MODE_APPEND`).
    pub fn write(&self, data: &[u8], append: bool) -> ZxResult<usize> {
        // Zero-length writes are no-ops — don't update seek position.
        if data.is_empty() {
            return Ok(0);
        }
        let mut inner = self.inner.lock();
        let do_append = append || (inner.options & MODE_APPEND) != 0;
        if do_append {
            inner.seek = self.vmo.content_size();
        }
        let length = self.write_at(data, inner.seek)?;
        inner.seek += length;
        Ok(length)
    }

    /// Write data to the stream at a given offset.
    ///
    /// Streams never resize their backing VMO.  Writes are clamped to the
    /// VMO's current storage size.  Content-size is extended up to the
    /// high-water mark of written data.  Any gap between the old
    /// content-size and the write offset is zero-filled.
    pub fn write_at(&self, data: &[u8], offset: usize) -> ZxResult<usize> {
        let count = data.len();
        let vmo_len = self.vmo.len();
        // Check for offset + count overflow.
        if offset.checked_add(count).is_none() {
            return Err(ZxError::FILE_BIG);
        }
        // If offset is at or past the VMO boundary, no bytes can be written.
        if offset >= vmo_len {
            return Err(ZxError::OUT_OF_RANGE);
        }
        // Clamp to the VMO boundary.
        let length = count.min(vmo_len - offset);
        if length == 0 {
            return Ok(0);
        }
        // Zero the gap between old content_size and write offset.
        let old_content_size = self.vmo.content_size();
        if offset > old_content_size {
            let zero_end = offset.min(vmo_len);
            if zero_end > old_content_size {
                self.vmo
                    .zero(old_content_size, zero_end - old_content_size)?;
            }
        }
        self.vmo.write(offset, &data[..length])?;
        // Extend content_size if we wrote past the old high-water mark.
        let new_end = offset + length;
        if new_end > old_content_size {
            self.vmo.set_content_size(new_end)?;
        }
        Ok(length)
    }

    /// Modify the current seek offset of the stream
    pub fn seek(&self, whence: SeekOrigin, offset: isize) -> ZxResult<usize> {
        let mut inner = self.inner.lock();
        let origin: usize = match whence {
            SeekOrigin::Start => 0,
            SeekOrigin::Current => inner.seek,
            SeekOrigin::End => self.vmo.content_size(),
        };
        if offset >= 0 {
            let (target, overflow) = origin.overflowing_add(offset as usize);
            if overflow {
                return Err(ZxError::INVALID_ARGS);
            }
            inner.seek = target;
        } else {
            // Check for underflow: origin + negative offset < 0.
            let target = (origin as i64).checked_add(offset as i64);
            match target {
                Some(t) if t >= 0 => inner.seek = t as usize,
                _ => return Err(ZxError::INVALID_ARGS),
            }
        }
        Ok(inner.seek)
    }

    /// Get information about the stream.
    pub fn get_info(&self) -> StreamInfo {
        let inner = self.inner.lock();
        StreamInfo {
            options: inner.options,
            padding1: 0,
            seek: inner.seek as u64,
            content_size: self.vmo.content_size() as u64,
        }
    }

    /// Get whether append mode is enabled.
    pub fn get_mode_append(&self) -> bool {
        let inner = self.inner.lock();
        (inner.options & MODE_APPEND) != 0
    }

    /// Set or clear append mode.
    pub fn set_mode_append(&self, enable: bool) {
        let mut inner = self.inner.lock();
        if enable {
            inner.options |= MODE_APPEND;
        } else {
            inner.options &= !MODE_APPEND;
        }
    }
}

/// Information of a Stream
#[repr(C)]
#[derive(Default)]
pub struct StreamInfo {
    /// The options passed to `Stream::create()`.
    options: u32,
    padding1: u32,
    /// The current seek offset.
    ///
    /// Used by stream_readv and stream_writev to determine where to read
    /// and write the stream.
    seek: u64,
    /// The current size of the stream.
    ///
    /// The number of bytes in the stream that store data. The stream itself
    /// might have a larger capacity to avoid reallocating the underlying storage
    /// as the stream grows or shrinks.
    /// NOTE: in fact, this value is store in the VmObject associated and can be
    /// get/set through 'object_[get/set]_property(vmo_handle, ...)'
    content_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::VmObject;

    #[test]
    fn create_and_write_read() {
        let vmo = VmObject::new_paged(4); // 4 pages = 16K
        let stream = Stream::create(vmo, 0, 0);

        // Write data
        let data = b"Hello, Stream!";
        let written = stream.write(data, false).expect("write failed");
        assert_eq!(written, data.len());

        // Seek back to start
        let pos = stream.seek(SeekOrigin::Start, 0).expect("seek failed");
        assert_eq!(pos, 0);

        // Read it back
        let mut buf = [0u8; 64];
        let read = stream.read(&mut buf[..data.len()]).expect("read failed");
        assert_eq!(read, data.len());
        assert_eq!(&buf[..data.len()], data);
    }

    #[test]
    fn write_at_read_at() {
        let vmo = VmObject::new_paged(4);
        let stream = Stream::create(vmo, 0, 0);

        let data = b"offset test";
        let offset = 100;
        let written = stream.write_at(data, offset).expect("write_at failed");
        assert_eq!(written, data.len());

        let mut buf = [0u8; 64];
        let read = stream
            .read_at(&mut buf[..data.len()], offset)
            .expect("read_at failed");
        assert_eq!(read, data.len());
        assert_eq!(&buf[..data.len()], data);
    }

    #[test]
    fn seek_operations() {
        let vmo = VmObject::new_paged(4);
        let stream = Stream::create(vmo, 0, 0);

        // Write some data to establish content
        stream.write(b"1234567890", false).unwrap();

        // Seek from start
        assert_eq!(stream.seek(SeekOrigin::Start, 5).unwrap(), 5);

        // Seek from current (relative)
        assert_eq!(stream.seek(SeekOrigin::Current, 3).unwrap(), 8);

        // Seek from end (negative offset from content end)
        assert_eq!(stream.seek(SeekOrigin::End, -2).unwrap(), 8);

        // Seek to start
        assert_eq!(stream.seek(SeekOrigin::Start, 0).unwrap(), 0);
    }

    #[test]
    fn get_info() {
        let vmo = VmObject::new_paged(1);
        let stream = Stream::create(vmo, 42, 0);
        let info = stream.get_info();
        assert_eq!(info.seek, 42);
    }
}
