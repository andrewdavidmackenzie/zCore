use super::{phys_to_virt, PAGE_SIZE};
use crate::builder::IoMapper;
use crate::{Device, DeviceError, DeviceResult};
use alloc::{format, sync::Arc, vec::Vec};
use pci_types::capability::PciCapability;
use pci_types::*;

// ---------- ConfigRegionAccess implementations ----------

/// x86_64 PCI configuration space access via I/O ports 0xCF8/0xCFC.
#[cfg(feature = "apic")]
struct PciAccess;

#[cfg(feature = "apic")]
impl ConfigRegionAccess for PciAccess {
    unsafe fn read(&self, address: PciAddress, offset: u16) -> u32 {
        use x86_64::instructions::port::Port;
        let addr: u32 = 0x8000_0000
            | ((address.bus() as u32) << 16)
            | ((address.device() as u32) << 11)
            | ((address.function() as u32) << 8)
            | ((offset as u32) & 0xFC);
        unsafe {
            Port::new(0xCF8).write(addr);
            Port::new(0xCFC).read()
        }
    }

    unsafe fn write(&self, address: PciAddress, offset: u16, value: u32) {
        use x86_64::instructions::port::Port;
        let addr: u32 = 0x8000_0000
            | ((address.bus() as u32) << 16)
            | ((address.device() as u32) << 11)
            | ((address.function() as u32) << 8)
            | ((offset as u32) & 0xFC);
        unsafe {
            Port::new(0xCF8).write(addr);
            Port::new(0xCFC).write(value);
        }
    }
}

#[cfg(feature = "apic")]
const PCI_BASE: usize = 0;

/// MMIO-based PCI configuration space access (RISC-V, future aarch64).
#[cfg(all(
    any(target_arch = "riscv64", target_arch = "aarch64"),
    not(feature = "apic")
))]
struct PciAccess;

#[cfg(feature = "board_malta")]
const PCI_BASE: usize = 0xbbe00000;

#[cfg(all(target_arch = "riscv64", not(feature = "board_malta")))]
const PCI_BASE: usize = 0x30000000;

// Fallback PCI_BASE for host/libos builds where PCI is enabled but
// won't actually be used at runtime.
#[cfg(not(any(feature = "apic", target_arch = "riscv64", feature = "board_malta")))]
const PCI_BASE: usize = 0;

// Fallback PciAccess for host builds (PCI module compiles but won't run).
#[cfg(not(any(feature = "apic", target_arch = "riscv64", target_arch = "aarch64")))]
struct PciAccess;

#[cfg(not(any(feature = "apic", target_arch = "riscv64", target_arch = "aarch64")))]
impl ConfigRegionAccess for PciAccess {
    unsafe fn read(&self, _address: PciAddress, _offset: u16) -> u32 {
        0xFFFF_FFFF // No device present
    }
    unsafe fn write(&self, _address: PciAddress, _offset: u16, _value: u32) {}
}

#[cfg(all(
    any(target_arch = "riscv64", target_arch = "aarch64"),
    not(feature = "apic")
))]
impl ConfigRegionAccess for PciAccess {
    unsafe fn read(&self, address: PciAddress, offset: u16) -> u32 {
        let addr = phys_to_virt(PCI_BASE)
            + (((address.bus() as usize) << 20)
                | ((address.device() as usize) << 15)
                | ((address.function() as usize) << 12)
                | ((offset as usize) & 0xFFC));
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }

    unsafe fn write(&self, address: PciAddress, offset: u16, value: u32) {
        let addr = phys_to_virt(PCI_BASE)
            + (((address.bus() as usize) << 20)
                | ((address.device() as usize) << 15)
                | ((address.function() as usize) << 12)
                | ((offset as usize) & 0xFFC));
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
    }
}

// ---------- Discovered device info ----------

/// Information about a discovered PCI device.
struct PciDeviceInfo {
    address: PciAddress,
    vendor_id: VendorId,
    device_id: DeviceId,
    base_class: BaseClass,
    sub_class: SubClass,
    interrupt_line: InterruptLine,
    interrupt_pin: InterruptPin,
}

// ---------- Bus scanning ----------

/// Scan the PCI bus and return discovered devices.
fn scan_bus(access: &PciAccess) -> Vec<PciDeviceInfo> {
    let mut devices = Vec::new();
    for bus in 0..=255u8 {
        for device in 0..32u8 {
            for function in 0..8u8 {
                let address = PciAddress::new(0, bus, device, function);
                let header = PciHeader::new(address);
                let (vendor_id, device_id) = header.id(access);
                if vendor_id == 0xFFFF {
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                let (_revision, base_class, sub_class, _interface) =
                    header.revision_and_class(access);
                let (interrupt_pin, interrupt_line) =
                    if let Some(endpoint) = EndpointHeader::from_header(header, access) {
                        endpoint.interrupt(access)
                    } else {
                        (0, 0)
                    };
                devices.push(PciDeviceInfo {
                    address,
                    vendor_id,
                    device_id,
                    base_class,
                    sub_class,
                    interrupt_line,
                    interrupt_pin,
                });
                let header = PciHeader::new(address);
                if function == 0 && !header.has_multiple_functions(access) {
                    break;
                }
            }
        }
    }
    devices
}

// ---------- MSI setup ----------

/// Enable the PCI device and its MSI interrupt.
/// Returns the assigned MSI interrupt number when applicable.
unsafe fn enable_msi(address: PciAddress, access: &PciAccess) -> Option<usize> {
    use pci_types::capability::TriggerMode;

    // 23 and lower are used
    static mut MSI_IRQ: u32 = 23;

    let header = PciHeader::new(address);
    let endpoint = EndpointHeader::from_header(header, access)?;

    let mut msi_found = false;
    let mut assigned_irq = None;

    for capability in endpoint.capabilities(access) {
        if let PciCapability::Msi(msi) = capability {
            unsafe { MSI_IRQ += 1 };
            let irq = unsafe { MSI_IRQ };
            assigned_irq = Some(irq as usize);

            // Configure MSI: target BSP LAPIC, edge-triggered, vector = irq + 32
            msi.set_message_info_lapic(0xfee00000, (irq + 32) as u8, TriggerMode::Edge, access);
            msi.set_enabled(true, access);

            debug!("MSI enabled for {}, interrupt vector {}", address, irq + 32);
            msi_found = true;
        }
    }

    if !msi_found {
        // Enable MEM + bus mastering, set interrupt line
        unsafe {
            access.write(address, 0x04, 0x6);
            access.write(address, 0x3c, 33);
        }
        debug!("MSI not found for {}, using legacy PCI interrupt", address);
    }

    warn!("pci device enable done");
    assigned_irq
}

// ---------- Driver matching ----------

fn init_driver(
    dev: &PciDeviceInfo,
    access: &PciAccess,
    _mapper: &Option<Arc<dyn IoMapper>>,
) -> DeviceResult<Device> {
    let _name = format!(
        "enp{}s{}f{}",
        dev.address.bus(),
        dev.address.device(),
        dev.address.function()
    );
    let header = PciHeader::new(dev.address);

    match (dev.vendor_id, dev.device_id) {
        // e1000 and NVMe drivers removed (see #237)
        (0x8086, 0x10fb) => {
            // 82599ES 10-Gigabit SFI/SFP+ Network Connection
            if let Some(endpoint) = EndpointHeader::from_header(header, access) {
                if let Some(bar) = endpoint.bar(0, access) {
                    let (addr, _len) = bar.unwrap_mem();
                    let irq = unsafe { enable_msi(dev.address, access) };
                    let vaddr = phys_to_virt(addr);
                    info!("Found ixgbe dev {:#x}, irq: {:?}", vaddr, irq);
                    return Err(DeviceError::NotSupported);
                }
            }
        }
        (0x8086, 0x1533) => {
            let header = PciHeader::new(dev.address);
            if let Some(endpoint) = EndpointHeader::from_header(header, access) {
                if let Some(bar) = endpoint.bar(0, access) {
                    let (addr, _len) = bar.unwrap_mem();
                    info!("Intel Corporation I210 Gigabit Network Connection");
                    info!("DEV: {}, BAR0: {:#x}", dev.address, addr);
                    return Err(DeviceError::NotSupported);
                }
            }
        }
        (0x8086, 0x1539) => {
            let header = PciHeader::new(dev.address);
            if let Some(endpoint) = EndpointHeader::from_header(header, access) {
                if let Some(bar) = endpoint.bar(0, access) {
                    let (addr, _len) = bar.unwrap_mem();
                    info!(
                        "Found Intel I211 ethernet controller dev {}, addr: {:x?}",
                        dev.address, addr
                    );
                    return Err(DeviceError::NotSupported);
                }
            }
        }
        _ => {}
    }
    if dev.base_class == 0x01 && dev.sub_class == 0x06 {
        // Mass storage class, SATA subclass
        let header = PciHeader::new(dev.address);
        if let Some(endpoint) = EndpointHeader::from_header(header, access) {
            if let Some(bar) = endpoint.bar(5, access) {
                let (addr, _len) = bar.unwrap_mem();
                info!("Found AHCI dev {} BAR5 {:x?}", dev.address, addr);
                return Err(DeviceError::NotSupported);
            }
        }
    }

    Err(DeviceError::NoResources)
}

pub fn detach_driver(_address: &PciAddress) -> bool {
    false
}

pub fn init(mapper: Option<Arc<dyn IoMapper>>) -> DeviceResult<Vec<Device>> {
    let access = PciAccess;
    let mapper_driver = if let Some(m) = mapper {
        m.query_or_map(PCI_BASE, PAGE_SIZE * 256 * 32 * 8);
        Some(m)
    } else {
        None
    };

    let mut dev_list = Vec::new();
    let devices = scan_bus(&access);
    info!("");
    info!("--------- PCI bus:device:function ---------");
    for dev in &devices {
        info!(
            "pci: {} {:04x}:{:04x} ({} {}) irq: {}:{:?}",
            dev.address,
            dev.vendor_id,
            dev.device_id,
            dev.base_class,
            dev.sub_class,
            dev.interrupt_line,
            dev.interrupt_pin,
        );
        let res = init_driver(dev, &access, &mapper_driver);
        match res {
            Ok(d) => dev_list.push(d),
            Err(e) => warn!(
                "{:?}, failed to initialize PCI device: {:04x}:{:04x}",
                e, dev.vendor_id, dev.device_id
            ),
        }
    }
    info!("---------");
    info!("");

    Ok(dev_list)
}

pub fn find_device(vendor: u16, product: u16) -> Option<PciAddress> {
    let access = PciAccess;
    let devices = scan_bus(&access);
    for dev in &devices {
        if dev.vendor_id == vendor && dev.device_id == product {
            return Some(dev.address);
        }
    }
    None
}

pub fn get_bar0_mem(address: PciAddress) -> Option<(usize, usize)> {
    let access = PciAccess;
    let header = PciHeader::new(address);
    let endpoint = EndpointHeader::from_header(header, &access)?;
    let bar = endpoint.bar(0, &access)?;
    Some(bar.unwrap_mem())
}
