use super::Scheme;
use crate::DeviceResult;
use alloc::string::String;
use alloc::vec::Vec;
use smoltcp::iface::Context;
use smoltcp::wire::{EthernetAddress, IpCidr};

pub trait NetScheme: Scheme {
    fn recv(&self, buf: &mut [u8]) -> DeviceResult<usize>;
    fn send(&self, buf: &[u8]) -> DeviceResult<usize>;
    fn get_mac(&self) -> EthernetAddress;
    fn get_ifname(&self) -> String;
    fn get_ip_address(&self) -> Vec<IpCidr>;
    fn poll(&self) -> DeviceResult;
    /// Execute a closure with the interface context (needed for TCP connect).
    /// Uses a boxed closure to keep the trait dyn-compatible.
    fn with_context(&self, f: &mut dyn FnMut(&mut Context));
}
