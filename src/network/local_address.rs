use std::cmp::min;
use std::fmt;
use std::net::SocketAddrV6;

const INTERFACE_NAME_SIZE: usize = 16;

#[derive(Copy, Clone, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterfaceName([u8; INTERFACE_NAME_SIZE]);

impl fmt::Display for InterfaceName {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", String::from_utf8_lossy(&self.0))
    }
}

impl fmt::Debug for InterfaceName {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt::Display::fmt(self, fmt)
    }
}

impl AsRef<[u8]> for InterfaceName {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl InterfaceName {
    pub fn copy_from_slice(slice: &[u8]) -> InterfaceName {
        let mut name = [0u8; INTERFACE_NAME_SIZE];
        let max = min(slice.len(), INTERFACE_NAME_SIZE as usize);
        name[..max].clone_from_slice(&slice[..max]);
        InterfaceName(name)
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct LocalAddress {
    pub interface: InterfaceName,
    pub address: SocketAddrV6,
}

impl LocalAddress {
    pub fn new<S: AsRef<[u8]>>(interface: S, address: SocketAddrV6) -> LocalAddress {
        LocalAddress {
            interface: InterfaceName::copy_from_slice(interface.as_ref()),
            address: address,
        }
    }
}
