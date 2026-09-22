use alloc::boxed::Box;
use alloc::format;

use ::drivers::builder::{DevicetreeDriverBuilder, IoMapper};
use ::drivers::irq::riscv::ScauseIntCode;

use ::drivers::{Device, DeviceResult};

use crate::common::vm::GenericPageTable;
use crate::{mem::phys_to_virt, CachePolicy, MMUFlags, PhysAddr, VirtAddr};

struct IoMapperImpl;

impl IoMapper for IoMapperImpl {
    fn query_or_map(&self, paddr: PhysAddr, size: usize) -> Option<VirtAddr> {
        let vaddr = if paddr > (1 << 39) {
            // To retrieve avaliable sv39 vaddr
            paddr | (0x1ffffff << 39)
        } else {
            phys_to_virt(paddr)
        };
        let mut pt = super::vm::kernel_page_table().lock();
        if let Ok((paddr_mapped, _, _)) = pt.query(vaddr) {
            if paddr_mapped == paddr {
                Some(vaddr)
            } else {
                warn!(
                    "IoMapper::query_or_map: not linear mapping: vaddr={:#x}, paddr={:#x}",
                    vaddr, paddr_mapped
                );
                None
            }
        } else {
            let size = crate::addr::align_up(size);
            let flags = MMUFlags::READ
                | MMUFlags::WRITE
                | MMUFlags::HUGE_PAGE
                | MMUFlags::DEVICE
                | MMUFlags::from_bits_truncate(CachePolicy::UncachedDevice as usize);
            if let Err(err) = pt.map_cont(vaddr, size, paddr, flags) {
                warn!(
                    "IoMapper::query_or_map: failed to map {:#x?} => {:#x}, flags={:?}: {:?}",
                    vaddr..vaddr + size,
                    paddr,
                    flags,
                    err
                );
                None
            } else {
                Some(vaddr)
            }
        }
    }
}

/// Initialize device drivers.
pub(super) fn init() -> DeviceResult {
    // prase DTB and probe devices
    let dev_list =
        DevicetreeDriverBuilder::new(phys_to_virt(crate::KCONFIG.dtb_paddr), IoMapperImpl)?
            .build()?;
    // add drivers
    for dev in dev_list.into_iter() {
        if let Device::Uart(uart) = dev {
            // Wire UART received bytes to the shared console input buffer.
            #[allow(unused_imports)]
            use ::drivers::scheme::{EventScheme, UartScheme};
            let u = uart.clone();
            uart.subscribe(
                Box::new(move |_| {
                    while let Some(c) = u.try_recv().unwrap_or(None) {
                        let c = if c == b'\r' { b'\n' } else { c };
                        crate::common::console::console_input_push(c);
                    }
                }),
                false,
            );
            crate::device_registry::add_device(Device::Uart(uart));
        } else {
            crate::device_registry::add_device(dev);
        }
    }

    #[cfg(feature = "pci")]
    {
        use ::drivers::bus::pci;
        use alloc::sync::Arc;
        let pci_devs = pci::init(Some(Arc::new(IoMapperImpl)))?;
        for d in pci_devs.into_iter() {
            crate::device_registry::add_device(d);
        }
    }

    intc_init()?;

    Ok(())
}

pub(super) fn intc_init() -> DeviceResult {
    let irq = crate::device_registry::all_irq()
        .find(format!("riscv-intc-cpu{}", crate::cpu::cpu_id()).as_str())
        .expect("IRQ device 'riscv-intc' not initialized!");
    // register soft interrupts handler
    irq.register_handler(
        ScauseIntCode::SupervisorSoft as _,
        Box::new(super::trap::super_soft),
    )?;
    // register timer interrupts handler
    irq.register_handler(
        ScauseIntCode::SupervisorTimer as _,
        Box::new(super::trap::super_timer),
    )?;
    irq.unmask(ScauseIntCode::SupervisorSoft as _)?;
    irq.unmask(ScauseIntCode::SupervisorTimer as _)?;

    Ok(())
}
