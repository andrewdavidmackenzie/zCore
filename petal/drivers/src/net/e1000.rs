//! Intel PRO/1000 Network Adapter i.e. e1000 network driver
//! Datasheet: <https://www.intel.ca/content/dam/doc/datasheet/82574l-gbe-controller-datasheet.pdf>

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use smoltcp::iface::*;
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use smoltcp::wire::*;

use super::{timer_now_as_micros, ProviderImpl};
use crate::net::get_sockets;
use crate::scheme::{NetScheme, Scheme};
use crate::{DeviceError, DeviceResult};
use isomorphic_drivers::net::ethernet::intel::e1000::E1000;
use isomorphic_drivers::net::ethernet::structs::EthernetAddress as DriverEthernetAddress;
use lock::Mutex;

#[derive(Clone)]
pub struct E1000Driver(Arc<Mutex<E1000<ProviderImpl>>>);

#[derive(Clone)]
pub struct E1000Interface {
    iface: Arc<Mutex<Interface>>,
    driver: E1000Driver,
    name: String,
    irq: usize,
}

impl Scheme for E1000Interface {
    fn name(&self) -> &str {
        "e1000"
    }

    fn handle_irq(&self, irq: usize) {
        if irq != self.irq {
            // not ours, skip it
            return;
        }

        let data = self.driver.0.lock().handle_interrupt();

        if data {
            let timestamp = Instant::from_micros(timer_now_as_micros() as i64);
            let sockets = get_sockets();
            let mut sockets = sockets.lock();
            let mut driver = self.driver.clone();
            if self.iface.lock().poll(timestamp, &mut driver, &mut sockets) {
                info!("e1000 try_handle_interrupt poll: activity");
            }
        }
    }
}

impl NetScheme for E1000Interface {
    fn get_mac(&self) -> EthernetAddress {
        match self.iface.lock().hardware_addr() {
            HardwareAddress::Ethernet(addr) => addr,
            _ => panic!("expected Ethernet hardware address"),
        }
    }

    fn get_ifname(&self) -> String {
        self.name.clone()
    }

    // get ip addresses
    fn get_ip_address(&self) -> Vec<IpCidr> {
        Vec::from(self.iface.lock().ip_addrs())
    }

    fn with_context(&self, f: &mut dyn FnMut(&mut smoltcp::iface::Context)) {
        f(self.iface.lock().context())
    }

    fn poll(&self) -> DeviceResult {
        let timestamp = Instant::from_micros(timer_now_as_micros() as i64);
        let sockets = get_sockets();
        let mut sockets = sockets.lock();
        let mut driver = self.driver.clone();
        let changed = self.iface.lock().poll(timestamp, &mut driver, &mut sockets);
        trace!("e1000 NetScheme poll: {:?}", changed);
        Ok(())
    }

    fn recv(&self, buf: &mut [u8]) -> DeviceResult<usize> {
        if let Some(vec_recv) = self.driver.0.lock().receive() {
            buf.copy_from_slice(&vec_recv);
            Ok(vec_recv.len())
        } else {
            Err(DeviceError::NotReady)
        }
    }

    fn send(&self, data: &[u8]) -> DeviceResult<usize> {
        if self.driver.0.lock().can_send() {
            let mut driver = self.driver.0.lock();
            driver.send(data);
            Ok(data.len())
        } else {
            Err(DeviceError::NotReady)
        }
    }
}

pub struct E1000RxToken(Vec<u8>);
pub struct E1000TxToken(E1000Driver);

impl phy::Device for E1000Driver {
    type RxToken<'a> = E1000RxToken;
    type TxToken<'a> = E1000TxToken;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        self.0
            .lock()
            .receive()
            .map(|vec_recv| (E1000RxToken(vec_recv), E1000TxToken(self.clone())))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        if self.0.lock().can_send() {
            Some(E1000TxToken(self.clone()))
        } else {
            None
        }
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1536;
        caps.max_burst_size = Some(64);
        caps.medium = Medium::Ethernet;
        caps
    }
}

impl phy::RxToken for E1000RxToken {
    fn consume<R, F>(mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        f(&mut self.0)
    }
}

impl phy::TxToken for E1000TxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = [0u8; 1536];
        let result = f(&mut buffer[..len]);

        let mut driver = (self.0).0.lock();
        driver.send(&buffer[..len]);

        result
    }
}

// JudgeDuck-OS/kern/e1000.c
pub fn init(
    name: String,
    irq: usize,
    header: usize,
    size: usize,
    index: usize,
) -> DeviceResult<E1000Interface> {
    info!("Probing e1000 {}", name);

    // randomly generated
    let mac: [u8; 6] = [0x54, 0x51, 0x9F, 0x71, 0xC0, index as u8];

    let e1000 = E1000::new(header, size, DriverEthernetAddress::from_bytes(&mac));

    let mut net_driver = E1000Driver(Arc::new(Mutex::new(e1000)));

    let ethernet_addr = EthernetAddress::from_bytes(&mac);
    let ip_addrs = [IpCidr::new(IpAddress::v4(10, 0, 2, (15 + index) as u8), 24)];
    let default_v4_gw = Ipv4Address::new(10, 0, 2, 2); //Qemu user network gateway: 10.0.2.2

    let mut config = Config::new(HardwareAddress::Ethernet(ethernet_addr));
    config.random_seed = 0x12345678; // deterministic seed for reproducibility

    let now = Instant::from_micros(timer_now_as_micros() as i64);
    let mut iface = Interface::new(config, &mut net_driver, now);
    iface.update_ip_addrs(|addrs| {
        addrs.push(ip_addrs[0]).unwrap();
    });
    iface
        .routes_mut()
        .add_default_ipv4_route(default_v4_gw)
        .unwrap();

    info!(
        "e1000 interface {} up with addr 10.0.2.{}/24",
        name,
        15 + index
    );
    let e1000_iface = E1000Interface {
        iface: Arc::new(Mutex::new(iface)),
        driver: net_driver,
        name,
        irq,
    };

    Ok(e1000_iface)
}
