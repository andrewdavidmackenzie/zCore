use super::*;

#[derive(Default)]
pub struct VirtualInterrupt {}

impl VirtualInterrupt {
    pub fn new() -> Box<Self> {
        Default::default()
    }
}

impl InterruptTrait for VirtualInterrupt {
    fn mask(&self) -> ZxResult {
        Ok(())
    }
    fn unmask(&self) -> ZxResult {
        Ok(())
    }
    fn register_handler(&self, _handle: Box<dyn Fn() + Send + Sync>) -> ZxResult {
        Ok(())
    }
    fn unregister_handler(&self) -> ZxResult {
        Ok(())
    }
}
