use std::net::{SocketAddr};

#[allow(dead_code)]
pub struct LocalAddress {
    interface: String,
    internal_address: SocketAddr,
    external_address: Option<SocketAddr>,
}

impl LocalAddress {
    pub fn new<S: Into<String>>(interface: S, internal_address: SocketAddr, external_address: Option<SocketAddr>) -> LocalAddress {
        LocalAddress {
            interface: interface.into(),
            internal_address: internal_address,
            external_address: external_address,
        }
    }
}

