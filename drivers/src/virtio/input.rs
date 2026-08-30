use lock::Mutex;
use virtio_drivers::device::input::{InputConfigSelect, VirtIOInput as InnerDriver};
use virtio_drivers::transport::mmio::MmioTransport;

use super::HalImpl;
use crate::prelude::{CapabilityType, InputCapability, InputEvent, InputEventType};
use crate::scheme::{impl_event_scheme, InputScheme, Scheme};
use crate::utils::EventListener;
use crate::DeviceResult;

pub struct VirtIoInput {
    inner: Mutex<InnerDriver<HalImpl, MmioTransport>>,
    listener: EventListener<InputEvent>,
}

impl VirtIoInput {
    pub fn new(transport: MmioTransport) -> DeviceResult<Self> {
        let inner = Mutex::new(InnerDriver::new(transport)?);
        Ok(Self {
            inner,
            listener: EventListener::new(),
        })
    }
}

impl_event_scheme!(VirtIoInput, InputEvent);

impl Scheme for VirtIoInput {
    fn name(&self) -> &str {
        "virtio-input"
    }

    fn handle_irq(&self, _irq_num: usize) {
        let mut inner = self.inner.lock();
        inner.ack_interrupt();
        while let Some(e) = inner.pop_pending_event() {
            if let Ok(event_type) = InputEventType::try_from(e.event_type) {
                self.listener.trigger(InputEvent {
                    event_type,
                    code: e.code,
                    value: e.value as i32,
                });
            }
        }
    }
}

impl InputScheme for VirtIoInput {
    fn capability(&self, cap_type: CapabilityType) -> InputCapability {
        let mut inner = self.inner.lock();
        let mut bitmap = [0u8; 128];
        match cap_type {
            CapabilityType::InputProp => {
                let size = inner.query_config_select(InputConfigSelect::PropBits, 0, &mut bitmap);
                InputCapability::from_bitmap(&bitmap[..size as usize])
            }
            CapabilityType::Event => {
                let mut cap = InputCapability::empty();
                for i in 0..crate::input::input_event_codes::ev::EV_CNT {
                    let size =
                        inner.query_config_select(InputConfigSelect::EvBits, i as u8, &mut bitmap);
                    if size > 0 {
                        cap.set(i);
                    }
                }
                cap
            }
            _ => {
                let size = inner.query_config_select(
                    InputConfigSelect::EvBits,
                    cap_type as u8,
                    &mut bitmap,
                );
                InputCapability::from_bitmap(&bitmap[..size as usize])
            }
        }
    }
}
