use std::net::SocketAddr;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct LocalAddress {
    pub interface: String,
    pub internal_address: SocketAddr,
    pub external_address: Option<SocketAddr>,
}

impl LocalAddress {
    pub fn new<S: Into<String>>(
        interface: S,
        internal_address: SocketAddr,
        external_address: Option<SocketAddr>,
    ) -> LocalAddress {
        LocalAddress {
            interface: interface.into(),
            internal_address: internal_address,
            external_address: external_address,
        }
    }
}
