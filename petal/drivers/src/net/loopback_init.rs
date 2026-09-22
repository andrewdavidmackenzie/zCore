//! Loopback network interface initialization.
//!
//! Parked here from zCore/hal-impl/src/{bare,libos}/net.rs during the
//! HAL refactor (#279). This code creates a loopback network interface
//! and registers it as a device. It will need adaptation when network
//! support is re-enabled.
//!
//! Original location: zCore/hal-impl/src/bare/net.rs (identical copy
//! also existed at zCore/hal-impl/src/libos/net.rs).
//!
//! Dependencies needed: smoltcp, lock, drivers (for Device enum),
//! and the device registry (add_device/all_net).

// --- Original code below (not compiled) ---

/*
use smoltcp::{
    iface::{Config, Interface},
    phy::{Loopback, Medium},
    wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, Ipv4Address},
};

use alloc::vec::Vec;
use alloc::sync::Arc;
use alloc::string::String;
use lock::Mutex;
use smoltcp::time::Instant;

use crate::net::LoopbackInterface;

pub fn init(add_device_fn: impl FnOnce(/* Device */)) {
    let name = String::from("loopback");

    // Network device: loopback.
    let mut loopback = Loopback::new(Medium::Ethernet);

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

    // Register as a network device.
    // add_device_fn(Device::Net(Arc::new(loopback_iface)));
}
*/
