use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lock::Mutex;

use smoltcp::iface::*;
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use smoltcp::wire::*;

use super::realtek::rtl8211f::{self, RTL8211F};
use super::{timer_now_as_micros, ProviderImpl, PAGE_SIZE};

use crate::net::get_sockets;
use crate::scheme::{NetScheme, Scheme};
use crate::{DeviceError, DeviceResult};

#[derive(Clone)]
pub struct RTLxDriver(Arc<Mutex<RTL8211F<ProviderImpl>>>);

#[derive(Clone)]
pub struct RTLxInterface {
    pub iface: Arc<Mutex<Interface>>,
    pub driver: RTLxDriver,
    pub name: String,
    pub irq: usize,
}

impl Scheme for RTLxInterface {
    fn name(&self) -> &str {
        "rtl8211f"
    }

    fn handle_irq(&self, irq: usize) {
        if irq != self.irq {
            // not ours, skip it
            return;
        }

        let status = self.driver.0.lock().interrupt_status();

        let handle_tx_rx = 3;
        if status == handle_tx_rx {
            let timestamp = Instant::from_micros(timer_now_as_micros() as i64);
            let sockets = get_sockets();
            let mut sockets = sockets.lock();
            let mut driver = self.driver.clone();

            self.driver.0.lock().int_disable();
            let changed = self.iface.lock().poll(timestamp, &mut driver, &mut sockets);
            debug!("nic poll, is changed ?: {}", changed);
            self.driver.0.lock().int_enable();
        }
    }
}

impl NetScheme for RTLxInterface {
    fn get_mac(&self) -> EthernetAddress {
        match self.iface.lock().hardware_addr() {
            HardwareAddress::Ethernet(addr) => addr,
            _ => panic!("expected Ethernet hardware address"),
        }
    }

    fn get_ifname(&self) -> String {
        self.name.clone()
    }

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
        debug!("nic poll, is changed ?: {}", changed);
        Ok(())
    }

    fn recv(&self, buf: &mut [u8]) -> DeviceResult<usize> {
        if self.driver.0.lock().can_recv() {
            let (vec_recv, rxcount) = self.driver.0.lock().geth_recv(1);
            buf.copy_from_slice(&vec_recv);
            Ok(rxcount as usize)
        } else {
            Err(DeviceError::NotReady)
        }
    }

    fn send(&self, data: &[u8]) -> DeviceResult<usize> {
        if self.driver.0.lock().can_send() {
            self.driver.0.lock().geth_send(data).unwrap();
            Ok(data.len())
        } else {
            Err(DeviceError::NotReady)
        }
    }
}

pub struct RTLxRxToken(Vec<u8>);
pub struct RTLxTxToken(RTLxDriver);

impl phy::Device for RTLxDriver {
    type RxToken<'a> = RTLxRxToken;
    type TxToken<'a> = RTLxTxToken;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1536;
        caps.max_burst_size = Some(64);
        caps.medium = Medium::Ethernet;
        caps
    }

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if self.0.lock().can_recv() {
            // Only receive one network packet at a time here
            let (vec_recv, _rxcount) = self.0.lock().geth_recv(1);
            Some((RTLxRxToken(vec_recv), RTLxTxToken(self.clone())))
        } else {
            None
        }
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        if self.0.lock().can_send() {
            Some(RTLxTxToken(self.clone()))
        } else {
            None
        }
    }
}

impl phy::RxToken for RTLxRxToken {
    fn consume<R, F>(mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        f(&mut self.0)
    }
}

impl phy::TxToken for RTLxTxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = [0u8; 1536];
        let result = f(&mut buffer[..len]);
        if true {
            // always send (previously checked result.is_ok() but R is not Result anymore)
            (self.0).0.lock().geth_send(&buffer[..len]).unwrap();
        }
        result
    }
}

pub fn rtlx_init<F: Fn(usize, usize) -> Option<usize>>(
    irq: usize,
    mapper: F,
) -> DeviceResult<RTLxInterface> {
    mapper(rtl8211f::PINCTRL_GPIO_BASE as usize, PAGE_SIZE * 2);
    mapper(rtl8211f::SYS_CFG_BASE as usize, PAGE_SIZE * 2);

    let mut rtl8211f = RTL8211F::<ProviderImpl>::new(&[0u8; 6]);
    let mac = rtl8211f.get_umac();
    // Please plug in the Ethernet cable before starting the D1
    warn!("Please plug in the Ethernet cable");

    rtl8211f.open().unwrap();
    rtl8211f.set_rx_mode();
    rtl8211f.adjust_link().unwrap();

    let mut net_driver = RTLxDriver(Arc::new(Mutex::new(rtl8211f)));

    let ethernet_addr = EthernetAddress::from_bytes(&mac);
    let ip_addrs = [IpCidr::new(IpAddress::v4(192, 168, 0, 123), 24)];
    let default_gateway = Ipv4Address::new(192, 168, 0, 1);

    let mut config = Config::new(HardwareAddress::Ethernet(ethernet_addr));
    config.random_seed = 0x12345678;

    let now = Instant::from_micros(timer_now_as_micros() as i64);
    let mut iface = Interface::new(config, &mut net_driver, now);
    iface.update_ip_addrs(|addrs| {
        addrs.push(ip_addrs[0]).unwrap();
    });
    iface
        .routes_mut()
        .add_default_ipv4_route(default_gateway)
        .unwrap();

    info!("rtl8211f interface up with addr 192.168.0.123/24");
    info!("rtl8211f interface up with route 192.168.0.1/24");
    let rtl8211f_iface = RTLxInterface {
        iface: Arc::new(Mutex::new(iface)),
        driver: net_driver,
        name: String::from("rtl8211f"),
        irq,
    };

    Ok(rtl8211f_iface)
}
