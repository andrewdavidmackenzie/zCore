use x2apic::lapic::{
    xapic_base, LocalApic as LocalApicInner, LocalApicBuilder, TimerDivide, TimerMode,
};

use super::{consts, Phys2VirtFn};

static mut LOCAL_APIC: Option<LocalApic> = None;
static mut BSP_ID: Option<u8> = None;

pub struct LocalApic {
    inner: LocalApicInner,
}

impl LocalApic {
    /// # Safety
    /// Caller must ensure no other references to the APIC exist.
    /// In practice, this is only called from interrupt-disabled contexts.
    pub unsafe fn get<'a>() -> &'a mut LocalApic {
        unsafe {
            (*core::ptr::addr_of_mut!(LOCAL_APIC))
                .as_mut()
                .expect("Local APIC is not initialized by BSP")
        }
    }

    pub unsafe fn init_bsp(phys_to_virt: Phys2VirtFn) {
        let base_vaddr = phys_to_virt(xapic_base() as usize);
        let mut inner = LocalApicBuilder::new()
            .timer_vector(consts::X86_INT_APIC_TIMER)
            .error_vector(consts::X86_INT_APIC_ERROR)
            .spurious_vector(consts::X86_INT_APIC_SPURIOUS)
            .set_xapic_base(base_vaddr as u64)
            .build()
            .unwrap_or_else(|err| panic!("{}", err));
        inner.enable();

        assert!(inner.is_bsp());
        unsafe {
            *core::ptr::addr_of_mut!(BSP_ID) = Some((inner.id() >> 24) as u8);
            *core::ptr::addr_of_mut!(LOCAL_APIC) = Some(LocalApic { inner });
        }
    }

    pub unsafe fn init_ap() {
        Self::get().inner.enable();
    }

    pub fn bsp_id() -> u8 {
        unsafe { (*core::ptr::addr_of!(BSP_ID)).unwrap() }
    }

    pub fn id(&mut self) -> u8 {
        unsafe { (self.inner.id() >> 24) as u8 }
    }

    pub fn eoi(&mut self) {
        unsafe { self.inner.end_of_interrupt() }
    }

    pub fn disable_timer(&mut self) {
        unsafe { self.inner.disable_timer() }
    }

    pub fn enable_timer(&mut self) {
        unsafe { self.inner.enable_timer() }
    }

    pub fn set_timer_mode(&mut self, mode: TimerMode) {
        unsafe { self.inner.set_timer_mode(mode) }
    }

    pub fn set_timer_divide(&mut self, divide: TimerDivide) {
        unsafe { self.inner.set_timer_divide(divide) }
    }

    pub fn set_timer_initial(&mut self, initial: u32) {
        unsafe { self.inner.set_timer_initial(initial) }
    }

    /// Send an INIT IPI to the specified APIC ID.
    pub fn send_init_ipi(&mut self, apic_id: u32) {
        unsafe { self.inner.send_init_ipi(apic_id) }
    }

    /// Send an INIT IPI to all other processors.
    pub fn send_init_ipi_all(&mut self) {
        unsafe { self.inner.send_init_ipi_all() }
    }

    /// Send a Startup IPI (SIPI) to the specified APIC ID.
    /// `vector` is the page number of the trampoline code (physical address >> 12).
    pub fn send_sipi(&mut self, vector: u8, apic_id: u32) {
        unsafe { self.inner.send_sipi(vector, apic_id) }
    }

    /// Send a Startup IPI to all other processors.
    pub fn send_sipi_all(&mut self, vector: u8) {
        unsafe { self.inner.send_sipi_all(vector) }
    }

    /// Send a fixed IPI with the given vector to the specified APIC ID.
    pub fn send_ipi(&mut self, vector: u8, apic_id: u32) {
        unsafe { self.inner.send_ipi(vector, apic_id) }
    }
}
