pub use crate::arch::ContextData;

#[derive(Debug, Default)]
pub struct Context {
    context: usize,
}

impl Context {
    pub fn set_context(&mut self, addr: usize) {
        self.context = addr;
    }

    pub fn get_context_data(&self) -> &ContextData {
        unsafe {
            let context = self.context as *const ContextData;
            &*context
        }
    }

    /// Returns the raw context pointer/address.
    ///
    /// On x86_64, returns the address of the `context` field (because
    /// ContextData is pushed onto the stack and the switch assembly
    /// expects a pointer-to-pointer). On other arches, returns the
    /// stored context address directly.
    pub fn get_context(&self) -> usize {
        #[cfg(target_arch = "x86_64")]
        {
            (&self.context) as *const usize as _
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            self.context
        }
    }

    /// Returns the stack pointer.
    ///
    /// On x86_64 and aarch64, the context value IS the stack pointer.
    /// On riscv64, it's read from the ContextData.
    pub fn get_sp(&self) -> usize {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        {
            self.context
        }
        #[cfg(target_arch = "riscv64")]
        {
            self.get_context_data().sp()
        }
    }

    /// Returns the program counter.
    pub fn get_pc(&self) -> usize {
        self.get_context_data().pc()
    }

    /// Returns the page table base register.
    pub fn get_pgbr(&self) -> usize {
        self.get_context_data().pgbr()
    }
}
