// May need move to drivers
use smoltcp::{
    iface::{InterfaceBuilder, NeighborCache, Route, Routes},
    phy::{Loopback, Medium},
    wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address},
};

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

// use zcore_drivers::net::get_sockets;
use alloc::sync::Arc;

use alloc::string::String;
use lock::Mutex;

use crate::drivers::add_device;
use crate::drivers::all_net;
use zcore_drivers::net::LoopbackInterface;
use zcore_drivers::scheme::NetScheme;
use zcore_drivers::Device;

pub fn init() {
    let name = String::from("loopback");
    warn!("name : {}", name);
    // Initialize a network stack.
    // Accept configuration parameters from the caller; use defaults if none are provided.

    // Network device.
    // Default: loopback.
    let loopback = Loopback::new(Medium::Ethernet);

    // Assign network identity to the device.

    // MAC address.
    let mac: [u8; 6] = [0x52, 0x54, 0x98, 0x76, 0x54, 0x32];
    let ethernet_addr = EthernetAddress::from_bytes(&mac);
    // IP address.
    let ip_addrs = [IpCidr::new(IpAddress::v4(127, 0, 0, 1), 24)];
    // qemu
    // let ip_addrs = [IpCidr::new(IpAddress::v4(10, 0, 2, 15), 24)];
    // Routing.
    let default_gateway = Ipv4Address::new(127, 0, 0, 1);
    // qemu route
    // let default_gateway = Ipv4Address::new(10, 0, 2, 2);
    static mut ROUTES_STORAGE: [Option<(IpCidr, Route)>; 1] = [None; 1];
    let mut routes = unsafe { Routes::new(&mut ROUTES_STORAGE[..]) };
    routes.add_default_ipv4_route(default_gateway).unwrap();
    // ARP cache.
    let neighbor_cache = NeighborCache::new(BTreeMap::new());

    // Configure and build the network interface.
    let iface = InterfaceBuilder::new(loopback)
        .ethernet_addr(ethernet_addr)
        .ip_addrs(ip_addrs)
        .routes(routes)
        .neighbor_cache(neighbor_cache)
        .finalize();

    let loopback_iface = LoopbackInterface {
        iface: Arc::new(Mutex::new(iface)),
        name,
    };
    // loopback_iface
    let dev = Device::Net(Arc::new(loopback_iface));
    add_device(dev);
}

pub fn get_net_device() -> Vec<Arc<dyn NetScheme>> {
    all_net().as_vec().clone()
}
