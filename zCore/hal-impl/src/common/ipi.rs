use crate::{config::MAX_CORE_NUM, utils::mpsc_queue::MpscQueue};
use alloc::vec::Vec;

const REASON_SIZE: usize = 16;

pub type IpiEntry = usize;
type IRQueue = MpscQueue<'static, IpiEntry>;

/// Static buffer pool for per-CPU IPI queues.
/// Sized by `MAX_CORE_NUM` (from target config's `cores` field).
static mut IPI_REASON_POOL: [[IpiEntry; REASON_SIZE]; MAX_CORE_NUM] =
    [[0; REASON_SIZE]; MAX_CORE_NUM];

static IPI_QUEUE: spin::Lazy<[IRQueue; MAX_CORE_NUM]> =
    spin::Lazy::new(|| core::array::from_fn(|i| IRQueue::new(unsafe { &mut IPI_REASON_POOL[i] })));

pub(crate) fn ipi_queue(cpuid: usize) -> &'static IRQueue {
    &IPI_QUEUE[cpuid]
}

pub(crate) fn ipi_reason() -> Vec<usize> {
    let idx = crate::cpu::cpu_index();
    let queue = ipi_queue(idx);
    queue.consume_entrys().iter().map(|entry| entry.1).collect()
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IpiReason {
    Invalid,
    MockBlock { block_info: usize },
    TlbShutdown { vpn: usize }, // unused
}

/// usize : 64bit
/// |  type reason : 4bit  |   ipi info : 60bit   |
///
/// MockBlock info : 60bit
/// |  reserved : 60 bit  |
const TYPE_SHIFT: usize = 60;
const TYPE_INVALID: usize = 0x0;
const TYPE_MOCK_BLOCK: usize = 0x1;
const TYPE_TLB_SHUTDOWN: usize = 0x2;

impl From<IpiEntry> for IpiReason {
    fn from(r: IpiEntry) -> Self {
        let ipi_type = r >> TYPE_SHIFT;
        let ipi_info = r & 0x000FFFFFFFFFFFFF;
        match ipi_type {
            TYPE_MOCK_BLOCK => Self::MockBlock {
                block_info: ipi_info,
            },
            TYPE_TLB_SHUTDOWN => Self::TlbShutdown { vpn: ipi_info },
            _ => Self::Invalid,
        }
    }
}

impl From<IpiReason> for IpiEntry {
    fn from(reason: IpiReason) -> Self {
        match reason {
            IpiReason::MockBlock { block_info: info } => (TYPE_MOCK_BLOCK << TYPE_SHIFT) | info,
            IpiReason::TlbShutdown { vpn: info } => (TYPE_TLB_SHUTDOWN << TYPE_SHIFT) | info,
            IpiReason::Invalid => 0,
        }
    }
}
