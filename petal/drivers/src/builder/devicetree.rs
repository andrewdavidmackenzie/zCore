// Parse the device tree, create known devices, and register interrupts for them.
//
// Devices involving interrupts include:
//
// - Interrupt controllers that receive interrupts
// - Devices that raise interrupts
//
// A valid interrupt controller should have the following three properties:
//
// - `interrupt-controller`: indicates this is an interrupt controller
// - `#interrupt-cells`: specifies how many parameters are needed to register an interrupt with this controller
// - `phandle`: a number used when registering interrupts with this controller; may not exist if no device needs to register with it
//
// Registering interrupts for a device requires the `interrupts_extended` property, which is a `Vec<u32>` in the form `[{phandle, ...,}*]`,
// i.e., a controller reference followed by the number of parameters specified by that controller.
//! Probe devices and create drivers from device tree.
//!
//! Specification: <https://github.com/devicetree-org/devicetree-specification/releases/download/v0.3/devicetree-specification-v0.3.pdf>.

use super::IoMapper;
use crate::{
    utils::devicetree::{
        parse_interrupts, parse_reg, Devicetree, InheritProps, InterruptsProp, Node, StringList,
    },
    Device, DeviceError, DeviceResult, PhysAddr, VirtAddr,
};
use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};

const MODULE: &str = "device-tree";

type DevWithInterrupt = (Device, InterruptsProp);

/// Interrupt controller-specific properties from the device tree
struct IntcProps {
    phandle: u32,
    interrupt_cells: u32,
}

/// Interrupt controller info stored in the lookup table
struct Intc {
    index: usize,
    cells: usize,
}

/// A builder to probe devices and create drivers from device tree.
pub struct DevicetreeDriverBuilder<M: IoMapper> {
    dt: Devicetree,
    io_mapper: M,
}

impl<M: IoMapper> DevicetreeDriverBuilder<M> {
    /// Prepare to parse DTB from the given virtual address.
    pub fn new(dtb_base_vaddr: VirtAddr, io_mapper: M) -> DeviceResult<Self> {
        Ok(Self {
            dt: Devicetree::from(dtb_base_vaddr)?,
            io_mapper,
        })
    }

    /// Parse the device tree from root, and returns an array of [`Device`] it found.
    pub fn build(&self) -> DeviceResult<Vec<Device>> {
        let mut intc_map = BTreeMap::new(); // phandle -> intc
        let mut dev_list = Vec::new(); // devices

        // Enable uart5 for D1
        // Hard-coding is just a temporary solution
        #[cfg(feature = "allwinner")]
        {
            use d1_pac::{ccu::RegisterBlock as Ccu, gpio::RegisterBlock as Gpio, CCU, GPIO};

            let gpio = unsafe { &*(self.mmap(GPIO::PTR as _, 0x1000)? as *const Gpio) };
            let ccu = unsafe { &*(self.mmap(CCU::PTR as _, 0x800)? as *const Ccu) };

            #[rustfmt::skip]
            gpio.pb_cfg0.modify(|r, w| unsafe {
                w.bits(r.bits())
                 .pb0_select().uart2_tx()
                 .pb1_select().uart2_rx()
                 .pb4_select().uart5_tx()
                 .pb5_select().uart5_rx()
            });
            #[rustfmt::skip]
            gpio.pb_cfg1.modify(|r, w| unsafe {
                w.bits(r.bits())
                 .pb8_select().uart0_tx()
                 .pb9_select().uart0_rx()
            });
            #[rustfmt::skip]
            gpio.pd_cfg1.modify(|r, w| unsafe {
                w.bits(r.bits())
                 .pd10_select().uart3_tx()
                 .pd11_select().uart3_rx()
            });
            #[rustfmt::skip]
            ccu.uart_bgr.write(|w| w
                .uart0_rst()   .deassert()
                .uart0_gating().pass()
                .uart2_rst()   .deassert()
                .uart2_gating().pass()
                .uart3_rst()   .deassert()
                .uart3_gating().pass()
                .uart5_rst()   .deassert()
                .uart5_gating().pass()
            );
        }

        // Parse the device tree
        self.dt.walk(&mut |node, comp, props| {
            debug!(
                "{MODULE}: parsing node {:?} with compatible {comp:?}",
                node.name
            );
            // parse interrupt controller
            let res = if node.has_prop("interrupt-controller") {
                self.parse_intc(node, comp, props).map(|(dev, intc)| {
                    intc_map.insert(
                        intc.phandle,
                        Intc {
                            index: dev_list.len(),
                            cells: intc.interrupt_cells as _,
                        },
                    );
                    dev
                })
            } else {
                // parse other device
                match comp {
                    #[cfg(feature = "virtio")]
                    c if c.contains("virtio,mmio") => self.parse_virtio(node, props),
                    #[cfg(not(feature = "loopback"))]
                    c if c.contains("allwinner,sunxi-gmac") => {
                        self.parse_ethernet(node, comp, props)
                    }
                    c if c.contains("ns16550a") || c.iter().any(|str| str.ends_with("uart")) => {
                        self.parse_uart(node, comp, props)
                    }
                    _ => Err(DeviceError::NotSupported),
                }
            };
            match res {
                Ok(dev) => dev_list.push(dev),
                Err(DeviceError::NotSupported) => {}
                Err(err) => warn!("{MODULE}: failed to parsing node {:?}: {err:?}", node.name),
            }
        });

        // Register interrupts
        for (device, interrupts_extended) in &dev_list {
            let mut extended = interrupts_extended.as_slice();
            // Decompose interrupts_extended
            while let [phandle, irq_num, ..] = extended {
                if let Some(Intc { index, cells }) = intc_map.get(phandle) {
                    let (intc, _) = &dev_list[*index];
                    extended = &extended[1 + cells..];
                    if let Device::Irq(irq) = intc {
                        if *irq_num != 0xffff_ffff {
                            info!("{MODULE}: register interrupts for {intc:?}: {device:?}, irq_num={irq_num}");
                            if irq.register_device(*irq_num as _, device.inner()).is_ok() {
                                irq.unmask(*irq_num as _)?;
                            }
                        }
                    } else {
                        warn!("{MODULE}: node with phandle {phandle:#x} is not an interrupt-controller");
                        return Err(DeviceError::InvalidParam);
                    }
                } else {
                    warn!(
                        "{MODULE}: no such node with phandle {phandle:#x} as the interrupt-parent"
                    );
                    return Err(DeviceError::InvalidParam);
                }
            }
        }

        // Discard interrupt info
        Ok(dev_list.into_iter().map(|(dev, _)| dev).collect())
    }

    fn mmap(&self, phys_addr: PhysAddr, len: usize) -> DeviceResult<VirtAddr> {
        self.io_mapper
            .query_or_map(phys_addr, len)
            .ok_or(DeviceError::NoResources)
    }
}

#[allow(dead_code)]
#[allow(unused_imports)]
#[allow(unused_variables)]
#[allow(unreachable_code)]
impl<M: IoMapper> DevicetreeDriverBuilder<M> {
    /// Parse nodes for interrupt controllers.
    fn parse_intc(
        &self,
        node: &Node,
        comp: &StringList,
        props: &InheritProps,
    ) -> DeviceResult<(DevWithInterrupt, IntcProps)> {
        let phandle = node
            .prop_u32("phandle")
            .map_err(|_| DeviceError::InvalidParam)?;
        let interrupt_cells = node
            .prop_u32("#interrupt-cells")
            .map_err(|_| DeviceError::InvalidParam)?;
        let interrupts_extended = parse_interrupts(node, props)?;
        let base_vaddr =
            parse_reg(node, props).and_then(|(paddr, size)| self.mmap(paddr as _, size as _));
        use crate::irq::*;
        let dev = Device::Irq(match comp {
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            c if c.contains("riscv,cpu-intc") => Arc::new(riscv::Intc::new()),
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            c if c.contains("riscv,plic0") => Arc::new(riscv::Plic::new(base_vaddr?)),
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            c if c.contains("sifive,fu540-c000-plic") => Arc::new(riscv::Plic::new(base_vaddr?)),
            _ => return Err(DeviceError::NotSupported),
        });

        Ok((
            (dev, interrupts_extended),
            IntcProps {
                phandle,
                interrupt_cells,
            },
        ))
    }

    /// Parse nodes for virtio devices over MMIO.
    #[cfg(feature = "virtio")]
    fn parse_virtio(&self, node: &Node, props: &InheritProps) -> DeviceResult<DevWithInterrupt> {
        use core::ptr::NonNull;

        use crate::virtio::*;
        use virtio_drivers::transport::{mmio::MmioTransport, DeviceType, Transport};

        let interrupts_extended = parse_interrupts(node, props)?;
        let base_vaddr =
            parse_reg(node, props).and_then(|(paddr, size)| self.mmap(paddr as _, size as _))?;

        let header =
            NonNull::new(base_vaddr as *mut VirtIOHeader).ok_or(DeviceError::InvalidParam)?;
        let transport =
            unsafe { MmioTransport::new(header) }.map_err(|_| DeviceError::NotSupported)?;

        info!(
            "{MODULE}: detected virtio device: vendor_id={:#X}, type={:?}",
            transport.vendor_id(),
            transport.device_type()
        );

        let dev = match transport.device_type() {
            DeviceType::Block => Device::Block(Arc::new(VirtIoBlk::new(transport)?)),
            DeviceType::GPU => Device::Display(Arc::new(VirtIoGpu::new(transport)?)),
            DeviceType::Input => Device::Input(Arc::new(VirtIoInput::new(transport)?)),
            DeviceType::Console => Device::Uart(Arc::new(VirtIoConsole::new(transport)?)),
            _ => return Err(DeviceError::NotSupported),
        };

        Ok((dev, interrupts_extended))
    }

    /// Parse nodes for Ethernet devices.
    fn parse_ethernet(
        &self,
        node: &Node,
        comp: &StringList,
        props: &InheritProps,
    ) -> DeviceResult<DevWithInterrupt> {
        let interrupts_extended = parse_interrupts(node, props)?;
        let base_vaddr =
            parse_reg(node, props).and_then(|(paddr, size)| self.mmap(paddr as _, size as _));
        info!("Ethernet gmac init ...");

        let irq_num = interrupts_extended[1];
        use crate::net::*;
        let dev = Device::Net(match comp {
            #[cfg(target_arch = "riscv64")]
            c if c.contains("allwinner,sunxi-gmac") => {
                Arc::new(rtlx_init(irq_num as usize, |paddr, size| {
                    self.io_mapper.query_or_map(paddr, size)
                })?)
            }
            _ => return Err(DeviceError::NotSupported),
        });

        Ok((dev, interrupts_extended))
    }

    /// Parse nodes for UART devices.
    fn parse_uart(
        &self,
        node: &Node,
        comp: &StringList,
        props: &InheritProps,
    ) -> DeviceResult<DevWithInterrupt> {
        let interrupts_extended = parse_interrupts(node, props)?;
        let base_vaddr =
            parse_reg(node, props).and_then(|(paddr, size)| self.mmap(paddr as _, size as _))?;

        use crate::uart::*;
        let dev = Device::Uart(match comp {
            c if c.contains("ns16550a") => {
                Arc::new(unsafe { Uart16550Mmio::<u8>::new(base_vaddr) })
            }
            c if c.contains("snps,dw-apb-uart") => {
                Arc::new(unsafe { Uart16550Mmio::<u32>::new(base_vaddr) })
            }
            #[cfg(feature = "allwinner")]
            c if c.contains("allwinner,sun20i-uart") => Arc::new(UartAllwinner::new(base_vaddr)),
            #[cfg(feature = "fu740")]
            c if c.contains("sifive,fu740-c000-uart") => {
                Arc::new(unsafe { UartU740Mmio::<u32>::new(base_vaddr) })
            }
            _ => return Err(DeviceError::NotSupported),
        });

        Ok((dev, interrupts_extended))
    }
}
