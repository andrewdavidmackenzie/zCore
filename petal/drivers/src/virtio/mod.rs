//! Packaging of [`virtio-drivers` library](https://github.com/rcore-os/virtio-drivers).

mod blk;
mod console;
mod gpu;
mod input;

pub use blk::VirtIoBlk;
pub use console::VirtIoConsole;
pub use gpu::VirtIoGpu;
pub use input::VirtIoInput;
pub use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};

use crate::DeviceError;
use core::convert::From;
use virtio_drivers::Error;

impl From<Error> for DeviceError {
    fn from(err: Error) -> Self {
        match err {
            Error::QueueFull => Self::NotReady,
            Error::NotReady => Self::NotReady,
            Error::WrongToken => Self::IoError,
            Error::AlreadyUsed => Self::AlreadyExists,
            Error::InvalidParam => Self::InvalidParam,
            Error::DmaError => Self::DmaError,
            Error::IoError => Self::IoError,
            Error::Unsupported => Self::NotSupported,
            Error::ConfigSpaceTooSmall => Self::InvalidParam,
            Error::ConfigSpaceMissing => Self::InvalidParam,
            Error::SocketDeviceError(_) => Self::IoError,
        }
    }
}

// -- HAL implementation for virtio-drivers -----------------------------------
//
// The new virtio-drivers (0.4+) expects a type that implements `unsafe trait Hal`
// instead of the old `#[no_mangle] extern "C"` FFI functions.
//
// We define HalImpl here in the `drivers` crate (where the virtio device types
// live) and delegate to the FFI functions that `kernel-hal` still provides.
// This avoids a circular dependency between `drivers` and `kernel-hal`.

use core::ptr::NonNull;
use virtio_drivers::{BufferDirection, Hal, PhysAddr};

type VirtAddr = usize;

extern "C" {
    fn virtio_dma_alloc(pages: usize) -> PhysAddr;
    fn virtio_dma_dealloc(paddr: PhysAddr, pages: usize) -> i32;
    fn virtio_phys_to_virt(paddr: PhysAddr) -> VirtAddr;
    fn virtio_virt_to_phys(vaddr: VirtAddr) -> PhysAddr;
}

/// HAL implementation that delegates to the `#[no_mangle]` FFI functions
/// provided by `kernel-hal`.
pub enum HalImpl {}

unsafe impl Hal for HalImpl {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        let paddr = unsafe { virtio_dma_alloc(pages) };
        let vaddr = unsafe { virtio_phys_to_virt(paddr) };
        let ptr = NonNull::new(vaddr as *mut u8).expect("virtio_phys_to_virt returned null");
        (paddr, ptr)
    }

    unsafe fn dma_dealloc(paddr: PhysAddr, _vaddr: NonNull<u8>, pages: usize) -> i32 {
        virtio_dma_dealloc(paddr, pages)
    }

    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, _size: usize) -> NonNull<u8> {
        let vaddr = virtio_phys_to_virt(paddr);
        NonNull::new(vaddr as *mut u8).expect("virtio_phys_to_virt returned null for MMIO")
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> PhysAddr {
        // Identity mapping: just convert virtual to physical address.
        virtio_virt_to_phys(buffer.as_ptr() as *const u8 as usize)
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection) {
        // No-op for identity/linear mapping.
    }
}
