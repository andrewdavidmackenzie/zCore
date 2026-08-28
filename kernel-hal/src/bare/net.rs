// May need move to drivers
use smoltcp::{
    iface::{Config, Interface},
    phy::{Loopback, Medium},
    wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, Ipv4Address},
};

use alloc::vec::Vec;

// use zcore_drivers::net::get_sockets;
use alloc::sync::Arc;

use alloc::string::String;
use lock::Mutex;

use crate::drivers::add_device;
use crate::drivers::all_net;
use smoltcp::time::Instant;
use zcore_drivers::net::LoopbackInterface;
use zcore_drivers::scheme::NetScheme;
use zcore_drivers::Device;

pub fn init() {
    let name = String::from("loopback");
    warn!("name : {}", name);
    // Initialize a network stack with default configuration.

    // Network device.
    // Default: loopback.
    let mut loopback = Loopback::new(Medium::Ethernet);

    // Assign network identity to the device.

    // MAC address.
    let mac: [u8; 6] = [0x52, 0x54, 0x98, 0x76, 0x54, 0x32];
    let ethernet_addr = EthernetAddress::from_bytes(&mac);
    // IP address.
    let ip_addrs = [IpCidr::new(IpAddress::v4(127, 0, 0, 1), 24)];
    // Routing.
    let default_gateway = Ipv4Address::new(127, 0, 0, 1);

    // Configure and build the network interface.
    let config = Config::new(HardwareAddress::Ethernet(ethernet_addr));
    let now = Instant::from_millis(0);
    let mut iface = Interface::new(config, &mut loopback, now);
    iface.update_ip_addrs(|addrs| {
        addrs.push(ip_addrs[0]).unwrap();
    });
    iface
        .routes_mut()
        .add_default_ipv4_route(default_gateway)
        .unwrap();

    let loopback_iface = LoopbackInterface {
        iface: Arc::new(Mutex::new(iface)),
        loopback: Arc::new(Mutex::new(loopback)),
        name,
    };
    // loopback_iface
    let dev = Device::Net(Arc::new(loopback_iface));
    add_device(dev);
}

pub fn get_net_device() -> Vec<Arc<dyn NetScheme>> {
    all_net().as_vec().clone()
}
