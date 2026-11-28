use hal_impl::interrupt;
use {super::*, lock::IrqLock};

pub struct EventInterrupt {
    vector: usize,
    inner: IrqLock<EventInterruptInner>,
}

#[derive(Default)]
struct EventInterruptInner {
    register: bool,
}

impl EventInterrupt {
    pub fn new(vector: usize) -> Box<Self> {
        // TODO check vector is a vaild IRQ number
        Box::new(EventInterrupt {
            vector,
            inner: Default::default(),
        })
    }
}

impl InterruptTrait for EventInterrupt {
    fn mask(&self) -> ZxResult {
        let inner = self.inner.lock();
        if inner.register {
            // #538 (b): hal mask failures propagate to the syscall
            // boundary instead of unwrapping into a kernel panic.
            // The vector was validated at registration, so a failure
            // here is kernel-internal.
            interrupt::mask_irq(self.vector).map_err(|_| ZxError::INTERNAL)?;
        }
        Ok(())
    }

    fn unmask(&self) -> ZxResult {
        let inner = self.inner.lock();
        if inner.register {
            // #538 (b): same.
            interrupt::unmask_irq(self.vector).map_err(|_| ZxError::INTERNAL)?;
        }
        Ok(())
    }

    fn register_handler(&self, handle: Box<dyn Fn() + Send + Sync>) -> ZxResult {
        let mut inner = self.inner.lock();
        if inner.register {
            return Err(ZxError::ALREADY_BOUND);
        }
        // #538 (b): a vector the controller rejects is INVALID_ARGS
        // (real Zircon rejects bad vectors at zx_interrupt_create)
        // — the old ALREADY_BOUND mapping lied about the failure.
        if interrupt::register_irq_handler(self.vector, handle).is_ok() {
            inner.register = true;
            Ok(())
        } else {
            Err(ZxError::INVALID_ARGS)
        }
    }

    fn unregister_handler(&self) -> ZxResult {
        let mut inner = self.inner.lock();
        if !inner.register {
            return Ok(());
        }
        if interrupt::unregister_irq_handler(self.vector).is_ok() {
            inner.register = false;
            Ok(())
        } else {
            Err(ZxError::NOT_FOUND)
        } // maybe a better error code?
    }
}
