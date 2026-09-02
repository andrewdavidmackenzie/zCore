//! Zircon Boot Image (ZBI) format definitions.
//!
//! A ZBI is a concatenation of items, each preceded by a 32-byte header.
//! The first item is a container header wrapping all subsequent items.
//! The ZBI format is defined by the Zircon kernel.
//!
//! Reference: <https://fuchsia.dev/fuchsia-src/reference/kernel/zbi>

/// ZBI item header (32 bytes). Precedes every item including the container.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ZbiHeader {
    /// Item type (one of the `ZBI_TYPE_*` constants).
    pub item_type: u32,
    /// Payload length in bytes (excludes this header and any padding).
    pub length: u32,
    /// Type-specific extra data. For containers: `ZBI_CONTAINER_MAGIC`.
    pub extra: u32,
    /// Flags. Must include `ZBI_FLAG_VERSION`.
    pub flags: u32,
    /// Reserved, must be zero.
    pub reserved0: u32,
    /// Reserved, must be zero.
    pub reserved1: u32,
    /// Must be `ZBI_ITEM_MAGIC`.
    pub magic: u32,
    /// CRC32 of the payload, or `ZBI_ITEM_NO_CRC32`.
    pub crc32: u32,
}

/// Bootfs image header (8 bytes). Appears at the start of a
/// `ZBI_TYPE_STORAGE_BOOTFS` item's payload.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ZbiBootfsHeader {
    /// Must be `ZBI_BOOTFS_MAGIC`.
    pub magic: u32,
    /// Total size of directory entries in bytes.
    pub dirsize: u32,
}

/// Bootfs directory entry. Variable-length: followed by `name_len` bytes
/// of the file path (NUL-terminated), then padded to 4-byte alignment.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ZbiBootfsDirent {
    /// Length of the name field, including NUL terminator.
    pub name_len: u32,
    /// File content length in bytes.
    pub data_len: u32,
    /// Offset of file data from the start of the bootfs payload.
    pub data_off: u32,
}

// ZBI type constants
/// Container item type -- wraps all other items.
pub const ZBI_TYPE_CONTAINER: u32 = 0x544f4f42; // "BOOT"
/// Bootfs filesystem image.
pub const ZBI_TYPE_STORAGE_BOOTFS: u32 = 0x42534642; // "BFSB"
/// Kernel command line (NUL-terminated string).
pub const ZBI_TYPE_CMDLINE: u32 = 0x4c444d43; // "CMDL"

// Magic constants
/// Container header `extra` field value.
pub const ZBI_CONTAINER_MAGIC: u32 = 0x868c_f7e6;
/// Every item header `magic` field value.
pub const ZBI_ITEM_MAGIC: u32 = 0xb578_1729;
/// CRC32 field value when CRC is not computed.
pub const ZBI_ITEM_NO_CRC32: u32 = 0x4a87_e8d6;
/// Bootfs header magic.
pub const ZBI_BOOTFS_MAGIC: u32 = 0xa56d_3ff9;

// Flags
/// Version flag -- must be set on all items.
pub const ZBI_FLAG_VERSION: u32 = 0x0001_0000;

impl ZbiHeader {
    /// Size of the ZBI header in bytes.
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// Payload length rounded up to 8-byte alignment (for finding the next item).
    pub fn padded_length(&self) -> usize {
        (self.length as usize + 7) & !7
    }
}

impl ZbiBootfsDirent {
    /// Size of the fixed part of the dirent (excluding the name).
    pub const FIXED_SIZE: usize = core::mem::size_of::<Self>();

    /// Total size of this dirent including the name, padded to 4-byte alignment.
    pub fn total_size(&self) -> usize {
        (Self::FIXED_SIZE + self.name_len as usize + 3) & !3
    }
}

/// Find the first bootfs file in a ZBI, returning a reference to its data.
///
/// This is a no-alloc version suitable for `#![no_std]` environments.
/// Returns `(filename, data)` or `None` if no bootfs is found.
pub fn find_first_bootfs_entry(zbi_data: &[u8]) -> Option<(&[u8], &[u8])> {
    if zbi_data.len() < ZbiHeader::SIZE {
        return None;
    }

    let container: ZbiHeader =
        unsafe { core::ptr::read_unaligned(zbi_data.as_ptr() as *const ZbiHeader) };
    if container.item_type != ZBI_TYPE_CONTAINER || container.magic != ZBI_ITEM_MAGIC {
        return None;
    }
    if container.extra != ZBI_CONTAINER_MAGIC {
        return None;
    }

    let container_end = (ZbiHeader::SIZE + container.length as usize).min(zbi_data.len());
    let mut offset = ZbiHeader::SIZE;

    while offset + ZbiHeader::SIZE <= container_end {
        let item: ZbiHeader =
            unsafe { core::ptr::read_unaligned(zbi_data.as_ptr().add(offset) as *const ZbiHeader) };
        if item.magic != ZBI_ITEM_MAGIC {
            break;
        }

        let payload_start = offset + ZbiHeader::SIZE;
        let payload_end = payload_start + item.length as usize;
        // Validate against both container end and buffer length
        if payload_end > container_end || payload_end > zbi_data.len() {
            break;
        }

        if item.item_type == ZBI_TYPE_STORAGE_BOOTFS {
            let bootfs = &zbi_data[payload_start..payload_end];
            return find_first_file_in_bootfs(bootfs);
        }

        offset = payload_start + item.padded_length();
    }

    None
}

/// Find the first file in a bootfs image, returning references to name and data.
fn find_first_file_in_bootfs(bootfs: &[u8]) -> Option<(&[u8], &[u8])> {
    let bfs_hdr_size = core::mem::size_of::<ZbiBootfsHeader>();
    if bootfs.len() < bfs_hdr_size {
        return None;
    }

    let header: ZbiBootfsHeader =
        unsafe { core::ptr::read_unaligned(bootfs.as_ptr() as *const ZbiBootfsHeader) };
    if header.magic != ZBI_BOOTFS_MAGIC {
        return None;
    }

    let dirent_start = bfs_hdr_size;
    let dir_end = bfs_hdr_size + header.dirsize as usize;
    if dirent_start + ZbiBootfsDirent::FIXED_SIZE > dir_end.min(bootfs.len()) {
        return None;
    }

    let dirent: ZbiBootfsDirent = unsafe {
        core::ptr::read_unaligned(bootfs.as_ptr().add(dirent_start) as *const ZbiBootfsDirent)
    };

    let name_start = dirent_start + ZbiBootfsDirent::FIXED_SIZE;
    let name_end = name_start + dirent.name_len as usize;
    if name_end > dir_end.min(bootfs.len()) || dirent.name_len == 0 {
        return None;
    }
    let name = &bootfs[name_start..name_end - 1]; // exclude NUL

    let data_start = dirent.data_off as usize;
    let data_end = data_start + dirent.data_len as usize;
    if data_end > bootfs.len() {
        return None;
    }

    Some((name, &bootfs[data_start..data_end]))
}

#[cfg(feature = "zbi")]
/// Safely convert a repr(C) struct to its byte representation.
fn as_bytes<T: Copy>(val: &T) -> &[u8] {
    unsafe { core::slice::from_raw_parts(val as *const T as *const u8, core::mem::size_of::<T>()) }
}

#[cfg(feature = "zbi")]
/// Build a minimal ZBI containing a single bootfs entry.
///
/// This creates a valid ZBI with:
/// 1. A container header
/// 2. A bootfs item containing a single file
///
/// Used by the kernel to construct a test ZBI when no external
/// ZBI is provided.
pub fn build_test_zbi(filename: &[u8], file_data: &[u8]) -> alloc::vec::Vec<u8> {
    use alloc::vec::Vec;

    // Bootfs dirent: name_len, data_len, data_off, then name bytes
    let name_len = filename.len() as u32 + 1; // include NUL
    let dirent_size = ((ZbiBootfsDirent::FIXED_SIZE + name_len as usize) + 3) & !3;
    let dirsize = dirent_size as u32;

    // Data starts after the bootfs header + directory, page-aligned
    let bootfs_header_size = core::mem::size_of::<ZbiBootfsHeader>();
    let data_off = ((bootfs_header_size + dirent_size + 4095) & !4095) as u32;
    let bootfs_payload_len = data_off as usize + file_data.len();

    // Build bootfs payload
    let mut bootfs = Vec::with_capacity(bootfs_payload_len);

    // Bootfs header
    let bfs_hdr = ZbiBootfsHeader {
        magic: ZBI_BOOTFS_MAGIC,
        dirsize,
    };
    bootfs.extend_from_slice(as_bytes(&bfs_hdr));

    // Directory entry
    let dirent = ZbiBootfsDirent {
        name_len,
        data_len: file_data.len() as u32,
        data_off,
    };
    bootfs.extend_from_slice(as_bytes(&dirent));
    bootfs.extend_from_slice(filename);
    bootfs.push(0); // NUL terminator
                    // Pad dirent to 4-byte alignment
    while bootfs.len() % 4 != 0 {
        bootfs.push(0);
    }

    // Pad to data_off
    bootfs.resize(data_off as usize, 0);

    // File data
    bootfs.extend_from_slice(file_data);

    // Build the full ZBI
    let bootfs_padded = (bootfs.len() + 7) & !7;
    let container_payload_len = ZbiHeader::SIZE + bootfs_padded;
    let mut zbi = Vec::with_capacity(ZbiHeader::SIZE + container_payload_len);

    // Container header
    let container = ZbiHeader {
        item_type: ZBI_TYPE_CONTAINER,
        length: container_payload_len as u32,
        extra: ZBI_CONTAINER_MAGIC,
        flags: ZBI_FLAG_VERSION,
        reserved0: 0,
        reserved1: 0,
        magic: ZBI_ITEM_MAGIC,
        crc32: ZBI_ITEM_NO_CRC32,
    };
    zbi.extend_from_slice(as_bytes(&container));

    // Bootfs item header
    let bootfs_item = ZbiHeader {
        item_type: ZBI_TYPE_STORAGE_BOOTFS,
        length: bootfs.len() as u32,
        extra: 0,
        flags: ZBI_FLAG_VERSION,
        reserved0: 0,
        reserved1: 0,
        magic: ZBI_ITEM_MAGIC,
        crc32: ZBI_ITEM_NO_CRC32,
    };
    zbi.extend_from_slice(as_bytes(&bootfs_item));

    // Bootfs payload
    zbi.extend_from_slice(&bootfs);
    // Pad to 8-byte alignment
    while zbi.len() % 8 != 0 {
        zbi.push(0);
    }

    zbi
}
