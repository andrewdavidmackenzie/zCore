use {super::*, crate::object::*, alloc::sync::Arc, lock::Mutex, numeric_enum_macro::numeric_enum};

/// A readable, writable, seekable interface to some underlying storage
///
/// ## SYNOPSIS
///
/// A stream is an interface for reading and writing data to some underlying
/// storage, typically a VMO.
pub struct Stream {
    base: KObjectBase,
    options: u32,
    vmo: Arc<VmObject>,
    seek: Mutex<usize>,
}

impl_kobject!(Stream);

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
            options,
            vmo,
            seek: Mutex::new(seek),
        })
    }

    /// Read data from the stream at the current seek offset
    pub fn read(&self, data: &mut [u8]) -> ZxResult<usize> {
        let mut seek = self.seek.lock();
        let length = self.read_at(data, *seek)?;
        *seek += length;
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

    /// write data to the stream at the current seek offset or append data at the end of content
    pub fn write(&self, data: &[u8], append: bool) -> ZxResult<usize> {
        let mut seek = self.seek.lock();
        if append {
            *seek = self.vmo.content_size();
        }
        let length = self.write_at(data, *seek)?;
        *seek += length;
        Ok(length)
    }

    /// Write data to the stream at a given offset
    pub fn write_at(&self, data: &[u8], offset: usize) -> ZxResult<usize> {
        let count = data.len();
        let mut content_size = self.vmo.content_size();
        let (target_size, overflow) = offset.overflowing_add(count);
        if overflow {
            return Err(ZxError::FILE_BIG);
        }
        if target_size > content_size {
            content_size = self.vmo.set_content_size_and_resize(target_size, offset)?;
        }
        if offset >= content_size {
            return Err(ZxError::NO_SPACE);
        }
        let length = count.min(content_size - offset);
        self.vmo.write(offset, &data[..length])?;
        Ok(length)
    }

    /// Modify the current seek offset of the stream
    pub fn seek(&self, whence: SeekOrigin, offset: isize) -> ZxResult<usize> {
        let mut seek = self.seek.lock();
        let origin: usize = match whence {
            SeekOrigin::Start => 0,
            SeekOrigin::Current => *seek,
            SeekOrigin::End => self.vmo.content_size(),
        };
        if offset >= 0 {
            let (target, overflow) = origin.overflowing_add(offset as usize);
            if overflow {
                return Err(ZxError::INVALID_ARGS);
            }
            *seek = target;
        } else {
            let target = origin as isize + offset;
            if origin as isize >= 0 && target < 0 {
                return Err(ZxError::INVALID_ARGS);
            }
            *seek = target as usize;
        }
        Ok(*seek)
    }

    /// Get information of the socket.
    pub fn get_info(&self) -> StreamInfo {
        let seek = self.seek.lock();
        StreamInfo {
            options: self.options,
            padding1: 0,
            seek: *seek as u64,
            content_size: self.vmo.content_size() as u64,
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
