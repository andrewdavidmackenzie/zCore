//! ARM GICv3 (Generic Interrupt Controller v3) driver.
//!
//! GICv3 uses system registers (ICC_*_EL1) for the CPU interface instead
//! of MMIO (GICC). The distributor (GICD) and redistributor (GICR) are
//! still MMIO.
//!
//! Reference: ARM GICv3 Architecture Specification (IHI0069)

use crate::prelude::IrqHandler;
use crate::scheme::{IrqScheme, Scheme};
use crate::utils::IrqManager;
use crate::DeviceResult;
use lock::Mutex;

// GICD register offsets (same as GICv2 for most)
const GICD_CTLR: u32 = 0x000;
const GICD_TYPER: u32 = 0x004;
const GICD_IGROUPR: u32 = 0x080;
const GICD_ISENABLER: u32 = 0x100;
const GICD_ICENABLER: u32 = 0x180;
const GICD_IPRIORITYR: u32 = 0x400;
const GICD_ICFGR: u32 = 0xC00;
const GICD_IROUTER: u32 = 0x6100;

// GICD_CTLR bits (affinity routing enabled)
const GICD_CTLR_ARE_NS: u32 = 1 << 4;
const GICD_CTLR_ENABLE_G1A: u32 = 1 << 1;
const GICD_CTLR_ENABLE_G1NS: u32 = 1 << 0;

// GICR register offsets (per-CPU redistributor)
const GICR_WAKER: u32 = 0x014;
const GICR_IGROUPR0: u32 = 0x10080; // SGI_base + 0x80
const GICR_ISENABLER0: u32 = 0x10100; // SGI_base + 0x100
const GICR_ICENABLER0: u32 = 0x10180; // SGI_base + 0x180
const GICR_IPRIORITYR0: u32 = 0x10400; // SGI_base + 0x400

// GICR_WAKER bits
const GICR_WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;

/// GICD MMIO region size (distributor).
pub static GICD_SIZE: usize = 0x1_0000; // 64 KiB

/// GICR MMIO region size (redistributor: RD_base + SGI_base = 128 KiB).
pub static GICR_SIZE: usize = 0x2_0000; // 128 KiB

pub struct IntController {
    gicd_base: usize,
    gicr_base: usize,
    nirqs: u32,
    manager: Mutex<IrqManager<1024>>,
}

unsafe impl Send for IntController {}
unsafe impl Sync for IntController {}

impl IntController {
    pub fn new(gicd_base: usize, gicr_base: usize) -> Self {
        Self {
            gicd_base,
            gicr_base,
            nirqs: 0,
            manager: Mutex::new(IrqManager::new(0..1024)),
        }
    }

    /// Initialize the GICv3 distributor, redistributor, and CPU interface.
    fn init(&mut self) {
        unsafe {
            // --- Enable system register interface ---
            let sre = Self::read_icc_sre_el1();
            Self::write_icc_sre_el1(sre | 0x1); // SRE bit
            core::arch::asm!("isb", options(nostack, preserves_flags));

            // --- Distributor init ---
            // Disable distributor
            self.gicd_write(GICD_CTLR, 0);

            // Read number of interrupt lines
            let typer = self.gicd_read(GICD_TYPER);
            self.nirqs = ((typer & 0x1f) + 1) * 32;

            // Set all SPIs to Group 1 (non-secure)
            for irq in (32..self.nirqs).step_by(32) {
                self.gicd_write(GICD_IGROUPR + (irq / 32) * 4, 0xFFFF_FFFF);
            }

            // Set all SPIs to level-triggered
            for irq in (32..self.nirqs).step_by(16) {
                self.gicd_write(GICD_ICFGR + (irq / 16) * 4, 0);
            }

            // Disable all SPIs
            for irq in (32..self.nirqs).step_by(32) {
                self.gicd_write(GICD_ICENABLER + (irq / 32) * 4, 0xFFFF_FFFF);
            }

            // Set all SPI priorities to default (0xA0)
            for irq in (32..self.nirqs).step_by(4) {
                self.gicd_write(GICD_IPRIORITYR + irq, 0xA0A0_A0A0);
            }

            // Route all SPIs to this CPU (affinity from MPIDR)
            let mpidr: u64;
            core::arch::asm!("mrs {}, mpidr_el1", out(reg) mpidr, options(nostack));
            let affinity = (mpidr & 0xFF)
                | ((mpidr >> 8) & 0xFF) << 8
                | ((mpidr >> 16) & 0xFF) << 16
                | ((mpidr >> 32) & 0xFF) << 32;
            for irq in 32..self.nirqs {
                let offset = GICD_IROUTER + irq * 8;
                let addr = (self.gicd_base + offset as usize) as *mut u64;
                core::ptr::write_volatile(addr, affinity);
            }

            // Enable distributor with ARE, Group 1A, Group 1NS
            self.gicd_write(
                GICD_CTLR,
                GICD_CTLR_ARE_NS | GICD_CTLR_ENABLE_G1A | GICD_CTLR_ENABLE_G1NS,
            );

            // --- Redistributor init ---
            // Wake up the redistributor
            let waker = self.gicr_read(GICR_WAKER);
            self.gicr_write(GICR_WAKER, waker & !GICR_WAKER_PROCESSOR_SLEEP);
            // Wait for ChildrenAsleep to clear
            for _ in 0..1_000_000 {
                if self.gicr_read(GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP == 0 {
                    break;
                }
            }

            // Set SGIs/PPIs (0-31) to Group 1
            self.gicr_write(GICR_IGROUPR0, 0xFFFF_FFFF);

            // Enable all SGIs/PPIs
            self.gicr_write(GICR_ISENABLER0, 0xFFFF_FFFF);

            // Set SGI/PPI priorities to default
            for i in (0..32).step_by(4) {
                self.gicr_write(GICR_IPRIORITYR0 + i * 4 / 4, 0xA0A0_A0A0);
            }

            // --- CPU interface init (system registers) ---
            // Set priority mask to allow all
            Self::write_icc_pmr_el1(0xFF);

            // Enable Group 1 interrupts
            Self::write_icc_igrpen1_el1(1);

            // Binary point register = 0 (no preemption grouping)
            Self::write_icc_bpr1_el1(0);

            core::arch::asm!("isb", options(nostack, preserves_flags));
        }
    }

    pub fn irq_enable(&self, irq: u32) {
        unsafe {
            if irq < 32 {
                // SGI/PPI: use redistributor
                self.gicr_write(GICR_ISENABLER0, 1 << irq);
            } else {
                // SPI: use distributor
                let offset = GICD_ISENABLER + (irq / 32) * 4;
                self.gicd_write(offset, 1 << (irq % 32));
            }
        }
    }

    pub fn irq_disable(&self, irq: u32) {
        unsafe {
            if irq < 32 {
                self.gicr_write(GICR_ICENABLER0, 1 << irq);
            } else {
                let offset = GICD_ICENABLER + (irq / 32) * 4;
                self.gicd_write(offset, 1 << (irq % 32));
            }
        }
    }

    /// Acknowledge interrupt (read IAR).
    pub fn pending_irq(&self) -> usize {
        let iar = Self::read_icc_iar1_el1();
        if iar >= 1020 {
            usize::MAX // spurious
        } else {
            iar as usize
        }
    }

    /// End of interrupt (write EOIR).
    pub fn irq_eoi(&self, irq: u32) {
        Self::write_icc_eoir1_el1(irq);
        unsafe {
            core::arch::asm!("isb", options(nostack, preserves_flags));
        }
    }

    // --- GICD MMIO helpers ---
    unsafe fn gicd_read(&self, offset: u32) -> u32 {
        core::ptr::read_volatile((self.gicd_base + offset as usize) as *const u32)
    }

    unsafe fn gicd_write(&self, offset: u32, value: u32) {
        core::ptr::write_volatile((self.gicd_base + offset as usize) as *mut u32, value);
    }

    // --- GICR MMIO helpers ---
    unsafe fn gicr_read(&self, offset: u32) -> u32 {
        core::ptr::read_volatile((self.gicr_base + offset as usize) as *const u32)
    }

    unsafe fn gicr_write(&self, offset: u32, value: u32) {
        core::ptr::write_volatile((self.gicr_base + offset as usize) as *mut u32, value);
    }

    // --- ICC system register accessors ---
    #[cfg(target_arch = "aarch64")]
    fn read_icc_sre_el1() -> u64 {
        let val: u64;
        unsafe {
            core::arch::asm!("mrs {}, icc_sre_el1", out(reg) val, options(nostack));
        }
        val
    }

    #[cfg(target_arch = "aarch64")]
    fn write_icc_sre_el1(val: u64) {
        unsafe {
            core::arch::asm!("msr icc_sre_el1, {}", in(reg) val, options(nostack));
        }
    }

    #[cfg(target_arch = "aarch64")]
    fn read_icc_iar1_el1() -> u32 {
        let val: u64;
        unsafe {
            core::arch::asm!("mrs {}, icc_iar1_el1", out(reg) val, options(nostack));
        }
        val as u32
    }

    #[cfg(target_arch = "aarch64")]
    fn write_icc_eoir1_el1(val: u32) {
        unsafe {
            core::arch::asm!("msr icc_eoir1_el1, {}", in(reg) val as u64, options(nostack));
        }
    }

    #[cfg(target_arch = "aarch64")]
    fn write_icc_pmr_el1(val: u32) {
        unsafe {
            core::arch::asm!("msr icc_pmr_el1, {}", in(reg) val as u64, options(nostack));
        }
    }

    #[cfg(target_arch = "aarch64")]
    fn write_icc_igrpen1_el1(val: u32) {
        unsafe {
            core::arch::asm!("msr icc_igrpen1_el1, {}", in(reg) val as u64, options(nostack));
        }
    }

    #[cfg(target_arch = "aarch64")]
    fn write_icc_bpr1_el1(val: u32) {
        unsafe {
            core::arch::asm!("msr icc_bpr1_el1, {}", in(reg) val as u64, options(nostack));
        }
    }

    // Stubs for non-aarch64 (driver won't be used, but allows compilation)
    #[cfg(not(target_arch = "aarch64"))]
    fn read_icc_sre_el1() -> u64 {
        0
    }
    #[cfg(not(target_arch = "aarch64"))]
    fn write_icc_sre_el1(_val: u64) {}
    #[cfg(not(target_arch = "aarch64"))]
    fn read_icc_iar1_el1() -> u32 {
        1023
    }
    #[cfg(not(target_arch = "aarch64"))]
    fn write_icc_eoir1_el1(_val: u32) {}
    #[cfg(not(target_arch = "aarch64"))]
    fn write_icc_pmr_el1(_val: u32) {}
    #[cfg(not(target_arch = "aarch64"))]
    fn write_icc_igrpen1_el1(_val: u32) {}
    #[cfg(not(target_arch = "aarch64"))]
    fn write_icc_bpr1_el1(_val: u32) {}
}

impl Scheme for IntController {
    fn name(&self) -> &str {
        "ARM GICv3 Interrupt Controller"
    }

    fn handle_irq(&self, irq_num: usize) {
        if irq_num != usize::MAX {
            self.manager.lock().handle(irq_num).ok();
        }
        self.irq_eoi(irq_num as u32);
    }
}

impl IrqScheme for IntController {
    fn is_valid_irq(&self, irq_num: usize) -> bool {
        irq_num != usize::MAX
    }

    fn mask(&self, irq_num: usize) -> DeviceResult {
        self.irq_disable(irq_num as u32);
        Ok(())
    }

    fn unmask(&self, irq_num: usize) -> DeviceResult {
        self.irq_enable(irq_num as u32);
        Ok(())
    }

    fn register_handler(&self, irq_num: usize, handler: IrqHandler) -> DeviceResult {
        self.manager
            .lock()
            .register_handler(irq_num, handler)
            .map_err(|irq_num| {
                trace!("Unknown irq_num: {:?}", irq_num);
            })
            .ok();
        Ok(())
    }

    fn unregister(&self, _irq_num: usize) -> DeviceResult {
        todo!()
    }
}

/// Initialize GICv3 and return the controller.
pub fn init(gicd_base: usize, gicr_base: usize) -> IntController {
    let mut controller = IntController::new(gicd_base, gicr_base);
    controller.init();
    controller
}

/// Read the pending IRQ number from ICC_IAR1_EL1.
pub fn get_irq_num(_gicd_base: usize, _gicr_base: usize) -> usize {
    IntController::new(_gicd_base, _gicr_base).pending_irq()
}
