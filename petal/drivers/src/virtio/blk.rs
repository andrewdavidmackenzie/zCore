use lock::Mutex;
use virtio_drivers::device::blk::VirtIOBlk as InnerDriver;
use virtio_drivers::transport::mmio::MmioTransport;

use super::HalImpl;
use crate::scheme::{BlockScheme, Scheme};
use crate::DeviceResult;

pub struct VirtIoBlk {
    inner: Mutex<InnerDriver<HalImpl, MmioTransport>>,
}

impl VirtIoBlk {
    pub fn new(transport: MmioTransport) -> DeviceResult<Self> {
        Ok(Self {
            inner: Mutex::new(InnerDriver::new(transport)?),
        })
    }
}

impl Scheme for VirtIoBlk {
    fn name(&self) -> &str {
        "virtio-blk"
    }

    fn handle_irq(&self, _irq_num: usize) {
        self.inner.lock().ack_interrupt();
    }
}

impl BlockScheme for VirtIoBlk {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) -> DeviceResult {
        self.inner.lock().read_blocks(block_id, buf)?;
        Ok(())
    }

    fn write_block(&self, block_id: usize, buf: &[u8]) -> DeviceResult {
        self.inner.lock().write_blocks(block_id, buf)?;
        Ok(())
    }

    fn flush(&self) -> DeviceResult {
        Ok(())
    }
}
