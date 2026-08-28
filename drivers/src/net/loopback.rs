// smoltcp
use smoltcp::{iface::Interface, phy::Loopback, time::Instant};

use crate::net::get_sockets;
use alloc::sync::Arc;

use alloc::string::String;
use lock::Mutex;

use crate::scheme::{NetScheme, Scheme};
use crate::{DeviceError, DeviceResult};

use alloc::vec::Vec;
use smoltcp::wire::EthernetAddress;
use smoltcp::wire::IpCidr;

#[derive(Clone)]
pub struct LoopbackInterface {
    pub iface: Arc<Mutex<Interface>>,
    pub loopback: Arc<Mutex<Loopback>>,
    pub name: String,
}

impl Scheme for LoopbackInterface {
    fn name(&self) -> &str {
        "loopback"
    }

    fn handle_irq(&self, _cause: usize) {}
}

impl NetScheme for LoopbackInterface {
    fn recv(&self, _buf: &mut [u8]) -> DeviceResult<usize> {
        unimplemented!()
    }
    fn send(&self, _buf: &[u8]) -> DeviceResult<usize> {
        unimplemented!()
    }

    fn with_context(&self, f: &mut dyn FnMut(&mut smoltcp::iface::Context)) {
        f(self.iface.lock().context())
    }

    fn poll(&self) -> DeviceResult {
        let timestamp = Instant::from_millis(0);
        let sockets = get_sockets();
        let mut sockets = sockets.lock();
        let mut loopback = self.loopback.lock();
        self.iface
            .lock()
            .poll(timestamp, &mut *loopback, &mut sockets);
        Ok(())
    }

    fn get_mac(&self) -> EthernetAddress {
        match self.iface.lock().hardware_addr() {
            smoltcp::wire::HardwareAddress::Ethernet(addr) => addr,
            _ => panic!("expected Ethernet hardware address"),
        }
    }

    fn get_ifname(&self) -> String {
        self.name.clone()
    }

    fn get_ip_address(&self) -> Vec<IpCidr> {
        Vec::from(self.iface.lock().ip_addrs())
    }
}
